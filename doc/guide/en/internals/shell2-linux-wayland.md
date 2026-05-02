---
slug: shell2-linux-wayland
title: Shell2 — Linux Wayland
language: en
canonical_slug: shell2-linux-wayland
audience: contributor
maturity: wip
guide_order: null
topic_only: false
short_desc: Linux Wayland shell — wl_surface lifecycle, xdg-shell, libinput integration, and protocol fallbacks.
prerequisites: [shell2-common, shell2-linux-x11]
tracked_files:
  - dll/src/desktop/shell2/linux/wayland/clipboard.rs
  - dll/src/desktop/shell2/linux/wayland/defines.rs
  - dll/src/desktop/shell2/linux/wayland/dlopen.rs
  - dll/src/desktop/shell2/linux/wayland/events.rs
  - dll/src/desktop/shell2/linux/wayland/gl.rs
  - dll/src/desktop/shell2/linux/wayland/menu.rs
  - dll/src/desktop/shell2/linux/wayland/mod.rs
  - dll/src/desktop/shell2/linux/wayland/tooltip.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T00:00:00Z
---

> **WIP** — `xdg_popup` for menus, `text-input v3` IME, and KDE blur
> are wired but several edge cases (popup grabs, fractional scale,
> nested popups) still TODO.

The Wayland backend is `LinuxWindow::Wayland(WaylandWindow)`. Selection
and the shared `LinuxWindow` enum are documented on the
[X11 page](shell2-linux-x11.md) — this page covers what is
Wayland-specific.

The struct is `WaylandWindow` at `linux/wayland/mod.rs:136`. Notable
differences from `X11Window`:

- All globals (`wl_compositor`, `wl_shm`, `wl_seat`, `xdg_wm_base`,
  `wl_subcompositor`, `org_kde_kwin_blur_manager`,
  `zwp_text_input_manager_v3`) are owned as raw `*mut` pointers,
  bound from the registry on startup.
- Wayland uses **surface-local logical coordinates** — there is no
  XY screen address. The window cannot ask for absolute position; the
  compositor decides where it ends up.
- A separate `wl_event_queue` is allocated per window so dispatching
  one window's events does not block on another's.
- Configure/ack handshake replaces ConfigureNotify: the compositor
  sends `xdg_surface::configure(serial)` and the client must reply
  `xdg_surface_ack_configure(serial)` after applying the size.
- `WaylandPopup` (`mod.rs:248`) is a separate struct from
  `WaylandWindow` because xdg_popup is a different surface role from
  xdg_toplevel.

## Loading libwayland-client + libwayland-egl

`Wayland::new` (`wayland/dlopen.rs:22`) loads
`libwayland-client.so.0`, `libwayland-egl.so.1`, and optionally
`libwayland-cursor.so.0` via the same `Library` /
`load_first_available` machinery as X11.

Wayland's variadic `wl_proxy_marshal_constructor` /
`wl_proxy_marshal` are stored as raw `*const c_void` and cast at the
call site — Rust function pointers cannot represent C variadic
signatures.

The protocol interfaces (`wl_compositor_interface`,
`xdg_wm_base_interface`, …) are stored as `wl_interface` *structs*
(not pointers) so they can be passed to `wl_registry_bind` by
reference.

XKB (`libxkbcommon.so.0`) is loaded the same way and re-used from
`super::x11::dlopen::Xkb` — the keyboard translation layer is
identical between X11 and Wayland.

## Render modes

```rust,ignore
enum RenderMode {
    Gpu(gl::GlContext, GlFunctions),
    Cpu(Option<CpuFallbackState>),  // wl_shm pool + buffer + mmap fd
}

struct CpuFallbackState {
    pool: *mut wl_shm_pool,
    buffer: *mut wl_buffer,
    data: *mut u8,
    width: i32, height: i32, stride: i32,
    fd: i32,                      // shm fd kept open until drop
    damage_rects: Vec<LogicalRect>,
}
```

GPU mode uses EGL with `wl_egl_window` as the native window:

1. `wl_egl_window_create(surface, w, h)` produces a native handle.
2. `eglGetDisplay`, `eglInitialize`, `eglBindAPI(EGL_OPENGL_API)`,
   `eglChooseConfig`, `eglCreateContext` (the same EGL flow as X11
   — `wayland/gl.rs` mirrors `x11/gl.rs:22`).
3. `eglCreateWindowSurface(display, config, wl_egl_window, null)`.
4. Frame: `eglSwapBuffers`.

CPU fallback uses POSIX shared memory:

1. `memfd_create("azul-shm", MFD_CLOEXEC)` (with `shm_open` /
   `O_TMPFILE` fallbacks).
2. `ftruncate(fd, stride * height)`, `mmap(NULL, size,
   PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0)`.
3. `wl_shm_create_pool(shm, fd, size)` →
   `wl_shm_pool_create_buffer(pool, 0, w, h, stride,
   WL_SHM_FORMAT_ARGB8888)`.
4. Frame: `cpurender → AzulPixmap → memcpy into mmap → wl_surface_attach
   → wl_surface_damage_per_rect → wl_surface_commit`.

The `damage_rects` are kept on `CpuFallbackState` so the next commit
can call `wl_surface_damage` for each region instead of a full
surface — Wayland compositors use this to skip recompositing
unchanged tiles.

GPU mode also tracks `gpu_damage_rects` on the window for the same
reason — see `wayland/mod.rs:185`.

## Event loop

Wayland is **compositor-driven**: client never reads input directly.
The compositor sends events on the wire, and `wl_display_dispatch_queue`
runs the registered listener callbacks (`wl_pointer_listener`,
`wl_keyboard_listener`, `xdg_surface_listener`, …) which mutate
`WaylandWindow.current_window_state`.

`WaylandWindow::poll_event` (search for it in
`wayland/mod.rs`) drains the queue:

1. `wl_display_dispatch_queue_pending` — non-blocking dispatch.
2. After dispatch, call the unified `process_window_events()` so the
   state diff between `previous_window_state` and
   `current_window_state` fires the correct callbacks.

The idle wait in `run.rs:wait_for_x11_connection_activity` (despite
the name) is shared with Wayland — it `select`s on
`wl_display_get_fd` for the active backend.

The `frame_callback_pending` flag (`mod.rs:202`) gates rendering: the
window only commits a new frame after the compositor's
`wl_callback::done` signals the previous frame is on screen. This
prevents queueing more buffers than the compositor can consume.

## XDG surface lifecycle

```text
wl_compositor.create_surface()       → wl_surface
xdg_wm_base.get_xdg_surface(surface) → xdg_surface
xdg_surface.get_toplevel()           → xdg_toplevel
   ─ configure(serial) ─ from compositor
   ─ size_change_event ─ from compositor on resize
xdg_surface.ack_configure(serial)    ← from us
wl_surface.commit()                  ← from us
   ─ surface mapped ─ window appears
```

`configure` events arrive after the compositor has decided initial
geometry (window manager rules, output scale, restored size). Not
acking promptly causes the compositor to drop the connection.

## XKB keyboard translation

`translate_keysym_to_virtual_keycode` (`wayland/mod.rs:303`) is a
flat match from XKB keysym constants
(`XKB_KEY_Escape = 0xff1b`, `XKB_KEY_space = 0x0020`, etc.) to
`VirtualKeyCode`. The same keysym table is used by X11 — XKB is the
shared abstraction. `events.rs::WaylandKeyboardState` holds the
`xkb_context`, `xkb_keymap`, `xkb_state` and feeds raw key codes
through `xkb_state_key_get_one_sym` before dispatching.

Modifiers track via `xkb_state_update_mask` on every
`wl_keyboard::modifiers` event; the resulting state populates
`KeyboardState.shift_down` / `ctrl_down` / `alt_down` / `super_down`
on the diffed `current_window_state`.

## Pointer events

`PointerState` (`events.rs:39`) tracks the most recent serial,
needed for any subsequent compositor request that requires it
(`xdg_toplevel_move`, `xdg_popup_grab`, cursor surface changes).

Cursor changes: load `wl_cursor_theme` via `wl_cursor_theme_load`,
look up by name (`"left_ptr"`, `"text"`, `"hand2"`, …), call
`wl_pointer_set_cursor(serial, cursor_surface, hotspot_x, hotspot_y)`
with the dedicated `cursor_surface` so the theme bitmap is uploaded
into a `wl_buffer` and shown.

Scroll uses `wl_pointer::axis` (continuous wheel) plus `axis_discrete`
(notched wheel) plus `axis_source` for distinguishing finger / wheel /
continuous; `record_scroll_from_hit_test` collapses this to a unified
`(delta_x, delta_y, source)` tuple.

## IME — text-input v3 with GTK fallback

The struct holds three IME-related slots:

- `text_input_manager: Option<*mut zwp_text_input_manager_v3>` —
  bound from the registry if `zwp_text_input_manager_v3` is advertised.
- `text_input: Option<*mut zwp_text_input_v3>` — created via
  `text_input_manager_get_text_input(seat)` once.
- `gtk_im_context: Option<*mut GtkIMContext>` — fallback when the
  compositor does not implement text-input v3.

`text_input_active` and `text_input_enabled` are gates so that
`enable()` is only called once per focus, and `disable()` is sent
on focus-out. `text_input_pending` (`events::TextInputPendingState`)
buffers preedit / commit string / cursor rectangle pairs that arrive
across multiple events; the v3 protocol delivers them as
`preedit_string` + `commit_string` + `delete_surrounding_text` then
a final `done(serial)` — all four must apply atomically.

The pending state ends up at the standard
`process_text_input(text)` path on `LayoutWindow`, identical to the
X11 / macOS / Win32 input path.

## Tooltips via `wl_subsurface`

`TooltipWindow` (`wayland/tooltip.rs:30`) creates a child surface:

```rust,ignore
let tooltip_surface = wl_compositor_create_surface(compositor);
let tooltip_subsurface =
    wl_subcompositor_get_subsurface(subcompositor, tooltip_surface, parent_surface);
wl_subsurface_set_position(tooltip_subsurface, x, y);
wl_subsurface_set_desync(tooltip_subsurface);
```

Subsurfaces are composited with the parent — they share the parent's
mapped state and stacking. Show/hide flips `wl_surface_attach(buffer)`
vs `wl_surface_attach(NULL)` followed by `wl_surface_commit` on the
parent.

The tooltip body is currently a placeholder rasteriser — solid
rectangles per glyph cell. Real text rendering will route through
the same `cpurender` path used for window content; the constants
`TOOLTIP_CHAR_WIDTH_PX = 7` etc. (`tooltip.rs:17`) will go away
once that lands.

## Menus via `xdg_popup` (planned)

Menu popups currently take the same path as X11 — `wayland/menu.rs:84`
(`create_menu_popup_options`) returns `WindowCreateOptions` for a
generic toplevel. The TODO at `run.rs:1157` documents the missing
piece: when `pending_create.window_state.flags.window_type ==
WindowType::Menu`, `xdg_surface::get_popup` should be called with an
`xdg_positioner` anchored to `MenuLayoutData::trigger_rect` (stashed
in the layout-callback `RefAny` for exactly this purpose).

`MenuLayoutData` (`wayland/menu.rs:28`) carries the trigger rect in
parent-surface logical coordinates so the popup positioner can
anchor relative to the trigger node — Wayland clients have no way
to address absolute screen coordinates, so positioning must be done
through the compositor with this rect.

`WaylandPopup` (`wayland/mod.rs:248`) is the eventual home of the
real popup window. It already includes `xdg_popup`, `xdg_positioner`,
and `parent_surface` fields, plus a `listener_context` boxed for
manual cleanup.

## KDE blur protocol

`org.kde.kwin.blur_manager` and `org.kde.kwin.blur` (`mod.rs:166`)
are bound from the registry on KDE Plasma sessions. When the user
enables `WindowBackgroundMaterial::Blur`, `blur_manager_create(surface)`
returns a per-surface blur object that the compositor uses to blur
whatever is behind the window. `blur::set_region(NULL)` blurs the
entire surface; calling `commit` on the parent surface activates the
effect.

This protocol is KDE-only; GNOME (`mutter`) and other Wayland
compositors ignore the request.

## Multi-monitor — `wl_output`

Wayland delivers outputs through the registry as
`wl_output_interface` globals. Each `wl_output` fires
`geometry`, `mode`, `scale`, `name`, `done` events that
`WaylandWindow::known_outputs` accumulates into `MonitorState`
records (`mod.rs:101`).

`current_outputs` tracks which outputs the surface is currently on
(via `wl_surface::enter` / `leave`). When the active output changes
its scale, `dynamic_selector_context.dpi` and the renderer DPI are
updated and the layout regenerates.

`MonitorState::get_monitor_id` builds a stable `MonitorId` from
`make + model + name + position`, falling back to just `name` when
make/model are unset (some compositors don't populate them).

## D-Bus screensaver inhibit

Same pattern as X11: `screensaver_inhibit_cookie: Option<u32>` plus a
shared `*mut DBusConnection` from `linux/dbus`. The
`org.freedesktop.ScreenSaver.Inhibit` call is sent when
`WindowFlags::keep_screen_awake` flips on; `UnInhibit` on flip-off
or window close.

## Clipboard

`wayland/clipboard.rs` currently delegates to the `x11-clipboard`
crate, which requires XWayland. On a pure Wayland session (no
XWayland), `Clipboard::new()` fails and clipboard ops become no-ops.
A native `wl_data_device` / `zwp_primary_selection_v1` implementation
is the long-term plan — the boilerplate listener wiring would mirror
the IME setup.

## Defines

`wayland/defines.rs` (~700 lines) is the literal Rust translation of
the Wayland protocol XML. Each `wl_*_listener` struct contains
`extern "C" fn` pointers that match the protocol's event signatures.
Adding a new global means:

1. Define the C-ABI struct in `defines.rs`.
2. Add the `wl_interface` constant.
3. Add the `*_add_listener` and any `*_destroy` to `dlopen.rs`.
4. Bind from the registry handler in `mod.rs`.
5. Implement the `extern "C" fn` listener callbacks (these run on
   the dispatch thread — they receive a `*mut c_void` data pointer
   that should be cast to `*mut WaylandWindow`).

## Multi-window

Same `linux/registry.rs` as X11. Wayland uses
`wl_display as u64` as the registry key (`run.rs:1022`) since
Wayland clients have no equivalent of X11 Window IDs.

## Known issues / TODOs

- `xdg_popup` for menus is the most important pending piece — the
  current toplevel-based popups don't grab input correctly and don't
  dismiss on outside click.
- `wp_fractional_scale_v1` for non-integer DPI scaling is not bound.
  Currently scale is rounded to the nearest integer per output.
- Native `wl_data_device` clipboard would remove the XWayland dependency.
- Drag-and-drop (`wl_data_device::start_drag`) is not implemented.

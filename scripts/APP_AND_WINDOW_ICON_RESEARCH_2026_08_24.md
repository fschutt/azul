# Setting window / taskbar / dock / app icons at runtime, from RGBA

**Date:** 2026-08-24. Companion to `MULTIMONITOR_AND_TRAY_RESEARCH_2026_08_24.md`.
**Why:** azul can now render any icon-registry entry to RGBA at any size
(`azul_layout::tray_icon::render_icon_to_rgba`). This is what to feed that into.

## 0. The one table that matters

Four slots, four different pixel conventions. Getting one wrong is the whole
bug class (black boxes, dark fringes, garbled rows).

| Slot | Byte order in memory | Alpha | Rows |
|---|---|---|---|
| Win32 `HICON` (`CreateIconIndirect`, V5 header, `BI_BITFIELDS`, A=`0xFF000000`) | B,G,R,A | **straight** | top-down iff `bV5Height = -h` |
| Win32 tray `hIcon` | B,G,R,A | **straight** | as above |
| Win32 `AlphaBlend` / `UpdateLayeredWindow` / GDI+ PARGB | B,G,R,A | **premultiplied** | — |
| macOS `NSBitmapImageRep` + `AlphaNonpremultiplied` | R,G,B,A | **straight** | top-down |
| X11 `_NET_WM_ICON` | `0xAARRGGBB` packed into **`c_ulong`** | straight (spec silent, de-facto) | top-down |
| Wayland `wl_shm` `ARGB8888` | B,G,R,A | **premultiplied** | top-down |

From one straight-RGBA8 master: swizzle→BGRA (Windows); keep as-is + tag
NonPremultiplied (macOS); pack to `0xAARRGGBB` in `u64` slots (X11); swizzle
→BGRA **and multiply** (Wayland). Only Wayland premultiplies.

## 1. Corrections to assumptions we would otherwise have made

1. **`CreateIconIndirect` wants STRAIGHT alpha**, not premultiplied. The
   "GDI always wants premultiplied" folklore is true for `AlphaBlend`,
   `UpdateLayeredWindow` and GDI+ PARGB — *not* for icons. GLFW, SDL and
   wxWidgets all copy straight. Premultiplying gives dark fringes on AA edges.
2. **The AND mask is REQUIRED even for 32bpp alpha icons, and must be
   computed.** `bit = (alpha < 128)`, 1bpp, MSB-first, rows DWORD-aligned, and
   note the inversion: **1 = transparent**. All-zero (GLFW's `CreateBitmap`
   leaves it undefined; SDL uses all-`0xFF`) survives the common `DI_NORMAL`
   path and is "hideously ugly" wherever `DI_MASK` is consumed — drop shadows,
   some Alt-Tab paths, image lists.
3. **`ICON_SMALL2` is NOT settable.** `WM_SETICON` accepts only `ICON_BIG` and
   `ICON_SMALL`; `ICON_SMALL2` is a `WM_GETICON` query value. The DPI-correct
   mechanism is instead to **handle `WM_GETICON` yourself** — its `lParam`
   carries the DPI being requested — and answer from a `(slot, dpi)` cache.
4. **The EXE icon cannot be changed at runtime.** `BeginUpdateResourceW`
   documents that the target "cannot be currently executing", and rewriting
   resources invalidates Authenticode anyway.
5. **macOS windows have no icon**, only the document proxy icon
   (`representedURL` + `standardWindowButton(.documentIconButton)`). A
   cross-platform `set_window_icon` should no-op on macOS, not fake it.
6. **`NSWorkspace.setIcon:forFile:` on your own bundle breaks the code
   signature** ("app is damaged" on Ventura+), fails under the sandbox. Do not
   expose it. `applicationIconImage` is the answer; it is process-local and
   resets on relaunch, and persisting requires an `NSDockTilePlugIn` (banned
   from the App Store).
7. **`xdg-toplevel-icon-v1` did NOT solve Wayland window icons.** Shipped in
   wayland-protocols 1.37 (2024-08-31), still **staging** in 2026, and
   **Mutter/GNOME does not implement it** (GNOME/mutter#4100 open). KWin
   (Plasma 6.3+), wlroots 0.19+, sway 1.11+, labwc do. Plan for the fallback
   being the common case.

## 2. Windows

Precedence, per the `WM_GETICON` remarks: **per-window (`WM_SETICON`) >
per-class (`GCLP_HICON`/`GCLP_HICONSM`) > `LoadIcon` default.**

Set BOTH: the class slots at registration (or between `CreateWindowExW` and
`ShowWindow`), because a slow startup lets the shell snapshot the default icon
for the taskbar button before `WM_SETICON` ever runs (dotnet/wpf#11308); and
the window slots per window.

Sizes: `GetSystemMetricsForDpi(SM_CXSMICON, dpi)` = 16/20/24/32 at
96/120/144/192; `SM_CXICON` = 32/40/48/64. Supply at the EXACT metric size —
`WM_SETICON` will accept an oversized icon and downscale it, which is why so
many apps have a mushy title-bar icon.

**Ownership:** `WM_SETICON` does not take ownership and returns the previous
handle. `DestroyIcon` it (unless it came from `LoadIcon`/`LR_SHARED`). Not
doing this on every `WM_DPICHANGED` burns toward the ~10 000 USER-object cap,
after which icon creation silently fails.

**Badges:** `ITaskbarList3::SetOverlayIcon` (16x16 @96dpi, `NULL` clears,
description required for a11y) — and note it is the one API that copies the
icon, so you may `DestroyIcon` immediately after.

**Taskbar icon selection:** pinned-not-running → the `.lnk`'s icon; running
without an explicit AUMID → the window icon; running WITH an AUMID matching a
shortcut → grouped under it, and `System.AppUserModel.RelaunchIconResource`
governs the pinned representation (ignored unless the window has an explicit
AUMID).

## 3. macOS

`NSApp.applicationIconImage = NSImage` — Dock tile, Cmd-Tab, About panel.
Apple's own wording: "**temporarily** change the app icon"; `nil` restores.
Process-local, resets on relaunch.

`NSApp.dockTile.badgeLabel` is the badge slot. **`NSDockTile` never redraws
itself — you must call `display()`.** That is the #1 "my dock icon doesn't
update" cause. Also: installing `dockTile.contentView` SUPERSEDES
`applicationIconImage`; they do not compose.

RGBA → `NSImage`: `NSBitmapImageRep` with `bitsPerSample:8 samplesPerPixel:4
hasAlpha:YES isPlanar:NO colorSpaceName:NSDeviceRGBColorSpace
bitmapFormat:NSBitmapFormatAlphaNonpremultiplied bytesPerRow:w*4
bitsPerPixel:32`. **Pass `planes:NULL` and `memcpy` into `[rep bitmapData]`** —
it does NOT copy a buffer you supply, so handing it a Rust `Vec`'s pointer is a
use-after-free. Avoid `NSCalibratedRGBColorSpace` (colour shift).

`NSImage.size` is POINTS, `pixelsWide` is PIXELS; the ratio is the scale. One
1024x1024 rep is sufficient and never upscales.

macOS 26 "Tahoe" enforces a squircle for bundle icons, but a runtime
`applicationIconImage` is drawn as supplied — if you want it to look native you
must draw the rounded-square yourself.

## 4. X11

`_NET_WM_ICON`, type `XA_CARDINAL`, format 32: `[w1,h1, w1*h1 px..., w2,h2,
...]`, pixels `0xAARRGGBB`, multiple sizes concatenated into ONE property.

**The 64-bit trap — this is the #1 bug here.** With `format == 32`, Xlib
requires `data` to be an array of `long`, which is **8 bytes on LP64**. Passing
`*const u32` gives garbage or crashes inside `_XData32`. From Rust build
`Vec<c_ulong>` and zero-extend each pixel. **XCB is exempt** (raw bytes,
`format=32` really means 32-bit) — so if we dlopen both, the buffer layout
differs per path. Only reproduces on 64-bit, i.e. everywhere real.

Set it BEFORE the first `XMapWindow` — a few WMs only read at `MapNotify`. Re-set
after any window recreation.

Ship a stable `WM_CLASS` + a `.desktop` file whose `StartupWMClass` matches it
exactly (case-sensitive): panels generally prefer the desktop file's `Icon=`
over `_NET_WM_ICON`.

Skip the ICCCM `WM_HINTS.icon_pixmap` path — no alpha, needs a server-side
Pixmap, nothing has preferred it in fifteen years.

## 5. Wayland

`xdg-toplevel-icon-v1` is the only pixel path. Bind
`xdg_toplevel_icon_manager_v1`; it sends `icon_size` events then `done`.

    create_icon -> icon
    set_name("org.example.App")        // optional, but set BOTH name and buffers:
    add_buffer(buf, scale)             // compositor policy chooses which to use
    set_icon(toplevel, icon)           // icon is now IMMUTABLE
    // keep every buffer + the shm pool ALIVE and unmodified

Four rules, each violation a **fatal protocol error that kills the connection**:
buffers must be **square** and `wl_shm`-backed (Wine shipped a fix purely to pad
non-square icons); buffers must outlive the icon and not be rewritten;
`add_buffer`/`set_name` after `set_icon` raises `immutable`; destroying a buffer
early raises `no_buffer`. To change the icon, build a NEW icon object.

Destroying the icon object does NOT clear it from the toplevel.

Fallback when the global is absent: `xdg_toplevel.set_app_id` + a matching
`.desktop` file (app_id == desktop file ID minus `.desktop`). No match → the
generic/"image-missing" icon. The classic failure is a CASE mismatch between
`WM_CLASS` and app_id (the Electron/Slack/Discord generic-icon bug).

There is no other way to set pixels — no KDE- or GNOME-private extension.
Under XWayland, `_NET_WM_ICON` works.

## 6. Sizes to render

* Windows: `SM_CXSMICON` and `SM_CXICON` per DPI (16/20/24/32 and 32/40/48/64);
  tray at `SM_CXSMICON`; overlay 16.
* macOS: one 1024 px rep; menu-bar `NSStatusItem` 18 pt (36 px @2x), TEMPLATE
  image (black + alpha) so AppKit tints it.
* X11: 16, 24, 32, 48, 64, 128, 256 in one property.
* Wayland: honour `icon_size`; else 16/24/32/48/64/128/256 at scale 1 plus 2x
  buffers. Square only.

## 7. Proposed API

    set_app_icon(&[IconImage])          // process-wide
    set_window_icon(window, &[IconImage])
    set_badge(Option<Badge>)

|  | app icon | window icon | badge |
|---|---|---|---|
| Windows | class `GCLP_HICON`+`GCLP_HICONSM` + `WM_SETICON` on all windows | `WM_SETICON` both slots, answer `WM_GETICON` per-DPI | `ITaskbarList3::SetOverlayIcon` |
| macOS | `NSApp.applicationIconImage` | **no-op** | `dockTile.badgeLabel` (+ `display()`) |
| X11 | `_NET_WM_ICON` on every toplevel + `WM_CLASS` + `.desktop` | `_NET_WM_ICON` | none standard |
| Wayland | `set_app_id` + `.desktop` | `xdg-toplevel-icon-v1` if bound, else nothing | none standard |

Cache by `(slot, size, scale)`. Probe capability at startup (bind the Wayland
global; `GetProcAddress("GetSystemMetricsForDpi")`) and degrade **loudly** —
silently doing nothing is how the Wayland generic-icon bug survives.

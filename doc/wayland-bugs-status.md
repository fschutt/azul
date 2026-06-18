# azul 0.2.0 Wayland/X11 bug status (vs current master)

Source: `/home/fs/Downloads/temp2/bugs.txt` — a manual report against the **June-15 CI
binary** (downloaded from the releases page). Cross-checked against current `master`
(code read + `git log` + the user's own captured logs in temp2/).

**KEY INSIGHT:** the tested binary predates several fixes. `temp2/cpu_run.log` contains
`Auto-injecting Titlebar` and `CpuBackend … Clip stack underflow` — both fixed on master.
So **cutting a fresh 0.2.0 release alone clears the FIXED bugs below** (#2, #4, #5, and the
#1/#7 fixed parts). That makes the release (plan step 6) high-leverage on its own.

## Status table

| # | Bug | Verdict |
|---|-----|---------|
| 2 | Wayland frame-callback leak + malloc crash on close | **FIXED** (`5baefa08c`) |
| 4 | Input misclassification (pinch / titlebar-maximize / stray markers) | **FIXED** (`d4dc18844`,`20ed12941`, Linux titlebar gate) |
| 5 | Maps VirtualView resize (grey bg / clear-pins text / smudges) | **FIXED** (`f14586893`,`aa7662df3`,`684470f04`) |
| 1 | Repaint / damage-rect | **PARTIAL** — clip-underflow flicker FIXED (`7a323ef1e`,`881afcd1e`); over-damage + X11 resize-spam OPEN |
| 7 | hello-world titlebar | **PARTIAL** — double-bar + font FIXED (`cb29e7b76`); white-bg/no-blur OPEN (cosmetic) |
| 3 | HiDPI / 2× low-res (canvas/SVG/map tiles) | **OPEN — top priority** |
| 6 | Menu/popup (toplevel not popup; captures dialog; decorated) | **OPEN — release blocker** |
| 10 | azul-maps Wayland freeze (~12% CPU) | **OPEN — needs live profiling** |
| 9 | azul-paint Wayland startup crash (intermittent) | **OPEN — needs live trace** |
| 8 | "Locate" hangs forever | **OPEN — example-level, easy** |

## OPEN bugs — fix approaches (priority order)

### #3 HiDPI 2× low-res (TOP)
Glyphs raster at the render `dpi_factor`, but replaced content (image/canvas/tile) is
blitted from a source bitmap produced at a *different* scale, then nearest-scaled in
`render_image` (`layout/src/cpurender.rs:4694`, dst `:4701`). Two defects:
- Map tiles ignore DPR: `layout/src/widgets/map.rs:1336` `let tile_px = 256.0 * zoom_scale;`
  — no `hidpi_factor`. `HidpiAdjustedBounds.get_physical_size = logical × hidpi_factor`
  (`core/src/callbacks.rs:773`).
- Wayland initial scale defaults to 1.0, corrected late: `calculate_current_scale_factor()`
  returns 1.0 when outputs empty (`wayland/mod.rs:3841`); scale arrives async via
  `wl_surface.enter` (`wayland/events.rs:196`). First layout + image raster run at wrong
  scale → "clicking the menu snaps it to correct pixel-scale."
- **Fix step 0 (do first):** LOG the runtime `hidpi_factor`/output scale at window-create +
  each relayout — DPI is currently logged nowhere, and the user's "not a HiDPI monitor yet
  2×" contradiction can't be resolved without the real value.
- Then: thread `hidpi_factor` into `map.rs:1336` (and @2x tiles when scale≥2); make the
  image-callback `HidpiAdjustedBounds.hidpi_factor` == renderer `dpi_factor`; use the real
  output scale on first layout.

### #6 Menu → xdg_popup (release blocker; unblocks azul-paint file load)
Root cause: the Wayland menu is created as an `xdg_toplevel`, not an `xdg_popup`. A correct
`WaylandPopup::new()` (with `xdg_positioner`) exists at `wayland/mod.rs:3982-4190` **but is
never called** — TODO at `run.rs:1226-1254`; `show_menu_from_callback` (`wayland/mod.rs:825`)
always falls back to `show_fallback_menu()`. Toplevel ⇒ monitor-anchored + WM-decorated +
no popup-grab/dismiss (all three symptoms). Captures over file dialog: menu is
`is_always_on_top` (`menu.rs:436`), nothing dismisses it when the portal FileChooser opens.
Crash on position: zero-size anchor rect (`wayland/mod.rs:897`) fed to positioner — clamp 1×1.
- **Fix:** route `WindowType::Menu` in `run.rs:1226+` to `WaylandPopup::new()`; auto-dismiss
  menus when a modal/native dialog opens; clamp anchor. (X11 is mostly OK: override_redirect +
  `_NET_WM_WINDOW_TYPE_POPUP_MENU` set; monitor-anchor only when parent id unresolved,
  `x11/mod.rs:1701`.)

### #10 azul-maps Wayland freeze (~12% CPU) — needs live Wayland session
12% ≈ one core busy, not a 100% spin. Event loop ticks every 16 ms while `lw.threads`
non-empty (`wayland/mod.rs:1689`) + runs `check_timers_and_threads` each wake (`:1730`). If a
tile-fetch thread is never drained from `lw.threads`, it ticks+repaints forever. CI binary
predates `f6aa34479` (shared CpuBackend) + `e8410a84a` (tile sweep timer). **Fix:** log
`lw.threads.len()` + `needs_redraw` per iter on a live session; check tile-thread drain +
VirtualView self-retrigger.

### #9 azul-paint Wayland startup crash — needs live trace
No deterministic crash site; EGL/buffer init null-checked (`wayland/gl.rs:220`). Likely a
configure/first-buffer race or async sensor/geo threads during init, possibly tied to the
menu path (#6). **Fix:** `AZ_LOG=trace` on a live Wayland session to capture the abort.

### #8 "Locate" hangs — example-level, EASY (good headless candidate)
`on_locate` (`examples/azul-maps/src/lib.rs:494-519`) sets `locating:true` + probes, but has
**no timeout/error path**; the Linux GeoClue2 backend returns `Err` silently when unavailable
(`dll/src/desktop/extra/geolocation/linux.rs:73-90`), so it stays "Acquiring location…"
forever + the placeholder marker never clears. **Fix:** timeout timer / handle the backend
error → reset `locating` + show a "geolocation unavailable" message box.

### #1 over-damage (PARTIAL) — wire existing damage diff into live shells
`cpu_state.damage_rects` is read but never populated → full-surface damage every present
(`wayland/mod.rs:558`); X11 always full `XPutImage` (`x11/mod.rs:2698`). The per-rect damage
diff (`cpurender::compute_display_list_damage`, `:2802`) is wired only into
`headless/mod.rs:282`. **Fix:** populate `damage_rects` from the diff + partial-raster the
live CPU present (GPU already per-rect, `wayland/mod.rs:3194`). Also: X11 relayouts on every
`ConfigureNotify` (`x11/mod.rs:~2091`) — coalesce (drain pending, relayout once).

### #7 blur (PARTIAL, cosmetic)
KDE blur is wired (`wayland/mod.rs:2851`) but only for translucent
`WindowBackgroundMaterial`; default opaque bg paints over it. Low priority.

## Recommended fix order
1. #3 HiDPI (log DPI first, then map.rs:1336 + image-callback scale + initial-DPI timing)
2. #6 menu→xdg_popup + dialog auto-dismiss
3. #10 maps freeze + #9 paint crash (need a live Wayland session — likely hand to user)
4. #1 over-damage (wire damage diff) + X11 resize coalescing
5. #8 Locate timeout, then #7 blur
**Plus: cut a fresh 0.2.0 release** to ship the already-FIXED #2/#4/#5/#1-#7-parts.

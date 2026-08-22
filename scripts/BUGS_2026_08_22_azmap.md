# AzMaps: "+" stops responding, "↑" pans south, tiles load edge-first, gestures/3D, themes

Written 2026-08-22 on worktree `debug-slider-scroll-2026-08-22` (branch
`fix/open-bugs-wave-2026-08-22`, HEAD `1de302052`). Read-only investigation: no
source was edited, nothing was built or run. Line numbers are from this
worktree. Web facts (tile providers, licences, attribution strings) were
fetched 2026-08-22 and are quoted with their source URL; re-check before
hard-coding them.

The generic "mouse button stuck" family is covered by
`scripts/BUGS_2026_08_22_input_state_stuck.md` (another session). This report
covers only the MAP-SPECIFIC side: the "+" wiring, what a zoom does, which
map state can wedge, the pan-button sign bug, tile load order, gesture
plumbing, the 3D-tilt scope, and the theme/attribution research.

## Symptoms (user, AzMaps demo on macOS)

> "renders CORRECTLY but does not respond anymore to '+' or zoom in → UI is
> not locked, responsive on window resize — probably the same 'mouse button
> stuck' bug"

> "'UP' button goes down (on northern hemisphere)?"

> "tiles should load from the middle out"

> "zooming / gestures should work — also could we add a 3D transform on
> 3-finger up (needs better tile calculation logic)?"

> "theme is horrible — research good map themes for various tile systems;
> ideally use something already existing from open free maps instead of
> hand-rolling; the slippy-maps widget should provide many predefined themes
> with attribution"

## Status at a glance

| # | Symptom | Status | Root cause (file:line) |
|---|---|---|---|
| 1 | "+" / zoom-in no-op, UI otherwise live | **Explained without any stuck button (not reproduced — read-only session).** Most likely the view is pinned at the layer's `max_zoom = 14` (Z1) after a trackpad-scroll runaway (G1: every trackpad event = ±0.5 zoom, momentum included). Independently, a `RefreshDom` whose only change is the viewport inside the dataset is swallowed by the pre-cascade fast path (Z2) — masked in the demo by the header readout. | `examples/azul-maps/src/lib.rs:107-115` (clamp), `layout/src/widgets/map.rs:912-913` (±0.5 per wheel event, clamp), `dll/src/desktop/shell2/common/layout.rs:412-421,474,505-519` (fast path returns `LayoutUnchanged`, VirtualView never re-invoked) |
| 2 | "↑" pans south | **Confirmed sign bug.** `MapState::pan` adds `dy` straight to latitude while the callers pass `dy = -1` for "up" in TILE space (y grows south). | `examples/azul-maps/src/lib.rs:150-152` with `:561-571` |
| 3 | Tiles load from the left edge, not the centre | **Confirmed.** Fetch order is `BTreeMap<MapTileId>` order = `(z, x, y)` ascending, 16 per 250 ms sweep, with a 1-tile off-screen margin — so the first column spawned is the INVISIBLE west margin; a lower-zoom `Pending` leftover is fetched before anything at the current zoom; nothing is ever cancelled. | `layout/src/widgets/map.rs:56-63` (Ord), `:1127-1134` (take 16 in key order), `:1289-1308` (+1 margin), `:1069` (250 ms), `:1180` (thread id discarded) |
| 4a | Trackpad pinch does nothing | **Confirmed, two independent breaks.** (a) The native magnify gesture is cleared from the gesture manager BEFORE callbacks run, so `info.get_pinch()` in the map handler is `None`; (b) the `PinchIn`/`PinchOut` (and all `Touch*`) handlers sit on the OUTER widget div, which the VirtualView's nested DOM shadows — they can never fire. | `dll/src/desktop/shell2/common/event.rs:7117` vs `:7475`; `layout/src/widgets/map.rs:373-404` vs `:1480-1508` |
| 4b | Trackpad scroll-zoom is a runaway | **Confirmed.** ±0.5 level per event regardless of delta size or momentum phase. | `layout/src/widgets/map.rs:912`, `dll/src/desktop/shell2/macos/events.rs:509-532` |
| 4c | 3D tilt on 3-finger swipe | **Not implemented; scoped below.** GPU path can draw a perspective plane today; CPU raster/hit-test are affine-only; macOS has no raw-touch/3-finger ingress and 3-finger swipes are OS gestures. | `core/src/transform.rs:538`, `dll/src/desktop/compositor2.rs:1381-1392`, `layout/src/cpurender/raster.rs:2305-2311` |
| 5 | Theme | **Confirmed: hand-rolled 7-rule palette, drawn in tile decode order, no class/zoom rules, grey land base.** Research + preset API below. | `dll/src/desktop/extra/map/svg.rs:61-78,163-175,183` |

---

## 1. "+" / zoom-in does not respond

### 1.1 How "+" is wired (the full trace)

1. The header buttons are plain `div`s with `HoverEventFilter::MouseUp`
   callbacks (`examples/azul-maps/src/lib.rs:285-292` for "+"). `on_zoom_in`
   (`:479-487`) calls `MapState::zoom_in` (`:107-110`):
   `zoom = (zoom + 1.0).min(max_zoom)` and returns `Update::RefreshDom`.
2. `RefreshDom` → `regenerate_layout` → `layout()` rebuilds the whole DOM.
   `MapWidget::create(layer).with_viewport(viewport)…dom()` (`:342-367`) →
   api.json routes `dom()` to `azul_dll::unified::map::map_widget_dom`
   (`dll/src/desktop/extra/map/mod.rs:206-228`) → `dom_with_fetch` →
   `build_dom` (`layout/src/widgets/map.rs:319-424`) allocates a FRESH
   `MapTileCache` carrying the new viewport.
3. Reconcile. Two possible paths in `dll/src/desktop/shell2/common/layout.rs`:
   * **Full path** (fingerprints differ): `transfer_states`
     (`core/src/diff.rs:982-1097`) runs `merge_map_tile_cache`
     (`map.rs:610-648`), which RETURNS THE OLD cache but adopts the fresh
     build's `viewport`, `layer`, `on_viewport_changed` (`:642-644`; note
     `on_pin_tap` is NOT adopted — minor). Then
     `layout_and_generate_display_list` calls
     `virtual_view_manager.reset_all_invocation_flags()`
     (`layout/src/window.rs:1791`) so `check_reinvoke` returns
     `InitialRender` and `map_widget_render` (`map.rs:1356`) runs with the
     new zoom, marking the new grid `Pending` (`:1432-1441`).
   * **Pre-cascade skip** (both fingerprint tiers equal,
     `common/layout.rs:412-421`): the retained `StyledDom` is kept; fresh
     callbacks are installed and `merge_fresh_dataset` (`:474`,
     `core/src/diff.rs:1149-1163`) runs the same merge callback — so the
     cache's viewport IS updated — but the function then returns
     `LayoutRegenerateResult::LayoutUnchanged` (`:505-519`) unless the window
     size or a hover/focus/active flag changed. **No VirtualView re-invoke,
     no `Pending` tiles, nothing for the sweep timer to do.** The map keeps
     showing the old zoom until an unrelated event re-renders it.
4. Fetching. Nothing in step 3 spawns threads. The `AfterMount` handler
   (`map.rs:1051-1087`, fires once per mount) installed a 250 ms sweep
   timer (`:1062-1070`) whose tick (`:1196-1205`) calls
   `spawn_pending_tile_fetches` (`:1090-1189`): up to 16 `Pending` tiles
   per tick become `Fetching` and get one framework `Thread` each running
   `tile_fetch_worker` (`extra/map/mod.rs:94-203`). Writebacks land in
   `map_tile_writeback` (`map.rs:1224-1278`) which calls
   `info.trigger_all_virtual_view_rerender()` → in-place re-render.

The fingerprint covers node content including text (`core/src/diff.rs:4604-4625`,
`node.get_node_type().hash()`), so in the demo the header line
`"… · zoom {:.1}"` (`lib.rs:230-233`) changes on every effective zoom change
and forces the full path. That is the only reason the demo's "+" works at
all; see Z2.

### 1.2 What can make a later "+" a no-op

**Z1 — pinned at `max_zoom` (most likely what the user hit).**
`zoom_in` clamps at `layer.max_zoom` (`lib.rs:109`), and the wheel handler
clamps the same way (`map.rs:907-913`). `MapTileLayer::default()` is
OpenFreeMap's planet vector set with `max_zoom: 14` (`map.rs:92-112`). At
14.0 the header text no longer changes, so "+" → `RefreshDom` → identical
fingerprints → pre-cascade skip → `LayoutUnchanged`. Wheel/trackpad "zoom
in" is clamped too. "−" still works, drag still works, resize still works:
exactly "UI is not locked, responsive on window resize".

How you get to 14 without noticing: G1 below — on a macOS trackpad every
scroll EVENT (including the momentum tail after the fingers lift) is a full
±0.5 level (`map.rs:912`). One two-finger flick is typically 20-40 events → the view
goes from 2.0 to 14.0 in one gesture and the tiles at z14 do load and
"render correctly". Diagnostic: the header reads `zoom 14.0`;
`AZ_MAP_DEBUG=1` prints `[map-demo] on_zoom_in FIRED` on every click
(`lib.rs:480-482`) followed by no `[map] render …` line.

The widget already renders OVERZOOM correctly — `z_int` is clamped to
`max_zoom` but `frac_zoom = zoom - z_int` is not, so zoom 16 draws z14 tiles
at `zoom_scale = 4` (`map.rs:1398-1403`). Only the two clamps stand in the
way (Leaflet's `maxNativeZoom` vs `maxZoom` distinction).

**Z2 — dataset-only `RefreshDom` is swallowed by the pre-cascade skip
(framework hole, masked in the demo).** Any app whose DOM does not print the
zoom somewhere — a map with only "+"/"−" buttons — rebuilds an identical
DOM. `merge_fresh_dataset` adopts the new viewport into the persistent cache
(`common/layout.rs:474`), but the skip path never touches the
`VirtualViewManager`, and `check_reinvoke`
(`layout/src/managers/virtual_view.rs:320-372`) has no rule for "host
dataset changed". The map then shows the OLD grid while the cache says the
new zoom; the next tile writeback (if any is in flight) or the next
drag/wheel/resize suddenly snaps it to the new zoom. In the demo today the
only identical-DOM rebuilds are the clamped ones (Z1 — nothing new to show
anyway), so Z2 is latent; it becomes live the moment an app drops the
header readout, or once Z4 is fixed and a hook's `RefreshDom` rebuilds a
DOM whose text did not change (a pin tap re-renders only the overlay).

**Z3 — stuck map drag state (the user's guess).** `drag_anchor` is set on
`MouseDown`/`TouchStart` (`map.rs:740-756`) and cleared in
`map_on_pointer_up` (`:844-881`), which is wired to `MouseUp`, `MouseLeave`,
`TouchEnd`, `TouchCancel` on the grid (`:1483-1503`). Every event bubbles
(`core/src/events.rs:900-940`, no non-bubbling list), but the tile divs have
no callbacks/`:hover`/cursor and so get no hit-test tag
(`core/src/prop_cache.rs:1441-1490`): crossing from tile to tile does NOT
produce a `MouseLeave`, so the drag is not cut at tile boundaries (the
slider bug on this branch does not recur here). Leaving the window produces
an empty hit test (`macos/events.rs:335-396` → `update_hit_test` → the
grid loses hover → `MouseLeave` → `map_on_pointer_up`), so the map drag ends
at the window edge. If the framework-level button state IS stuck (other
report), the map-visible symptom is a "sticky map" that pans on plain mouse
moves — it does not block the "+" button: the header div still receives
`MouseUp` (`determine_all_events` emits `MouseUp` on any `left_down`
true→false transition, `layout/src/event_determination.rs:366-387`). So the
map-specific state cannot wedge "+"; Z1/Z2 can.

Also NOT a wedge: the nested-DOM hit test. The tile grid over-scans one
tile past every edge (`map.rs:1289-1308`), so tile divs extend above the
header, but the grid is `overflow: hidden` and WebRender hit-tests through
the same clip chain it paints; the "+" button stays reachable. (If a
screenshot ever shows tiles painted over the header, this assumption is
wrong and the button IS covered.)

**Z4 — the user hooks' `Update` return is discarded.**
`invoke_viewport_changed` is called at `map.rs:794`, `:833`, `:921` and
`invoke_pin_tap` at `:867`, and each handler then returns
`Update::DoNothing`. The demo's hooks return `RefreshDom` (`lib.rs:492-497`,
`:579-587`). Effect: the header readout does not update during drags or
wheel zooms, and a tapped pin does not appear until the next unrelated
rebuild ("+", Recentre). Not the "+" bug, but the user will report it next.

**Z5 — `pinch_anchor` is never cleared on macOS.** A trackpad magnify has no
pointer-up, and only `map_on_pointer_up` clears `pinch_anchor` (`:855`).
Moot until 4a is fixed, then it makes the second pinch start from the last
gesture's distance.

### 1.3 Fixes

* **Z1 (demo + widget):** split native and display zoom like Leaflet:
  `MapTileLayer { max_native_zoom: 14, max_zoom: 18 }` — render-side is
  already overzoom-capable; change the two clamps (`lib.rs:109`,
  `map.rs:907-913`, and the pinch clamp `:783-785`) to `max_zoom` and the
  `z_int` clamp (`map.rs:1398`, `:1334`, `:524`) to `max_native_zoom`. Grey
  out "+" when `zoom >= max_zoom` in the demo (`BTN_DISABLED` style) so a
  clamped press is visible instead of silent.
* **Z2 (framework + widget, belt and braces):**
  1. Widget-local: stamp `rendered_viewport: Option<MapViewport>` in
     `map_widget_render` and let `map_fetch_sweep_tick` (`map.rs:1196`)
     compare `cache.viewport` against it; if different, call
     `info.callback_info.trigger_all_virtual_view_rerender()`. Cost: one
     compare per 250 ms; covers every external viewport change regardless
     of the reconcile path, and the `Pending` marking + spawn follow.
  2. Framework: in the pre-cascade skip branch
     (`common/layout.rs:472-476`), when `transfers.datasets` is non-empty
     and a merge callback actually ran, queue `DomRecreated` for every
     VirtualView hosted under a merged node
     (`layout_window.queue_all_virtual_view_reinvoke()` is the coarse
     version, `layout/src/window.rs:14189-14197`) and return
     `LayoutChanged` so the frame tail's `process_pending_virtual_view_updates`
     (`common/layout.rs:1660-1672`) runs. A stateful widget that rebuilt an
     identical DOM but changed its dataset is by definition asking for a
     re-render of anything that reads the dataset.
* **Z4:** return `user_update.max(Update::DoNothing)` from the three pointer
  handlers and the scroll handler instead of dropping it. A `RefreshDom`
  from the hook is safe: the merge keeps the cache, and the full path
  re-invokes the VirtualView anyway.
* **Z5:** clear `pinch_anchor` when a pinch event arrives with
  `duration_ms == 0` (native per-event delta), see G2/G4.
* **Minor:** adopt `on_pin_tap` in `merge_map_tile_cache` (`:644`) next to
  `on_viewport_changed`.

---

## 2. "↑" pans south — the sign bug

`examples/azul-maps/src/lib.rs`:

```rust
// :137  fn pan(&mut self, dx: f64, dy: f64)   — "Nudge the viewport ~half a tile in tile-space"
// :150  let delta_lat = (dy / 2.0) * (180.0 / tile_count);
// :151  self.viewport.centre_lat_deg =
// :152      (self.viewport.centre_lat_deg + delta_lat).clamp(-85.0, 85.0);
// :563  on_pan_up:   s.pan(0.0, -1.0);
// :570  on_pan_down: s.pan(0.0,  1.0);
```

The callers speak TILE space (documented in the fn comment and in the
widget: "y grows south (0 at the north edge ~85.05°)", `map.rs:934-940`):
"up" on screen = smaller tile-y = `dy = -1`. But `pan` adds `dy` to
LATITUDE, where north is POSITIVE. So "↑" subtracts latitude → the centre
moves south; "↓" moves north. It is wrong in both hemispheres (the user saw
it in the north; in the south it is equally inverted). The x axis is fine
(tile-x and longitude both grow east, `:141-147`).

Second, smaller defect on the same lines: the step is linear in degrees
(`180 / tile_count` per tile), but Mercator tile rows are not equal in
latitude — at 37.8° N one z2 tile is ~38° of latitude, not 45°; near 60° N
the button overshoots by ~2×. The widget's drag math
(`pan_viewport`, `map.rs:982-997`) applies `cos(lat)` for exactly this
reason, and its exact helpers `lat_to_tile_y`/`tile_y_to_lat`
(`:947-971`) are private (`tile_y_to_lat` is even `#[allow(dead_code)]`).

### Fix (pick one; (b) is the right one)

(a) One-character fix in the demo: `let delta_lat = -(dy / 2.0) * (180.0 / tile_count);`
— keeps the linear approximation.

(b) Make the buttons reuse the widget's projection so the demo cannot drift
from the widget: expose a px-based pan on the public surface, e.g.
`MapWidget::pan_viewport_px(viewport: MapViewport, dx_px: f32, dy_px: f32) -> MapViewport`
(api.json `widgets/MapWidget` function, wraps `pan_viewport`), and call
`s.viewport = MapWidget::pan_viewport_px(s.viewport, 0.0, -128.0)` for "↑"
(dragging the CONTENT down by 128 px reveals the north, matching the mouse
drag sign at `map.rs:990-992`). Same for ←/→ with ±128 px. This also
removes the demo's private longitude wrap (`lib.rs:144-146`).

### Tests

* `layout/src/widgets/map.rs` (module `tests`): add
  `pan_up_increases_latitude_in_both_hemispheres` — `pan_viewport(37.0, 0, 2, 0, -128)` and `pan_viewport(-37.0, …)` both yield `lat < start` (content dragged UP reveals the south) and the `+128` mirror yields `lat > start`; and `pan_px_is_the_inverse_of_latlon_at_px` — for a 512×512 container, `latlon_at_px(vp, centre + (0,-128))` equals `pan_viewport(…, 0, +128)` within 1e-6.
* `examples/azul-maps` has no tests; after (b) the demo has no math left to test. If (a) is chosen, add a `#[cfg(test)]` in `lib.rs`: `pan(0,-1)` from 37.7749 N gives a larger latitude.

---

## 3. Tile load order: from the centre out

### 3.1 Where tiles are enumerated and fetched today

* Enumeration: `visible_tile_range` (`map.rs:1289-1308`) returns an
  inclusive rectangle with a `+1.0` tile margin on every side; both
  `map_visible_tiles` (`:1328-1353`) and `map_widget_render` (`:1432-1441`)
  walk it `for x in x_min..=x_max { for y in y_min..=y_max }` — column-major
  from the north-west corner — and insert `TileEntry::Pending` into
  `cache.tiles`, a `BTreeMap<MapTileId, _>`.
* Scheduling: `spawn_pending_tile_fetches` (`:1090-1189`) takes the first
  `MAX_SPAWN_PER_CALL = 16` (`:1095`) `Pending` entries **in map key
  order** (`:1127-1134`). `MapTileId` derives `Ord` with field order
  `z, x, y` (`:56-63`), so the order is: lowest zoom first, then west→east,
  then north→south. Consequences:
  * The first column spawned is the off-screen WEST margin column
    (`x_min = floor(centre_x - half_w)` with `half_w` including the margin).
    The centre is reached last. With 7×4 = 28 visible-ish tiles at z14 and
    a 250 ms sweep, the centre tile is spawned in the second sweep.
  * After a zoom-in, `Pending` leftovers at the OLD zoom (margin tiles that
    were never spawned) sort before every tile of the new zoom.
  * Horizontal wrap means at z≤2 most columns alias — harmless.
* Concurrency: one OS thread per tile (`Thread::create`, `:1176-1180`),
  blocking `http_get` (`extra/map/mod.rs:140`), no per-host cap, no
  connection reuse. The `ThreadId` is `unique()` and dropped (`:1180`) —
  the cache cannot address a worker later, so nothing is ever cancelled;
  the worker's own `TerminateThread` check (`mod.rs:157-163`, a
  non-blocking `try_recv`) never sees a message.
* The `mvt.rs` helpers the task mentions (`get_tile_coordinates_for_extent`
  `:162-186`, `tile_coords_to_urls` `:197-209`) are an older API with zero
  callers from the widget — the widget's own `visible_tile_range` is the
  live path; they are kept for the orphan allowlist only.

### 3.2 Ordering algorithm

Keep the data structure; change the selection:

```text
priority(tile) =
    |tile.z - z_cur| * 1_000_000                       // current zoom first, always
  + max(|tx - cx|, |ty - cy|) * 1_000                  // Chebyshev ring distance: ring 0, 1, 2 …
  + |tx - cx| + |ty - cy|                              // tie-break inside a ring: axis-near before corners
where (cx, cy) = viewport centre in tile space at z_cur, (tx, ty) = tile centre (x+0.5, y+0.5)
projected into z_cur space (same scaling `prune_distant_tiles` already does, map.rs:536-546).
```

In `spawn_pending_tile_fetches`: collect `Pending` ids, compute priority,
`sort_by_key`, `take(MAX_SPAWN_PER_CALL)`. Ring order is what the user sees
as "middle out"; Leaflet does the same (`_addTilesFromCenterOut`, sorting
by distance to the centre). Additionally:

* Drop `Pending` tiles that are not in the current `map_visible_tiles`
  set at all (other zoom, or scrolled away) instead of fetching them —
  remove the entry; it is re-inserted if they come back.
* Record the worker: `TileEntry::Fetching { thread: ThreadId }`; on every
  spawn pass (and in `map_on_pointer_up`/`map_on_scroll`) call
  `info.remove_thread(id)` (`layout/src/callbacks.rs:1375`; the drop sends
  `TerminateThread`, `layout/src/thread.rs:668`) for `Fetching` tiles that
  left the visible set, and reset them to absent. The worker already aborts
  between fetch and decode (`mod.rs:157-163`), so at least the decode +
  SVG raster + writeback are saved; to cancel the HTTP itself the worker
  would need a `http_get` with a cancellation token (not available today).
* Bound concurrency: 6 in flight per host (browser/Leaflet convention),
  re-filled on every writeback (the writeback already has a `CallbackInfo`,
  `map.rs:1224`) instead of waiting for the 250 ms sweep — the sweep stays
  as the fallback only. This also turns the 250 ms "first tile" latency
  into "as soon as the previous one lands".
* Render the parent tile (z-1) scaled ×2 under a `Pending` child when it is
  `Ready` in the cache — the cache keeps 192 tiles (`:517`), so after a
  zoom the parent is usually there. Leaflet's "retain parent" behaviour;
  removes the grey-grid flash on every "+".

### 3.3 Tests

* `spawn_pending_tile_fetches_spawns_the_centre_ring_first`: cache with a
  5×5 `Pending` grid around the centre, `MAX_SPAWN_PER_CALL` = 9 → the nine
  `Fetching` entries after one call are exactly the centre 3×3. Pattern:
  `spawn_pending_tile_fetches_caps_the_burst_at_sixteen` (`map.rs:3721`).
* `spawn_pending_tile_fetches_prefers_the_current_zoom`: two `Pending`
  tiles, one at z3 (stale), one at z4 (current, far corner) → the z4 one is
  spawned first.
* `stale_pending_tiles_are_dropped_not_fetched`.
* A pure-function test for `priority()` on a hand-computed table.

---

## 4. Gestures, and a 3D tilt

### 4.1 What reaches the map today (macOS)

| Input | Ingress | Reaches the map as | Works? |
|---|---|---|---|
| Mouse drag | `macos/events.rs:171-233,335-396` → `MouseDown/MouseOver/MouseUp` on the grid (`map.rs:1483-1503`) | pan via `pan_viewport` | yes |
| Wheel / two-finger scroll | `handle_scroll_wheel` (`macos/events.rs:444-637`) → `scroll_manager.pending_wheel_event` (`layout/src/managers/scroll_state.rs:621`) → synthesized `Scroll` at the hovered node (`event_determination.rs:540-554`) → `map_on_scroll` (`map.rs:884-924`) | `dz = dy.signum() * 0.5` per EVENT | runaway (G1) |
| Trackpad pinch | `magnifyWithEvent:` (`macos/mod.rs:925-928`) → `view_handlers::magnify` (`:414-447`) → `inject_native_gesture(Pinch{scale = 1+magnification, current_distance = 100*scale, duration_ms: 0})` → `determine_all_events` emits `PinchIn/PinchOut` (`event_determination.rs:1033-1047`) | handler calls `info.get_pinch()` | **no** (G2, G3) |
| Trackpad rotate | `rotateWithEvent:` → `NativeGestureEvent::Rotation` → `RotateClockwise/CounterClockwise` | map does not subscribe; `bearing_deg` is never used by the renderer | no |
| 3-finger swipe | nothing: no `swipeWithEvent:`, no `touchesBegan/Moved/EndedWithEvent:`, no `allowedTouchTypes` in `macos/mod.rs` (grep) | — | no |
| Touch (iOS/Android) | `Touch*` handlers exist only on the OUTER div (`map.rs:373-394`) | shadowed (G3) | pan works only via the synthesized mouse events |

**G1 — wheel/trackpad zoom runaway.** `map_on_scroll` ignores the magnitude
(`map.rs:912`) and the source. macOS classifies precise deltas as
`TrackpadContinuous` and the momentum tail as the same (`macos/events.rs:
509-532`; only phase `Ended/Cancelled` becomes `TrackpadEnd`), and
`record_scroll_from_hit_test` records the raw delta into
`pending_wheel_event` for every event (`scroll_state.rs:621`). A single
flick is dozens of events → dozens of half-levels, continuing after the
fingers lift, until the clamp (→ Z1). Sign is fine: AppKit `scrollingDeltaY > 0`
is "scroll up" = DOM `deltaY < 0` = Leaflet zoom-in.

**G2 — native gesture cleared before dispatch.** In
`process_window_events_inner` the injected native gesture is cleared at
`common/event.rs:7117` (`clear_native_gesture()`), but user callbacks are
dispatched later in the same pass at `:7475`
(`dispatch_events_propagated(&pre_filter.user_events)`). The map's
`map_on_pointer_move` reads `info.get_pinch()` (`map.rs:776`) →
`detect_pinch()` (`layout/src/managers/gesture.rs:1238-1242`) → the native
slot is already `None`, the in-process fallback needs two live touch
sessions (`:1244-1256`) → `None` → the handler falls through to the pan
branch with no `drag_anchor` → `DoNothing`. Contrast the wheel delta, which
is deliberately cleared AFTER dispatch (`:7477-7480`). Same ordering bites
`get_rotation()`, and iOS/Android go through the same function.

**G3 — handlers on the wrong node.** The comment at `map.rs:1476-1481`
explains that events hit the VirtualView's nested DOM and "never bubble to
the outer div's handlers", and moved `Mouse*`+`Scroll` onto the grid — but
`PinchIn/PinchOut` (`:396-404`) and `TouchStart/Move/End/Cancel`
(`:372-394`) stayed on the outer div. Propagation is per-DOM
(`common/event.rs:6134-6139` looks up `layout_results[event.target.dom]`),
and the target is the deepest node of the HIGHEST DomId
(`layout/src/managers/hover.rs:20-32`), so while the cursor is over the map
those handlers are unreachable.

**G4 — the pinch math assumes cumulative distances.** `map.rs:780-786`
keeps `pinch_anchor` = last `current_distance` and zooms by
`log2(current/anchor)`. macOS magnify (and `DetectedPinch` from iOS/Android
per the injection sites) delivers a PER-EVENT delta: `current_distance =
100·(1+δ)` every event, so for a steady pinch the ratio is ~1 and the zoom
barely moves; the correct per-event step is `dz = log2(scale)`. The
in-process two-finger detector (`gesture.rs:1244-1299`) reports the
cumulative ratio since the gesture start, for which the anchor logic is
right — the two sources need to be told apart (`duration_ms == 0` marks the
native per-event kind at `macos/mod.rs:440`).

### 4.2 Fixes

* **G1:** continuous zoom: `dz = clamp(dy * ZOOM_PER_PX, -0.5, 0.5)` with
  `ZOOM_PER_PX = 1/60` (Leaflet's `wheelPxPerZoomLevel = 60`); a discrete
  notch arrives as `WHEEL_SCROLL_PIXELS_PER_LINE` px (`common/event.rs:1126-1132`)
  so one notch stays ≈ half a level. Momentum: either skip recording
  `pending_wheel_event` when `momentumPhase != None` (macOS only knows this
  at `macos/events.rs:515`; add a `ScrollInputSource::TrackpadMomentum`
  and do not set `pending_wheel_event` for it), or give `ScrollEventData`
  a `source` field the callback can read. Zoom about the cursor, not the
  centre: `P = latlon_at_px(vp, cursor, bounds)`; after changing zoom,
  shift the centre so `px_at_latlon(vp', P, bounds) == cursor`.
* **G2:** move `w.gesture_drag_manager.clear_native_gesture()` from
  `common/event.rs:7117` to after `dispatch_events_propagated` at `:7475`
  (next to the wheel-delta clear at `:7477-7480`). Keep the comment's
  intent (no re-fire on later passes) — clearing after dispatch satisfies
  it.
* **G3:** register `PinchIn/PinchOut`, `RotateClockwise/CounterClockwise`,
  `Touch*` on the grid in `map_widget_render` (`map.rs:1480-1508`) and
  delete the dead copies on the outer div (`:372-404`). Keep `AfterMount`
  on the outer div (it needs a mounted node with the dataset).
* **G4:** `if pinch.duration_ms == 0 { dz = pinch.scale.log2() } else { anchor logic }`;
  clear `pinch_anchor` on `TouchEnd` AND when a native per-event pinch is
  seen.
* **Rotation:** apply `bearing_deg`: rotate the grid container with
  `transform: rotate(bearing)` (GPU and CPU both handle affine), enlarge
  the visible range by the rotated bounding box (√2 worst case), rotate
  mouse deltas by `-bearing` before `pan_viewport`. Snap to north on
  double-tap of a compass badge (the demo already draws one).
* **Perf prerequisite for smooth gestures:** every render re-rasterises
  every `Ready` tile from its SVG string
  (`svg_string_to_dom` → `render_svg_to_imageref(svg, 256, 256)`,
  `map.rs:1013` called from `:1570` on each pointer move and each
  writeback) and allocates a fresh `ImageRef` (new texture upload). Cache
  the raster in the entry (`TileEntry::Ready { svg, image: Option<ImageRef> }`),
  re-raster only when the on-screen tile size crosses a power of two (so
  overzoom stays crisp instead of blurry upscaling).

### 4.3 A tilted (3D) view — honest scope

What exists:
* CSS `perspective()`, `rotateX/Y`, `rotate3d`, `matrix3d` parse and become
  a full 4×4 (`core/src/transform.rs:369-541`); `transform-origin` is
  applied (`core/src/gpu.rs:246`).
* **GPU path:** the matrix is pushed as a WebRender reference frame with a
  4×4 `LayoutTransform` (`dll/src/desktop/compositor2.rs:1356-1392`,
  `TransformStyle::Flat`) and WebRender rasterises perspective-projected
  planes and hit-tests through them. So `transform: perspective(900px)
  rotateX(55deg)` on the tile-grid container already draws a tilted map.
* **CPU path:** `PushReferenceFrame` keeps only the 2-D affine part
  (`layout/src/cpurender/raster.rs:2305-2311`: `m[0][0] m[0][1] m[1][0]
  m[1][1] m[3][0] m[3][1]`), and the CPU hit-tester resolves transforms the
  same way (`common/event.rs:2513-2520`). Headless/CPU backends would
  render the tilt as a flat skew and hit-test wrongly. Either add a
  perspective branch to the agg raster (divide by `w`) or declare tilt
  GPU-only and fall back to `pitch = 0` on CPU backends.

What "better tile calculation" means: with pitch θ about the screen's
horizontal centre line and perspective distance D, screen `sy` maps to
map-plane `my = sy·D / (D·cosθ − sy·sinθ)`; the horizon is at
`sy = D·cotθ`. The visible extent is the trapezoid obtained by
un-projecting the four viewport corners with `sy` clamped to
`0.85·D·cotθ`; the tile range is its bounding box at the integer zoom
(`visible_tile_range` gains a pitch parameter). Far rows cover many
tiles that are only a few px tall, so v1 caps pitch at 60° and v2 picks a
lower zoom per row (MapLibre's per-tile LOD). `latlon_at_px` and
`pan_viewport` must invert the same projection (drag deltas scale with
`sy`), and `px_at_latlon` must forward-project for pins. Labels (none
today) would be the next problem.

Input for it on macOS: there is no 3-finger ingress, and a 3-finger
vertical swipe is Mission Control by default — an app cannot claim it
without the user changing System Settings. Options: (1) MapLibre's
convention, `Ctrl/⌘ + drag` (mouse) and two-finger vertical scroll with
`⌘` held → pitch; (2) opt-in raw trackpad touches via
`NSView.allowedTouchTypes = .indirect` + `touchesBegan/Moved/EndedWithEvent:`
→ count `touchesMatchingPhase` and synthesize a 3-finger `SwipeUp/Down`
into `inject_native_gesture` — works only when the system 3-finger
gestures are disabled/remapped to 4 fingers. Recommend (1) now, (2) as a
setting.

---

## 5. Themes: what exists, what the free providers offer, a preset API

### 5.1 Why it looks the way it does

`dll/src/desktop/extra/map/svg.rs`:
* `default_style` (`:61-78`) is seven substring rules on the MVT layer
  name: water, building, transportation(_name), park/landcover, boundary,
  else grey. No `class`/`subclass` (motorway = footpath = 0.8 px beige;
  every `landcover` class — wood, grass, ice, sand — is the same green;
  every boundary admin level is the same purple line at every zoom), no
  zoom-dependent widths, no casing, no labels (points are skipped, `:226-233`).
* Features are emitted in tile decode order (`:214-252`, the order
  `mvt-reader` walks layers), not in a style's layer order; the only
  guaranteed base is the opaque grey rect `#d6d8db` (`:183`) — so land is
  grey.
* `MapCss::parse` (`:95-145`) accepts `selector { fill; stroke; stroke-width }`
  keyed on the trailing selector token — no zoom ranges, no attribute
  filters, no layer order.
* The default source is OpenFreeMap's planet build pinned to a DATED path
  `…/planet/20260531_080002_pt/{z}/{x}/{y}.pbf` (`map.rs:96-104`). The
  TileJSON at `https://tiles.openfreemap.org/planet` currently points at
  `…/planet/20260816_080001_pt/{z}/{x}/{y}.pbf` (fetched 2026-08-22); the
  May path still serves a 642 KB tile today, but OpenFreeMap rebuilds weekly
  and the comment in the code already says the pinned path will go stale.
  Resolve `tiles[0]` from the TileJSON on first use (one request, cache it
  in the `MapTileCache`).
* The HTTP client identifies as `azul-http/1.0` (`layout/src/http.rs:176`);
  OSM's tile policy blocks generic user agents (below).
* Switching `url_template` does not invalidate `tiles` (`map.rs:643`
  adopts the layer, keys stay `(z,x,y)`): a theme switch would keep
  painting the previous provider's tiles. Key the cache by a layer
  generation counter or clear it when `url_template` changes.

### 5.2 Free providers (fetched 2026-08-22)

| Provider / style | Tiles | Key / limits | Licence + attribution (verbatim where quoted) | azul today |
|---|---|---|---|---|
| **OpenFreeMap** (planet vector, styles `liberty`, `bright`, `positron`, plus `dark`, `fiord`) | MVT `https://tiles.openfreemap.org/planet/<build>/{z}/{x}/{y}.pbf` z0-14 (resolve `<build>` via TileJSON `https://tiles.openfreemap.org/planet`); MapLibre styles `https://tiles.openfreemap.org/styles/{liberty,bright,positron}`; no raster | "there's no registration, no user database, no API keys" … "no limits on the number of map views or requests"; commercial OK; project MIT (https://openfreemap.org, https://github.com/hyperknot/openfreemap) | "Attribution is required … OpenFreeMap © OpenMapTiles Data from OpenStreetMap" (TileJSON `attribution` has the linked form). Styles are the OpenMapTiles GL styles: code BSD-3-Clause, design CC-BY 4.0 ("need not be provided on map images, but should be reasonably accessible") and OpenMapTiles wants "© OpenMapTiles © OpenStreetMap contributors" (https://github.com/openmaptiles/positron-gl-style/blob/master/LICENSE.md) | decodes today (`mvt.rs`); styling needs the style port (5.3) |
| **VersaTiles** (Shortbread schema, styles `colorful`, `graybeard`, `eclipse`, `neutrino`, `shadow`) | MVT `https://tiles.versatiles.org/tiles/osm/{z}/{x}/{y}` z0-14; styles `https://tiles.versatiles.org/assets/styles/<name>/style.json` | no key mentioned; docs recommend bundling styles as URLs may change (https://docs.versatiles.org/guides/use_tiles_versatiles_org.html) | TileJSON attribution: "© OpenStreetMap contributors · CC BY 4.0 ESA WorldCover 2021" | decodes; layer names differ (`water_polygons`, `streets`, `buildings`…) — the style port must key on the style's `source-layer`, not on substrings |
| **Protomaps** (basemap flavors `light`, `dark`, `white`, `grayscale`, `black`) | MVT via hosted API `https://api.protomaps.com/tiles/v4/{z}/{x}/{y}.mvt?key=MY_KEY` (https://protomaps.com/api) or self-hosted PMTiles (one ~120 GB planet file, z0-15, https://docs.protomaps.com/basemaps/downloads) | "The hosted API requires an API key"; free for non-commercial, soft cap 1,000,000 req/month, "For commercial use, become a GitHub Sponsor" (https://protomaps.com/blog/free-tier-maps/) | tiles ODbL: `<a href="https://osm.org/copyright">© OpenStreetMap</a>`; flavors: "© 2019-present Protomaps LLC"; styles CC0 (design) / BSD-3 (code) (https://github.com/protomaps/basemaps/blob/main/LICENSE.md) | decodes (`.mvt`); PMTiles needs a range-request reader (`http_get` has none) |
| **OpenStreetMap standard** | raster `https://tile.openstreetmap.org/{z}/{x}/{y}.png` z0-19 | no key, but the policy: "Send a clear, unique User-Agent string that names your app", honour `Cache-Control`/`Expires` (else cache ≥ 7 days), no bulk download, "Heavy or inappropriate use harms others' ability to edit and view the map. We may block access, without notice" (https://operations.osmfoundation.org/policies/tiles/). Fine for a demo; not for a shipped app with many users | "© OpenStreetMap contributors" | needs the raster path (5.3) |
| **CARTO** Positron / Dark Matter / Voyager | raster `https://{a-d}.basemaps.cartocdn.com/{light_all,dark_all,rastertiles/voyager}/{z}/{x}/{y}{@2x}.png` z0-20 (+`_nolabels`, `_only_labels`) (https://github.com/CartoDB/basemap-styles) | "An API key is required" (free, no account, https://carto.com/basemaps/apikey); "free to use up to a fair use limit of 5 million tile requests a month"; watermark without key; commercial = Enterprise licence (https://carto.com/basemaps) | `© <a href="http://www.openstreetmap.org/copyright">OpenStreetMap</a>, © <a href="https://carto.com/attributions">CARTO</a>` | raster path |
| **Stadia / Stamen** Toner, Terrain, Watercolor, Alidade Smooth (+Dark), Outdoors, OSM Bright | raster `https://tiles.stadiamaps.com/tiles/<style>/{z}/{x}/{y}{@2x}.png` (watercolor is `.jpg`), `?api_key=…` or `Authorization: Stadia-Auth` (https://docs.stadiamaps.com/raster/) | key required except localhost; domain auth alternative; free tier exists (limits not on the docs page) | "© Stadia Maps © OpenMapTiles © OpenStreetMap" + for Stamen styles "© Stamen Design" (Watercolor needs no OpenMapTiles credit) (https://docs.stadiamaps.com/attribution/) | raster path |
| **OpenTopoMap** | raster `https://{a,b,c}.tile.opentopomap.org/{z}/{x}/{y}.png` (z≤17) | no key; "provided the server is not overstressed by bulk downloads", no uptime guarantee (https://opentopomap.org/about) | CC-BY-SA: "Kartendaten: © OpenStreetMap-Mitwirkende, SRTM \| Kartendarstellung: © OpenTopoMap (CC-BY-SA)" | raster path |
| **MapTiler Cloud** | vector/raster with `?key=` | free plan 100 000 requests + 5 000 sessions/month, service pauses at the cap; free plan must show the MapTiler logo (https://www.maptiler.com/cloud/pricing/, https://docs.maptiler.com/guides/map-design/how-to-add-maptiler-attribution-to-a-map/) | "© MapTiler © OpenStreetMap contributors" | both paths |
| **Esri World Imagery / Street** | raster `https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{z}/{y}/{x}` (note `{y}/{x}`) | not free: usable only with an ArcGIS Online/Enterprise licence, "not available for commercial use" (https://community.esri.com/t5/arcgis-online-questions/terms-of-use-for-http-services-arcgisonline-com/td-p/601874) | "Source: Esri, Vantor, Earthstar Geographics, and the GIS User Community" | do not ship as a preset; document as bring-your-own-licence |

Recommendation: ship **key-less presets by default** — OpenFreeMap
(3 vector themes) and VersaTiles (4 vector themes) — plus OpenTopoMap and
OSM standard as raster presets flagged "demo/low-volume only"; ship CARTO,
Stadia/Stamen, Protomaps-hosted and MapTiler as presets that REQUIRE a key
parameter and refuse to build without one. No Esri.

### 5.3 How to stop hand-rolling: port the style, not the palette

All the good free looks (OpenFreeMap's positron/bright/liberty, VersaTiles'
four, Protomaps' five) are **MapLibre style JSON** over a known vector
schema. Positron, for example, is 95 ordered layers with `source-layer`,
`filter` (`==`, `in`, `$type`, `all`), `minzoom`/`maxzoom`, and paint
`fill-color` / `line-color` / `line-width` with zoom stops (background
`rgb(242,243,240)`, water `rgb(194,200,202)`, buildings `rgb(234,234,229)`,
motorway casing `rgb(213,213,213)` 0–40 px by zoom, etc. — fetched from
`https://tiles.openfreemap.org/styles/positron`). That is exactly the
information `features_to_svg` lacks: layer ORDER, per-class filters, and
zoom-scaled widths.

Proposed replacement for `MapCss` (`svg.rs:95-163`): a minimal style-JSON
interpreter —
`layers[]` in order; `type ∈ {background, fill, line}` (skip `symbol`
until labels exist); `source-layer`; `filter` subset (`==`, `!=`, `in`,
`!in`, `has`, `!has`, `all`, `any`, `$type`); `minzoom`/`maxzoom`; paint
`fill-color`, `fill-opacity`, `fill-outline-color`, `line-color`,
`line-width`, `line-opacity`, `line-dasharray` with `stops` /
`interpolate` by zoom (evaluate at the tile's z, later at the fractional
viewport zoom). Emit one SVG group per style layer in style order. The
styles are BSD-3/CC-BY 4.0 (OpenMapTiles) and CC0 (Protomaps) — bundle
them as `include_str!` assets under `dll/src/desktop/extra/map/styles/`
with their LICENSE files, and credit the design in the attribution
overlay ("Style © OpenMapTiles" where CC-BY applies).

Raster presets need a second decode path in `tile_fetch_worker`
(`mod.rs:94-203`): `MapTileSource::RasterXyz` → `http_get` →
`decode_raw_image_from_any_bytes` (`layout/src/image.rs:70`) → `ImageRef`
→ `TileEntry::Ready { image }`; the render loop then skips
`svg_string_to_dom`. `@2x` selection from `hidpi_factor` (the VirtualView
info carries it, `map.rs:1365` via `get_bounds`).

### 5.4 Preset API sketch (api.json `widgets`)

```rust
#[repr(C, u8)] pub enum MapTileSource {
    /// {z}/{x}/{y}(.png|.jpg) raster, optional {s} subdomain, optional {r}=@2x
    RasterXyz { url_template: AzString, subdomains: AzString, tile_size: u16 },
    /// MVT {z}/{x}/{y}.pbf|.mvt + a bundled or remote MapLibre style
    VectorMvt { url_template: AzString, style_json: AzString /* bundled */ },
    /// TileJSON endpoint; `tiles[0]` is resolved on the worker before the first fetch
    TileJson { url: AzString, style_json: AzString },
}

#[repr(C)] pub struct MapTileLayer {           // extends today's struct (map.rs:68-86)
    pub source: MapTileSource,
    pub min_zoom: u8, pub max_native_zoom: u8, pub max_zoom: u8,
    pub attribution: AzString,                // plain text shown in the overlay
    pub attribution_url: AzString,            // opened on click
    pub user_agent: AzString,                 // "AzulMaps/0.2 (+https://azul.rs)" — OSM policy
    pub api_key: AzString,                    // substituted into {key}; empty = none
    pub style_css: AzString,                  // keep for BC; ignored when style_json is set
}

#[repr(C, u8)] pub enum MapTheme {
    OpenFreeMapLiberty, OpenFreeMapBright, OpenFreeMapPositron,   // key-less, vector
    VersaTilesColorful, VersaTilesGraybeard, VersaTilesEclipse, VersaTilesNeutrino, // key-less, vector
    OpenStreetMapStandard, OpenTopoMap,                          // key-less raster, low-volume
    CartoPositron, CartoDarkMatter, CartoVoyager,                // key required
    StadiaAlidadeSmooth, StadiaAlidadeSmoothDark, StamenToner, StamenTerrain, StamenWatercolor, // key
    ProtomapsLight, ProtomapsDark, ProtomapsWhite, ProtomapsGrayscale, ProtomapsBlack,          // key
}

impl MapTileLayer {
    pub fn from_theme(theme: MapTheme) -> MapTileLayer;                  // key-less presets
    pub fn from_theme_with_key(theme: MapTheme, api_key: AzString) -> MapTileLayer;
    pub fn requires_api_key(theme: MapTheme) -> bool;
}
impl MapWidget {
    pub fn with_attribution_overlay(self, show: bool) -> Self;   // default true
}
```

Attribution overlay: rendered by the WIDGET, not the app, as the last
child of the grid `Dom` in `map_widget_render` (it must live inside the
VirtualView's nested DOM — that pipeline is composited on top of the
outer div, so an overlay on the outer div would be painted under the
tiles): `position:absolute; right:0; bottom:0; padding:0 5px; font:11px
sans-serif; background:rgba(255,255,255,.8); color:#333` (Leaflet's
`.leaflet-control-attribution`), text = `layer.attribution`, a `Click`
callback opening `attribution_url`. The demo's own `ATTRIB` div
(`lib.rs:181-183`, `:465-469`) then goes away. Each preset's string is
the provider's required form (table above); for CC-BY styles append
"Style © OpenMapTiles".

---

## 6. How to verify

Unit (all `cargo test -p azul-layout widgets::map`, no network; the
harness `with_callback_info_at` / `with_virtual_view_info` at
`map.rs:2046-2127` already builds a real `CallbackInfo`):

* §2: `pan_up_increases_latitude_in_both_hemispheres`,
  `pan_px_is_the_inverse_of_latlon_at_px` (above).
* §3: the three `spawn_pending_…` ordering tests (above) + `priority()`
  table.
* §1/Z2: `sweep_rerenders_when_the_viewport_changed_behind_the_render`:
  render once at zoom 2 (`rendered_viewport` stamped), set
  `cache.viewport.zoom = 3`, run `map_fetch_sweep_tick` through
  `with_callback_info`, assert a `CallbackChange::UpdateAllVirtualViews`
  was recorded (pattern: `after_mount_installs_the_sweep_timer_and_asks_for_a_re_render`, `map.rs:3860`).
* §4/G1: `scroll_zoom_is_proportional_and_bounded`: `dy = 3` → `|dz| ≈ 0.05`,
  `dy = 3000` → `|dz| = 0.5`.
* §4/G4: `native_pinch_delta_zooms_by_log2_scale`: inject `DetectedPinch
  { scale: 1.1, duration_ms: 0 }` via `CallbackInfo::inject_native_gesture`
  (`layout/src/callbacks.rs:4050`) — note the harness reads the live
  manager, so this test also pins G2's fix if it runs the full pass in
  `dll` (see below).
* §4/G2 (dll): a `headless_lifecycle`-style test
  (`dll/tests/headless_lifecycle.rs`) that injects
  `NativeGestureEvent::Pinch`, runs `process_window_events(0)`, and asserts
  a `PinchOut` callback observed `get_pinch().is_some()`.
* §1/Z1: `overzoom_renders_native_tiles_scaled`: layer `max_native_zoom
  14, max_zoom 18`, viewport zoom 16 → rendered ids are `z14` and each tile
  div is 1024 px (pattern: `render_clamps_an_out_of_band_zoom_and_stays_bounded`, `map.rs:4012`).

Headless e2e against the real demo binary
(`AZ_E2E=<spec.json> AZ_BACKEND=headless ./AzMaps`, the recipe in
`.github/workflows/rust.yml:3831-3836`; ops from `layout/src/e2e/full.rs:1832-1891`
— `click` by `text`, `mouse_down/move/up`, `scroll`, `assert_text`,
`assert_exists`, `get_dom_tree`). Point the layer at an invalid host
(`AZ_MAP_TILE_URL` env override is worth adding to the demo) so tiles go
`Failed` fast and the grid labels `✗ z{z}/{x}/{y}` are deterministic:

```json
[
 {"name":"map_plus_zooms","steps":[
  {"op":"wait","ms":300},
  {"op":"assert_text","selector":"body","contains":"zoom 2.0"},
  {"op":"click","text":"+"}, {"op":"wait","ms":300},
  {"op":"assert_text","selector":"body","contains":"zoom 3.0"},
  {"op":"get_dom_tree"},                       // nested VirtualView DOM must list "z3/" tile labels
  {"op":"assert_exists","selector":"[text*=\"z3/\"]"}
 ]},
 {"name":"map_plus_after_drag_out_of_window","steps":[
  {"op":"mouse_down","x":400,"y":300}, {"op":"mouse_move","x":420,"y":320},
  {"op":"mouse_move","x":-20,"y":320}, {"op":"mouse_up","x":-20,"y":320},
  {"op":"click","text":"+"}, {"op":"wait","ms":300},
  {"op":"assert_text","selector":"body","contains":"zoom 4.0"}
 ]},
 {"name":"map_up_goes_north","steps":[
  {"op":"click","text":"↑"}, {"op":"wait","ms":200},
  {"op":"assert_text","selector":"body","contains":"centre 60."}   // from 37.7749 at z2 a half-tile step lands north of 37.8
 ]},
 {"name":"map_wheel_is_bounded","steps":[
  {"op":"scroll","x":400,"y":300,"delta_x":0,"delta_y":3},          // one trackpad tick
  {"op":"wait","ms":100},
  {"op":"assert_text","selector":"body","contains":"zoom 2.0"}      // not 2.5 (after G1: 2.05 rounds to 2.0/2.1)
 ]}
]
```

(Verify first that `find_node_by_text`/`assert_exists` descend into
VirtualView child DOMs; if not, assert on the header line only.)

Manual on macOS: `AZ_MAP_DEBUG=1 ./AzMaps` — one two-finger flick should
now print a handful of `[map] scroll fired dy=…` lines with small `dz`,
not forty; pinch should print `pointer_move fired` with a zoom change;
pressing "+" at the top of the range should show a greyed button.

## 7. Effort

| Item | Effort |
|---|---|
| §2 sign fix (a) | 10 min; (b) with `pan_viewport_px` on api.json + demo rewrite: 1-2 h |
| Z1 `max_native_zoom`/`max_zoom` split + greyed button | 2 h |
| Z2 widget-side `rendered_viewport` check | 1 h; framework-side re-invoke on fast-path merge: 3-4 h incl. a `core/src/diff.rs` + `common/layout.rs` test |
| Z4 hook `Update` propagation, Z5, `on_pin_tap` merge | 1 h |
| §3 centre-out priority + stale drop | 3 h; cancellation via `ThreadId` + 6-in-flight pool + parent-tile retain: +1 day |
| G1 proportional wheel + momentum guard + zoom-about-cursor | 3-4 h (momentum needs a `ScrollInputSource` variant across the 5 shells) |
| G2 move `clear_native_gesture` | 30 min + dll test 1 h |
| G3 handlers onto the grid | 1 h |
| G4 per-event pinch math | 1 h |
| Raster cache (`ImageRef` per tile) | 3 h |
| Rotation (bearing) | 1 day |
| Tilt v1 (GPU only, capped pitch, corner un-projection, ⌘+drag) | 3-4 days; CPU perspective raster + hit-test: +3 days; per-row LOD: +1 week |
| Style-JSON interpreter (fill/line/background, filters, zoom stops) + 7 bundled key-less themes + attribution overlay + `MapTheme` API through api.json/codegen | 1-1.5 weeks |
| Raster XYZ source path + key-required presets | 2-3 days |
| TileJSON resolution + UA + cache invalidation on layer change | 1 day |

## 8. Overlaps

* `scripts/BUGS_2026_08_22_input_state_stuck.md` — the framework-level
  stuck button. Nothing here depends on it; Z3 explains why the map cannot
  be what wedges "+".
* The Slider `MouseLeave`-bubbling fix on this branch (commit
  `b44804467`) — same event model; the map is unaffected because its tile
  divs are untagged, but if tiles ever gain `:hover` styling or callbacks
  the drag will break exactly like the slider did. Keep tiles inert.
* `core/src/diff.rs:1149` `merge_fresh_dataset` / `common/layout.rs:474`
  (same branch's "identical rebuild keeps widget state") — Z2 is the
  missing second half of that fix for dataset-driven VirtualViews.
* Gesture clearing (`common/event.rs:7117`) is shared by iOS/Android
  pinch; fixing G2 changes their behaviour too (for the better — the
  native override becomes observable in callbacks).
* The open "capture tile repaint (NullImage after ChangeNodeImage)" note in
  memory is unrelated to map tiles (they are DOM-rebuilt `create_image`
  nodes, not `ChangeNodeImage`).

---

Sources used for §5:
https://openfreemap.org · https://openfreemap.org/quick_start/ ·
https://tiles.openfreemap.org/planet · https://tiles.openfreemap.org/styles/positron ·
https://github.com/hyperknot/openfreemap ·
https://github.com/openmaptiles/positron-gl-style/blob/master/LICENSE.md ·
https://docs.versatiles.org/guides/use_tiles_versatiles_org.html ·
https://tiles.versatiles.org/tiles/osm/tiles.json · https://versatiles.org/ ·
https://docs.protomaps.com/basemaps/downloads · https://docs.protomaps.com/basemaps/flavors ·
https://protomaps.com/api · https://protomaps.com/blog/free-tier-maps/ ·
https://github.com/protomaps/basemaps/blob/main/LICENSE.md ·
https://operations.osmfoundation.org/policies/tiles/ ·
https://github.com/CartoDB/basemap-styles · https://carto.com/basemaps ·
https://docs.stadiamaps.com/raster/ · https://docs.stadiamaps.com/attribution/ ·
https://opentopomap.org/about · https://www.maptiler.com/cloud/pricing/ ·
https://docs.maptiler.com/guides/map-design/how-to-add-maptiler-attribution-to-a-map/ ·
https://community.esri.com/t5/arcgis-online-questions/terms-of-use-for-http-services-arcgisonline-com/td-p/601874

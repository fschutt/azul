# Widgets + demo-fixes plan (branch `feat/widgets-and-demo-fixes`)

Base: `18a1f85ef` (clean pre-wall, like the highlevel-items branch). Local; merge later.
User request 2026-06-20 (max effort): fix paint HiDPI click + maps jumbled tiles; replace the
"Wasserwaage" (azul-spirit-level) release demo with an `azul-widgets` showcase of ALL widgets
(based on `examples/c/widgets.c`); research framework gaps + build many NEW widgets (each a
file, following existing patterns); link the showcase on the releases page.

## Workstream status
| # | Item | Status |
|---|------|--------|
| W1 | Paint HiDPI click fix | DONE (ebd19c71a) |
| W2 | Maps jumbled-tiles fix | DONE — fractional-zoom contiguous tile sizing (map.rs) + f32→f64 MVT projection (mvt.rs); see W2 note |
| W3 | Widget gap research | DONE (→ scripts/WIDGETS_RESEARCH.md) |
| W4 | Build new widgets (queue below; one file each) | DONE — 24/24 widget queue complete (date_picker + time_picker finish Tier3) |
| W5 | `azul-widgets` showcase demo crate (from widgets.c) | **BLOCKED** — needs api.json export of the 24 new widgets first (see BLOCKER below); awaiting user decision |
| W6 | Release-page swap: remove spirit-level, add azul-widgets | BLOCKED (depends on W5) |

## ⛔ BLOCKER (found 2026-06-23, cron PAUSED here) — W5/W6/export-wins all need api.json export
`azul::widgets` / `azul::dom::Dom` that every Rust + C demo uses are GENERATED from api.json:
dll/src/lib.rs:213 `include!(target/codegen/reexports.rs)`, and reexports.rs re-exports the FFI
MIRROR types (`pub use crate::ffi::dll::Az* as *`), NOT azul_core/azul_layout (grep count = 0).
So `azul::dom::Dom` == `AzDom` (a distinct `#[repr(C)]` mirror), and `azul::widgets::X` exists ONLY
for widgets in api.json (export generates the `AzX` mirror + the `azul::widgets::X` re-export).
The 24 new widgets are in `azul_layout::widgets` and produce `azul_core::dom::Dom` — a DIFFERENT type
from the `AzDom` an `azul` app's layout callback returns. A direct azul-layout dep does NOT compose
(type mismatch; only unsafe transmute would bridge it — unacceptable). ⇒ The showcase CANNOT display
the new widgets until they're exported to api.json (via azul-doc autofix — the sanctioned tool, the
"batch autofix later" the plan always intended). This unblocks W5 + W6 AND makes the widgets usable
from C/widgets.c + every language binding. Decision needed: run that export now, or defer the showcase.

## CRON QUEUE ORDER (one item per fire) — PAUSED at the BLOCKER above
1. **W2 maps fix** (DONE). 2. **W4 widgets** (DONE 24/24). 3. **api.json export of 24 widgets** (NEW —
the unblock step, pending user OK). 4. **W5 showcase**. 5. **W6 release-page swap**.

## W2 — maps jumbled tiles (do first)
azul-maps tiles are present but jumbled/disconnected (no coherent map). Investigate the tile→screen
grid math + tile-node CSS positioning in `layout/src/widgets/map.rs` + `examples/azul-maps/`. Fix so
tiles tile contiguously (each at `left = col*tile_px - offset_x`, `top = row*tile_px - offset_y`, sized
`tile_px`). Likely culprits: tile index used directly instead of `index*tile_size`, missing viewport
offset, x/y swap, wrong tile size, or tile nodes not `position:absolute` in a positioned container.
Verify `cargo build -p azul-maps`. (Runtime needs a GUI + live tiles — compile-only here.)

**RESOLUTION (this session).** Traced the whole pipeline (map.rs grid math → MVT decode → SVG
project → cpurender VirtualView composite). The *classic* slippy-map bugs the brief lists are all
ABSENT: `screen = (col − centre)*tile_px + span/2` is correct, viewport offset present, x/y not
swapped, tiles are `position:absolute` inside the positioned `position:absolute` grid. At the demo's
default **integer** zoom (z2) the grid is provably pixel-perfect (consecutive origins differ by exactly
256 and `size=256`), which is why commit 1315bf619 verified coherent continents. The two real,
zoom-dependent defects that fracture the map into a disconnected jumble:
- **map.rs `map_widget_render` — fractional-zoom seams.** Tile *size* was a fixed `tile_px.round()`
  while each tile *origin* is rounded independently; once `tile_px` isn't a whole number (any non-integer
  zoom — every scroll-wheel notch is 0.5), the fixed size drifts out of step with the rounded origins →
  gaps/overlaps. FIX: derive each tile's box from `round(next_origin) − round(this_origin)` per axis, so
  neighbours always share an exact edge at every zoom (identical to the old size at integer zoom).
- **mvt.rs `tile_pixel_to_lat_lng` — f32 precision.** Global pixel coord is `2^z * 4096` (≈6.7e7 at z14,
  max_zoom), past f32's exact-integer ceiling (2^24≈1.6e7), so tile-boundary multiples + per-pixel terms
  snap to a coarse grid and adjacent tiles' shared edges stop aligning → coastlines/borders fracture at
  street zooms. FIX: compute + return in f64 (the downstream SVG projection is already f64).
Both compile (`cargo build -p azul-maps` ✓, `cargo check -p azul-layout` ✓) and all map+MVT unit tests
pass (19 + 7). RUNTIME CAVEAT: not GUI-verified here. If a *first-load (z2)* jumble persists, the cause
is NOT the grid math (proven correct) — chase the cpurender VirtualView child-DOM composite instead.

## W4 — widget build queue (recipe + per-widget detail in scripts/WIDGETS_RESEARCH.md)
Each = its own file in `layout/src/widgets/`; follow the recipe (struct → callbacks via
`impl_widget_callback!`+`impl_managed_callback!` → builders → `.dom()`+internal handler), register in
`mod.rs` + api.json (via **azul-doc autofix**, NOT hand-edits), compile-verify (`cargo check -p azul-layout`
+ `build-dll`). Tick `[x]` when DONE + committed. Easiest-first order:
- Tier1: `[x]` switch `[x]` divider `[x]` card `[x]` badge `[x]` slider `[x]` segmented `[x]` radio_group `[x]` tooltip `[x]` text_area
- Tier2: `[x]` alert `[x]` accordion `[x]` avatar `[x]` chip `[x]` spinner `[x]` popover `[x]` combobox `[x]` modal
- Tier3: `[x]` toast `[x]` breadcrumb `[x]` pagination `[x]` stepper `[x]` split_pane `[x]` date_picker `[x]` time_picker
- Export wins: `[ ]` Label→api.json `[ ]` TabContent→api.json

## W6 — release-page edit (mechanism from investigation; do AFTER the azul-widgets crate exists)
The demo list is duplicated in 6 places — sync ALL (same order):
1. `doc/src/dllgen/deploy.rs:1542-1569` — `DEMO_APPS: &[(&str,&str,&str)]` const (the release-page
   HTML source, rendered by `generate_release_html()` @1283). Remove
   `("azul-spirit-level","AzSpiritLevel",...)`; add `("azul-widgets","AzWidgets","a showcase of all Azul widgets")`.
2. `Cargo.toml:9` — workspace members: remove `"examples/azul-spirit-level"`, add `"examples/azul-widgets"`.
3. `.github/workflows/rust.yml:1302` + `:1313` — `build_demos` build + staging loops (`for demo in ...`).
4. `.github/workflows/rust.yml:1773` — `build_mobile_apps` (iOS) loop.
5. `.github/workflows/rust.yml:1845` — `build_mobile_apps_android` loop (only azul-maps/paint/self-test
   today; add azul-widgets ONLY if it has Android lib.rs/cdylib support).
6. `doc/src/main.rs:1704-1708` — Dockerfile-copy loop (for the "Web (Docker)" release links; needs
   `examples/azul-widgets/Dockerfile`).
New demo crate `examples/azul-widgets/`: Cargo.toml (bin) + Dockerfile + src/main.rs (+ src/lib.rs if
Android), mirror `examples/azul-paint/`. The release page is generated programmatically (no HTML
templates to edit).

## Notes
- Conservative on rendering. Compile-verify each (host build-dll; mobile via cross-compile target-scoped env).
- ENOSPC: `rm -rf target` (~38G recurring). Commit per item; do NOT push (user merges later).
- After this + the Monday clippy fixes + another release, user will report which widgets misbehave.

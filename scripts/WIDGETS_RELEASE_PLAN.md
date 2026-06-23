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
| W2 | Maps jumbled-tiles fix | TODO (cron item 1; prior agents died on transient API 500) |
| W3 | Widget gap research | DONE (→ scripts/WIDGETS_RESEARCH.md) |
| W4 | Build new widgets (queue below; one file each) | TODO (cron churns one/fire) |
| W5 | `azul-widgets` showcase demo crate (from widgets.c) | TODO (after W4) |
| W6 | Release-page swap: remove spirit-level, add azul-widgets | TODO (after W5) |

## CRON QUEUE ORDER (one item per fire; cron self-deletes when ALL done)
1. **W2 maps fix** (spec below). 2. **W4 widgets** — lowest unchecked in the W4 queue, one per fire,
per the recipe in `scripts/WIDGETS_RESEARCH.md`. 3. **W5 showcase**. 4. **W6 release-page swap**.

## W2 — maps jumbled tiles (do first)
azul-maps tiles are present but jumbled/disconnected (no coherent map). Investigate the tile→screen
grid math + tile-node CSS positioning in `layout/src/widgets/map.rs` + `examples/azul-maps/`. Fix so
tiles tile contiguously (each at `left = col*tile_px - offset_x`, `top = row*tile_px - offset_y`, sized
`tile_px`). Likely culprits: tile index used directly instead of `index*tile_size`, missing viewport
offset, x/y swap, wrong tile size, or tile nodes not `position:absolute` in a positioned container.
Verify `cargo build -p azul-maps`. (Runtime needs a GUI + live tiles — compile-only here.)

## W4 — widget build queue (recipe + per-widget detail in scripts/WIDGETS_RESEARCH.md)
Each = its own file in `layout/src/widgets/`; follow the recipe (struct → callbacks via
`impl_widget_callback!`+`impl_managed_callback!` → builders → `.dom()`+internal handler), register in
`mod.rs` + api.json (via **azul-doc autofix**, NOT hand-edits), compile-verify (`cargo check -p azul-layout`
+ `build-dll`). Tick `[x]` when DONE + committed. Easiest-first order:
- Tier1: `[ ]` switch `[ ]` divider `[ ]` card `[ ]` badge `[ ]` slider `[ ]` segmented `[ ]` radio_group `[ ]` tooltip `[ ]` text_area
- Tier2: `[ ]` alert `[ ]` accordion `[ ]` avatar `[ ]` chip `[ ]` spinner `[ ]` popover `[ ]` combobox `[ ]` modal
- Tier3: `[ ]` toast `[ ]` breadcrumb `[ ]` pagination `[ ]` stepper `[ ]` split_pane `[ ]` date_picker `[ ]` time_picker
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

# Widgets + demo-fixes plan (branch `feat/widgets-and-demo-fixes`)

Base: `18a1f85ef` (clean pre-wall, like the highlevel-items branch). Local; merge later.
User request 2026-06-20 (max effort): fix paint HiDPI click + maps jumbled tiles; replace the
"Wasserwaage" (azul-spirit-level) release demo with an `azul-widgets` showcase of ALL widgets
(based on `examples/c/widgets.c`); research framework gaps + build many NEW widgets (each a
file, following existing patterns); link the showcase on the releases page.

## Workstream status
| # | Item | Status |
|---|------|--------|
| W1 | Paint HiDPI click fix | IN PROGRESS (agent) |
| W2 | Maps jumbled-tiles fix | QUEUED (after W1 frees cargo) |
| W3 | Widget gap research | IN PROGRESS (agent) → fills W4 |
| W4 | Build new widgets (one file each, per gap-list) | PENDING research |
| W5 | `azul-widgets` showcase demo crate (from widgets.c) | PENDING W4 |
| W6 | Release-page swap: remove spirit-level, add azul-widgets | PENDING W5 |

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

## W3/W4 — widget gap-list (TO FILL from research)
Current azul widgets (layout/src/widgets/): button, check_box, color_input, drop_down, file_input,
frame, label, list_view, map, menubar, node_graph, number_input, progressbar, ribbon, tabs,
text_input, titlebar, tree_view (+ media: camera/microphone/screencap/video). widgets.c showcases a
subset. >> GAP-LIST + the "add a widget" recipe land from the research agent; build each new widget as
its own file in layout/src/widgets/, register in mod.rs + api.json, follow the existing struct→dom()→
impl_managed_callback! pattern.

## Notes
- Conservative on rendering. Compile-verify each (host build-dll; mobile via cross-compile target-scoped env).
- ENOSPC: `rm -rf target` (~38G recurring). Commit per item; do NOT push (user merges later).
- After this + the Monday clippy fixes + another release, user will report which widgets misbehave.

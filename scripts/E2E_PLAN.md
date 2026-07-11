# E2E Plan — Headless Redraw-Correctness Testing at Scale

Status: **plan / not implemented** (2026-07-11). Scope: everything **up to and including the
headless window**. Target bug class: **stale updates, infinite redraw loops, resource leaks,
and the `layout/src/managers/` state-staleness family.**

> **Read `scripts/DAMAGE_REGION_PLAN.md` first.** It is the architecture doc for damage. This
> document does **not** re-plan damage; it plans how to **test** it at scale. Where the two
> overlap, DAMAGE_REGION_PLAN wins on architecture, this one wins on harness.

---

## 0. TL;DR — what exists, what must be added

The single most important finding: **the headless window is not a stub or a simulation. It is
the engine.** `CpuBackend::render_frame` (`dll/src/desktop/shell2/headless/mod.rs:357`) is the
*same* CPU render + damage path that X11, Wayland, Win32, macOS, Android and iOS all call
(`linux/x11/mod.rs:3653`, `linux/wayland/mod.rs:4520`, `windows/mod.rs:933`,
`macos/mod.rs:6142`, `android/mod.rs:227`, `ios/mod.rs:1216`). The platform shells are thin
presenters: they take `CpuBackend.last_present_damage` and blit it. So testing damage headlessly
tests the real thing, on every platform, for free.

| Capability | Status | Evidence |
|---|---|---|
| JSON e2e harness (`AZ_E2E`) | **EXISTS**, runs in CI | `dll/src/desktop/shell2/run.rs:56`, `.github/workflows/rust.yml:1943` |
| 9 assertion ops | **EXISTS** | `debug_server/full.rs:3372-3382` |
| ~60 debug ops (click/key/scroll/resize/DOM-mutate) | **EXISTS** | `debug_server/full.rs:1526-2073` |
| Headless CPU render → `AzulPixmap` | **EXISTS** | `headless/mod.rs:357`, `:266` |
| Headless PNG dump | **EXISTS** (2 env hooks) | `headless/mod.rs:1249` (`AZ_HEADLESS_SNAPSHOT_PATH`), `:797` (`AZ_DUMP_FRAME_DIR`) |
| Screenshot from an assertion | **EXISTS** | `CallbackInfo::take_screenshot`, `layout/src/callbacks.rs:3008` |
| Pixel diff | **EXISTS** | `layout/src/cpurender/pixmap.rs:465` |
| Damage rects (paint + present) | **EXISTS in the engine**, **NOT queryable** | `headless/mod.rs:280`, `:287` — zero hits in `debug_server/full.rs` and `api.json` |
| Resource counters | **EXIST as `pub` fields**, **NOT queryable** | `core/src/resources.rs:1318-1348`; zero hits in `api.json` |
| Relayout-iteration counter | **DOES NOT EXIST — must be added** | cap exists (`event.rs:255`) but only `log_warn`s (`event.rs:4003`) |
| **Mount a DOM/CSS from the test file** | **DOES NOT EXIST — must be added** | no `mount`/`set_dom` op anywhere in `full.rs` |
| Deterministic embedded font | **EXISTS in Rust tests only** | `headless/mod.rs:1894-1932` |
| Native screenshot | **EXISTS**, but **impossible headless** | `native_screenshot.rs:29`; headless returns `RawWindowHandle::Unsupported` (`headless/mod.rs:1717`) |

**Four things must be added to azul. Everything else is harness work:**

1. **`FrameDamage` must become queryable.** It lives on `CpuBackend` (the *window*); assertions
   only ever see `CallbackInfo → LayoutWindow`. They cannot reach it. (§1.3)
2. **A frame-work counter** (relayout/repaint iterations, DOM regenerations, terminal
   `ProcessEventResult`) so an invalidation loop trips an assertion instead of being silently
   capped at depth 7 and logged. (§1.3)
3. **Resource counters exposed as an assertion** (`assert_resource_counts`). The fields are
   already `pub` and already reachable from `CallbackInfo::get_layout_window()` — this is the
   cheapest of the four. (§1.4)
4. **A `mount` op** (or an XML-driven test host). Without it, **every test runs against
   whatever DOM the app binary's `layout_callback` happens to build** — you cannot write a
   thousand *independent* CSS/layout/manager cases. This is the biggest gap and the thing that
   gates the whole "thousands of tests" idea. (§2.C.1)

**And one thing must be *removed*: `assert_screenshot` currently auto-baselines.** If the
reference PNG is missing it writes azul's own output as the reference **and returns `pass`**
(`full.rs:3877-3889`). That is bug-enshrinement compiled into the harness. It is already
happening: `tests/e2e/contenteditable_overflow_test.json` points at
`layout/tests/reference_images/contenteditable_overflow/`, **which does not exist**, so all 12
of its screenshot assertions silently auto-baseline to green today.

---

# PART 1 — GROUND TRUTH

## 1.1 The two harnesses (they are different things; the names are a trap)

| | `AZ_E2E` | `AZ_E2E_TEST` |
|---|---|---|
| File | `dll/src/desktop/shell2/run.rs:56,71` + `debug_server/full.rs` | `dll/src/desktop/shell2/common/e2e_test.rs` |
| Cargo feature | `debug-server` | `e2e-test` |
| **In CI?** | **YES** (`rust.yml:1943`) | **NO** — grep for `e2e-test` in `rust.yml` returns zero hits |
| What it does | Runs a JSON list of steps/assertions against a **live app** via the debug event queue | Takes over `main()`, builds its own `HeadlessWindow`, replays Resize/Tick, probes RSS |
| Assertions | 9 `assert_*` ops | one: RSS growth ceiling |
| DOM source | the app binary's `layout_callback` | the app binary's `layout_callback` |

`e2e_test.rs` is explicit about the split (`e2e_test.rs:9-11`). **The plan builds on `AZ_E2E`**
(the one that ships and runs in CI), and *harvests* `AZ_E2E_TEST`'s `breakdown_line()`
(`e2e_test.rs:370-549`) — ~40 hand-rolled counters — as the shopping list for the resource
assertions, because someone already did the work of finding every countable table.

### The `AZ_E2E` format, as actually implemented

```rust
// debug_server/full.rs:3218
pub struct E2eTest { name: String, description: Option<String>,
                     config: E2eConfig, setup: Option<E2eSetup>, steps: Vec<E2eStep> }
// full.rs:3203
pub struct E2eConfig { continue_on_failure: bool, delay_between_steps_ms: u64 }
// full.rs:3237
pub struct E2eSetup { window_width: u32 /*800*/, window_height: u32 /*600*/,
                      dpi: u32 /*96*/, app_state: Option<serde_json::Value> }
// full.rs:3263
pub struct E2eStep { op: String, screenshot: bool, #[serde(flatten)] params: serde_json::Value }
```

- A file is **one test object or an array of them** (`run.rs:82-92`).
- Params are **siblings of `op`**, not nested (`#[serde(flatten)]`, `full.rs:3270`).
- **Any** `DebugEvent` variant is a valid `op` — the enum is `#[serde(tag="op",
  rename_all="snake_case")]` (`full.rs:1525`) and non-assert steps are re-serialized straight
  back into it (`full.rs:4033-4045`). That is ~60 free ops: `click`, `mouse_move`, `key_down`,
  `text_input`, `scroll`, `resize`, `dpi_changed`, `insert_node`, `delete_node`,
  `set_node_text`, `set_node_classes`, `set_node_css_override`, `scroll_node_to`, `relayout`,
  `redraw`, `wait_frame`, `wait`, `take_screenshot`, `get_display_list`, `get_scroll_states`,
  `get_virtual_view_states`, `set_app_state`, …
- Assertions dispatch through `evaluate_assertion(op, params, &CallbackInfo, &RefAny)`
  (`full.rs:3360`): `assert_text`, `assert_exists`, `assert_not_exists`, `assert_node_count`,
  `assert_layout`, `assert_css`, `assert_app_state`, `assert_scroll`, `assert_screenshot`.

**Two dead/broken things in the existing format — fix or design around:**
- `E2eStep.screenshot: bool` (`full.rs:3268`) is **never read**. It is explicitly stripped from
  forwarded params (`full.rs:4039`) and every `E2eStepResult.screenshot` is hard-coded `None`.
  The guide (`doc/guide/en/e2e-testing.md:92`) documents it as working. It does not.
- `take_native_screenshot` is a **unit variant** (`full.rs:1845`) — the `"save_actual"` key in
  `tests/e2e/widgets_native_test.json` is silently dropped on the floor.

**Also: `doc/guide/en/e2e-testing.md:160` documents `AZUL_HEADLESS=1`. That env var does not
exist anywhere in the repo.** The only selector is `AZ_BACKEND=headless`
(`common/compositor.rs:107`).

### The critical structural limit: **there is no way to mount a DOM**

The runner drives **the app binary's own `layout_callback`**. `tests/e2e/hello_world_counter.json`
asserts against hello-world's counter because that is the only DOM in the process. The DOM-mutation
ops (`insert_node`, `set_node_css_override`, …) mutate *that* DOM; they cannot replace it.

`layout/src/xml/mod.rs:196` already has `parse_xml_to_styled_dom(xml) -> Result<StyledDom, XmlError>`
(XML + embedded `<style>` → StyledDom). Nothing calls it from the shell. **This is the missing
primitive** and §2.C.1 proposes the (small) way to wire it.

## 1.2 The headless path and the platform boundary

**`HeadlessWindow`** — `headless/mod.rs:859`:
```rust
pub struct HeadlessWindow {
    pub common: CommonWindowState,   // :861 — the shared, cross-platform half
    pub cpu_backend: CpuBackend,     // :863 — replaces WebRender
    ...
}
```
`CommonWindowState` (`common/event.rs:772`) holds `layout_window: Option<LayoutWindow>` (`:774`),
`renderer_resources` (`:782`), `frame_needs_regeneration` (`:816`), `frame_relayout_only` (`:829`),
`display_list_dirty` (`:843`). Headless nulls the WebRender fields (`render_api`, `renderer`,
`hit_tester` → `None`, `headless/mod.rs:930-936`) and uses `CpuHitTester` instead (`:931`).

**The boundary is the `PlatformWindow` trait** (`common/event.rs:1074`). ~28 getters are
macro-generated (`impl_platform_window_getters!`, `common/event.rs:932`); each shell implements
only **11 genuinely platform-specific methods** (raw window handle, timers, threads, menus,
tooltips, `sync_window_state`). **Everything else — ~4500 lines including
`process_window_events` (`event.rs:4001`), hit-test dispatch, scroll physics, event
determination — is a *provided* default method on the trait, i.e. shared.**

**So the honest boundary is much higher than the user assumes.** "Headless window → real OS
window" is not a big remaining chunk of engine; it is window creation, a GL context, the native
event pump, and a blit. The layout/style/damage/callback/manager stack is *identical*.

Pixels: `regenerate_layout()` (`headless/mod.rs:982`) calls `cpu_backend.render_frame(...)`
(`:1055-1077`) → `cpu_backend.last_frame: Option<AzulPixmap>` (`:266`).
**`incremental_relayout()` does NOT render** (`common/layout.rs:842`) — a `Resize` step in the
`AZ_E2E_TEST` runner therefore never updates `last_frame`. Worth knowing before you write a
damage test that resizes.

The rasterizer is **agg-rust** (`layout/Cargo.toml:38`), not tiny-skia (removed —
`pixmap.rs:162` says so explicitly).

> **Do not build on `dll/src/desktop/shell2/common/cpu_compositor.rs`.** It is a 145-line dead
> stub whose `rasterize()` fills the buffer white and returns (`:49-53`). It is re-exported from
> `shell2::mod` and is easy to mistake for the renderer. `HeadlessWindow` never mentions it.

**`cpurender` is ON in CI.** `build-dll`→`_internal_deps` (`dll/Cargo.toml:531`) and
`link-static`→`_internal_deps` (`:581`), and `_internal_deps` contains `"cpurender"` (`:604`).
So the CI e2e binary (`--features build-dll,debug-server`, `rust.yml:1862`) has the full CPU
renderer, `last_frame`, the compositor, and `assert_screenshot`. Good — no feature work needed.

## 1.3 Damage / repaint — the machinery is real, and it is invisible

### What exists
`layout/src/cpurender/compositor.rs` is a genuine damage engine:

| Function | Line | Emits |
|---|---|---|
| `compute_display_list_damage(old, new, old_offsets, new_offsets)` | `:1616` | `Option<Vec<LogicalRect>>` — **`None` = full repaint required** |
| `gpu_value_damage(...)` | `:1250` | `GpuValueDamage { rects, needs_full }` (`:1231`) — scrollbar thumb / transforms live in the GPU value cache, the DL diff cannot see them |
| `compute_virtual_view_damage(...)` | `:1718` | `Vec<LogicalRect>` — async child-DOM re-render |
| `compute_resize_damage(ow,oh,nw,nh)` | `:1797` | grow-only right+bottom strips |
| `coalesce_damage_rects(&mut Vec<LogicalRect>)` | `:1744` | O(n²) merge, 8px gap |
| `scroll_shift_region(...)` | `:861` | memmove + exposed strips |
| `overlay_rects_after_frame(...)` | `:1323` | repaint items composited over a shifted frame |
| `render_display_list_damaged(..., damage_rects: &[LogicalRect])` | `raster.rs:1049` | the incremental rasterizer |

Orchestrated by `CpuBackend::render_frame` (`headless/mod.rs:357-837`), which records:
```rust
pub last_frame_damage:   FrameDamage,  // :280  PAINT damage — pixels re-rasterized
pub last_present_damage: FrameDamage,  // :287  PRESENT damage — pixels that changed on screen (⊇ paint)
```
```rust
pub enum FrameDamage { None, Rects(Vec<LogicalRect>), Full }   // :159
```
Both fields are **ungated** (not `#[cfg(cpurender)]`) and are `Clone + PartialEq + Debug`. The
paint/present split is real and load-bearing: a scroll memmoves the whole clip (large present)
but only *paints* a thin exposed strip (small paint) — `:829-834`.

The doc comment at `headless/mod.rs:278-279` says, verbatim: *"Recorded so the headless test
harness can assert on it without re-running the diff."* **The intent is already there. The
plumbing is not.**

### What does NOT exist: any way to see it from a test

- `debug_server/full.rs`: grep `damage|dirty|repaint|FrameDamage` → **zero real hits** (only
  error strings and comments).
- `api.json`: grep `damage|dirty|repaint|invalidat` → **zero azul hits** (all matches are
  WebRender `DebugState` flags or raw OpenGL constants).
- `e2e_test.rs:474-490` gets *closest* — it snapshots `cpu_layers`, `cpu_next_layer_id`,
  `cpu_prev_dl_items` — but **never reads `last_frame_damage` or `last_present_damage`**.
- The only damage assertions in the repo are `#[cfg(test)]` Rust tests inside `headless/mod.rs`
  (§1.3.1), unreachable from any external harness.

**The architectural blocker, stated precisely:**

> Assertions run as `evaluate_assertion(op, params, &CallbackInfo, &RefAny)` (`full.rs:3360`).
> `CallbackInfo::get_layout_window() -> &LayoutWindow` (`layout/src/callbacks.rs:1850`) — so an
> assertion can reach **`LayoutWindow`** and everything on it (font manager, renderer resources,
> every manager). **But `FrameDamage` lives on `CpuBackend`, which hangs off the *Window*
> (`HeadlessWindow.cpu_backend`), not off `LayoutWindow`.** There is no path from `CallbackInfo`
> to `CpuBackend`. That is why damage is unassertable, and it is the one non-trivial piece of
> plumbing this plan requires.

**Fix (Phase 1, small, and aligned with the existing plan):** `FrameDamage` is already described
as *"the seed of the unified `DamageRegion` type described in DAMAGE_REGION_PLAN.md"*
(`headless/mod.rs:150`). Move it down into `azul-core` (or `azul-layout`) and have the shell
write a per-frame report onto `LayoutWindow` after `render_frame` returns:

```rust
// NEW — azul_core (or azul_layout::window), stored on LayoutWindow
pub struct FrameReport {
    pub paint_damage:   FrameDamage,   // moved from dll/.../headless/mod.rs:159
    pub present_damage: FrameDamage,
    pub frame_index:    u64,
    // §1.3.2 — the loop detector
    pub relayout_iterations:  u32,   // times process_window_events recursed this event
    pub dom_regenerations:    u32,   // times layout_callback ran this event
    pub display_list_rebuilds: u32,
    pub terminal_result:      ProcessEventResult,
    pub hit_depth_cap:        bool,  // true if MAX_EVENT_RECURSION_DEPTH was hit
}
```
`render_frame` takes `layout_window: &LayoutWindow` (immutable, `headless/mod.rs:360`) so it
cannot write this itself — but its **caller** has `&mut` on both, so the copy is a two-line
addition at each `render_frame` call site (7 sites: headless, x11, wayland, windows, macos,
android, ios). Then `CallbackInfo::get_layout_window().frame_report` is readable from an
assertion, and **every platform gets damage assertions, not just headless.**

### 1.3.1 Prior art — the invariants already exist as hand-written Rust tests

This is the most important thing in this document. **The Tier-1 invariants below are not
speculative — they are already proven, one at a time, in `headless/mod.rs`'s `mod tests`
(`:1830-3603`, ~28 tests).** The plan is to *lift them into the data-driven JSON layer* so they
can be mass-generated, not to invent them.

| Existing Rust test | Line | Is the Tier-1 invariant… |
|---|---|---|
| `damage_idle_scrollbar_window_skips` | `:2951` | **(a) idle stability.** Its own message: *"an idle window with a scrollbar must skip (FrameDamage::None); non-None means the scrollbar produces false per-frame damage and idle windows burn CPU forever"* |
| `damage_noop_relayout_is_clean` | `:2101` | (a) idle stability |
| `damage_box_noop_clean` | `:2160` | (a) idle stability |
| `damage_mouse_move_no_change_is_clean` | `:2501` | (a) idle stability under input |
| `damage_text_change_repro` | `:2051` | **(b) liveness** — the stale-text bug (#11) |
| `damage_box_paint_change_is_local` | `:2128` | (c) damage soundness — tightness |
| `damage_structural_add_covers_new_node` | `:2348` | **(c) coverage** (under-paint) |
| `damage_disjoint_rects_do_not_erase_content_between` | `:2702` | **(c) coverage** (under-paint) |
| `damage_single_paint_in_large_grid_is_local` | `:2528` | **(c) tightness** (over-paint) |
| `damage_change_inside_scrolled_frame_repaints_at_viewport_position` | `:2841` | (c) coverage in scroll space |
| `png_scroll_vertical_fast_matches_full_render` | `:3450` | **(c) the exact full-vs-damage-driven pixel identity check** |
| `perf_noop_relayout_under_budget` | `:2182` | (d) bounded work |

Helpers already written: `make_window_with(state, cb)` (`:1934`), `step(window, event) ->
FrameDamage` (`:2387`), `damage_area(&FrameDamage) -> Option<f32>` (`:1969`), `damage_max_y`
(`:2275`), `sample_px` (`:2997`), `save_frame_png` (`:3014`), `pixmap_diff` (`:3025`).

`damage_area()` is private and would need `pub`. That is the entire delta on the helper side.

### 1.3.2 The loop detector: the cap exists, the counter does not

`MAX_EVENT_RECURSION_DEPTH = 7` (`common/event.rs:255`). On breach:
```rust
// common/event.rs:4003-4010
if depth >= MAX_EVENT_RECURSION_DEPTH {
    log_warn!(..., "[PlatformWindow] Max event recursion depth {} reached", ...);
    return ProcessEventResult::DoNothing;
}
```
**It logs and swallows.** An invalidation loop — the exact bug the user is chasing — currently
manifests as a `log_warn` nobody reads and a frame that silently stops converging. Nothing
counts it, nothing fails.

The escalation lattice is `ProcessEventResult` (`core/src/events.rs:82-95`): `DoNothing(0)` <
`ShouldReRenderCurrentWindow(1)` < `ShouldUpdateDisplayListCurrentWindow(2)` <
`UpdateHitTesterAndProcessAgain(3)` < `ShouldIncrementalRelayout(4)` <
`ShouldRegenerateDomCurrentWindow(5)` < `ShouldRegenerateDomAllWindows(6)`. `Ord` + `max_self()`
(`:124`) — handlers `max` their results together, so the frame takes the strongest action asked
for. `UpdateHitTesterAndProcessAgain` is explicitly documented as *"recurse until nothing has
changed anymore"* (`events.rs:86-88`) — **that is the loop.**

Adding `relayout_iterations` / `hit_depth_cap` to `FrameReport` turns a silent warning into a
failing assertion. This is Tier-1(d) and it is ~15 lines.

### 1.3.3 Two things the plan must NOT assume about damage

1. **Damage is computed post-hoc by re-diffing two display lists**, never from the layout dirty
   sets. `CompositorState::compute_damage(dirty_nodes, ...)` (`compositor.rs:355`) — the
   function that *would* bridge node-dirty → rects — **is never called from production code.**
   `DAMAGE_REGION_PLAN.md §4` is the plan to fix that; it is not fixed. So a damage test is
   testing *the DL diff*, not the invalidation logic. That is still worth testing (it is what
   ships), but name it honestly.
2. **`core/src/diff.rs`'s rich `NodeChangeSet` is computed and thrown away.** It is a `u32`
   bitflag set (`diff.rs:41`) with `needs_layout()` (`:122`) / `needs_paint()` (`:127`), but the
   **only** production consumer is `NodeDataFingerprint::diff` → tri-state `DirtyFlag`
   (`layout/src/solver3/cache.rs:1014`). `compute_node_changes` (`:161`), `ExtendedDiffResult`
   (`:148`), `reconcile_dom_with_changes` (`:1532`) and `ChangeAccumulator::merge_extended_diff`
   (`:1403`) are **test-only** — `doc/guide/en/internals/dom.md:200` documents them as the live
   pipeline, which is **aspirational, not real.** Do not write assertions against them.

## 1.4 Resources and the font leak — CONFIRMED, with the repo's own admission

**The user is right, and the codebase already knows it.** `core/src/resources.rs:1435-1462` is a
comment block titled `AUDIT-TODO (font GC, resources.rs font leak — 2026-07-08)`:

> *"Fonts and font instances are currently **NEVER** garbage-collected. … nothing ever removes
> fonts from `currently_registered_fonts` in the first place, and this helper itself has no
> callers. **No `DeleteFont` / `DeleteFontInstance` `ResourceUpdate` is ever emitted**, so
> WebRender font memory grows unbounded when an app cycles fonts (font pickers, editors, live
> CSS)."*

Independently verified, three ways:

**1. The insert/remove matrix is asymmetric.** (Live font path is in `dll/src/desktop/wr_translate2.rs`
— *not* `core/src/resources.rs`, whose `build_add_font_resource_updates` (`:3028`) has **zero
production callers**, tests only.)

| Table (`core/src/resources.rs`) | INSERT | REMOVE | Verdict |
|---|---|---|---|
| `currently_registered_images` `:1318` | `wr_translate2.rs:1833` | `wr_translate2.rs:1917` | GC'd ✅ |
| `image_key_map` `:1320` | `wr_translate2.rs:1838` | `wr_translate2.rs:1918` | GC'd ✅ |
| `image_last_seen_epoch` `:1327` | `wr_translate2.rs:1894` | `wr_translate2.rs:1919` | GC'd ✅ |
| **`currently_registered_fonts` `:1329`** | `wr_translate2.rs:1791-1801` | **NONE** | **LEAK** |
| **`font_hash_map` `:1348`** | `wr_translate2.rs:1787` | **NONE** | **LEAK** |
| **`font_id_map` `:1345`** | dead path only | dead fn only | **LEAK** |
| **`font_families_map` `:1343`** | dead path only | dead fn only | **LEAK** |
| `last_frame_registered_fonts` `:1337` | **NONE — zero write sites repo-wide** | none | **PHANTOM** |

**2. The frame loop GCs images and has no font counterpart.** Inside the *same function*,
`register_frame_resources` (`wr_translate2.rs:1770`):
- images → `collect_stale_image_deletes` (`:1848`, defined `:1883-1932`) — diffs the registry
  against this frame's live hashes, emits `DeleteImage` (`:1920`), evicts all three tables,
  with an `IMAGE_GC_KEEP_EPOCHS = 2` grace window (`:1876`).
- fonts → `collect_font_resource_updates` (`:1777`, defined `:1329-1480`) — **add-only**.

**There is no `collect_stale_font_deletes`. The asymmetry *is* the leak.**

**3. Both halves of the fix were written and never wired.**
- `LayoutWindow::scan_used_fonts()` (`layout/src/window.rs:2113`) — a `pub fn` returning exactly
  the `BTreeSet<FontKey>` a GC needs; its doc comment says *"Callers can diff the result against
  `renderer_resources.currently_registered_fonts` to find fonts that are no longer used."*
  **Zero callers repo-wide.** The scanner exists; the evictor does not.
- `FontManager::remove_font` (`layout/src/text3/cache.rs:1062`) — `pub`, **zero callers**.
- `remove_font_families_with_zero_references` (`core/src/resources.rs:1464`) —
  `#[allow(dead_code)]`, tests only, and inert by construction.

**The font *bytes* leak too, separately**, in `FontManager` (`layout/src/text3/cache.rs:759-793`):
`parsed_fonts` (`:768`, inserted `:1004`/`:1016`, `remove_font` never called),
`font_hash_to_families` (`:778`, inserted `layout/src/window.rs:1049`, **no removal**),
`font_chain_cache` (`:771`, `merge_font_chain_cache` `:911` extends unboundedly).

### Counters available for a leak assertion — TODAY, from `CallbackInfo`

All these fields are `pub` and reachable via `CallbackInfo::get_layout_window()`
(`layout/src/callbacks.rs:1850`). `e2e_test.rs:456-536` already reads all of them.

**Will show the leak (monotonic):** `renderer_resources.currently_registered_fonts.len()`,
`.font_hash_map.len()`, `.font_id_map.len()`, `.font_families_map.len()`,
`font_manager.parsed_fonts.lock().len()`, `.font_hash_to_families.len()`, `.font_chain_cache.len()`.

**Already return to baseline (control group — proves the assertion works):**
`.currently_registered_images.len()`, `.image_key_map.len()`.

**Trap:** `last_frame_registered_fonts.len()` (`resources.rs:1337`) is **permanently 0** — never
written. `e2e_test.rs:513` reports it as `mgr_renderer_last_frame_fonts`. **Do not build a leak
assertion on it**; it will always pass.

**Manager counters:** only 4 of 22 managers have `debug_counts()` —
`virtual_view.rs:95`, `hover.rs:69`, `scroll_state.rs:460`, `gesture.rs:538`. All four doc
comments say *"Used by `AZ_E2E_TEST` to watch for unbounded growth."* **Missing on
`focus_cursor`, `text_edit`, `gpu_state`, `undo_redo`, `a11y`, `drag_drop` — all of which hold
per-node state and are prime staleness suspects.** Adding `debug_counts()` to those six is
trivial and directly serves the "~50 manager bugs" goal.

`memory_report()` exists on exactly 4 types (`StyledDom` `core/src/styled_dom.rs:909`,
`Solver3LayoutCache` `layout/src/solver3/cache.rs:442`, `LayoutTree`
`layout/src/solver3/layout_tree.rs:828`, `TextLayoutCache` `layout/src/text3/cache.rs:5539`) —
**none on any manager**, confirming `e2e_test.rs:440`'s comment.

**Nothing is exposed through `api.json`.** Grep for `RendererResources|FontManager|memory_report|
currently_registered` → 0 hits. All of it must be added as an assertion op.

## 1.5 Screenshots

**Everything needed already exists.** `CallbackInfo::take_screenshot(dom_id) -> Vec<u8>` (PNG)
(`layout/src/callbacks.rs:3008`) → `AzulPixmap::encode_png` (`pixmap.rs:349`) /
`decode_png` (`:369`) → `pixel_diff(reference, test, threshold) -> PixelDiffResult`
(`pixmap.rs:465`, per-channel abs-delta, `diff_ratio()` `:451`).

Headless PNG dumps: `AZ_HEADLESS_SNAPSHOT_PATH` (`headless/mod.rs:1249` — first frame, then
close) and `AZ_DUMP_FRAME_DIR` (`headless/mod.rs:797` — every frame, capped at 40,
`frame_NNN_{inc|full}.png`). The `_inc`/`_full` suffix is a gift: it tells you which render path
produced each frame.

**Caveat:** `take_screenshot` builds a **fresh `GlyphCache::new()`** and re-renders the display
list from scratch (`callbacks.rs:3055-3056`). It therefore **bypasses `CpuBackend`'s incremental
/ damage path entirely.** That is *exactly what we want* for damage soundness — it is a free,
independent full-repaint oracle to diff the damage-driven `last_frame` against (§2.B.1c) — but it
means `assert_screenshot` **cannot catch an incremental-render bug today**, because it never
looks at the incremental buffer. Worth stating plainly: **`assert_screenshot` as it exists does
not test the damage path.**

**Native screenshot is impossible headless.** `NativeScreenshotExt` (`native_screenshot.rs:29`)
dispatches on `RawWindowHandle`; headless returns `RawWindowHandle::Unsupported`
(`headless/mod.rs:1717`). Wayland is unsupported even with a real window (`native_screenshot.rs:88`).
The only Wayland capture is the out-of-process `scripts/azshot.py` (portal screenshot) +
`doc/aztest`'s `azinput` (input injection). This is why native-vs-headless comparison is Phase 5
and is a *consistency check*, not an oracle.

**Chrome reftests exist and are real** (`doc/src/reftest/pipeline.rs`, CDP
`Page.captureScreenshot`, `compare_images` `doc/src/reftest/mod.rs:762`, pass gate
`PASS_THRESHOLD_PIXELS = (1920*1080)/200` `:308`, over 9,629 `.xht` files in `doc/xhtml1/`).
**Per the user's direction, they are OUT OF SCOPE for this layer** — they belong to the
layout-engine effort. Noted here only so nobody rebuilds them.

## 1.6 Sanity-check: "then the only final step is wiring to the actual window, right?"

**Mostly right, and more right than you'd expect — but with four real gaps.**

Right, because: the headless window *is* the engine (§1.2). Layout, style, display list, damage,
callbacks, managers, scroll physics, hit-testing and event determination are all shared trait
defaults, and all four desktop shells already consume `last_present_damage`. Getting these green
headlessly genuinely covers the platforms.

**What headless testing CANNOT cover — be honest about this residual risk:**

1. **The GPU/WebRender damage path is a *different system*.** WR has its own
   `PartialPresentDamage` (`wr_translate2.rs:197`) in `DeviceIntRect` (physical px) with
   buffer-age widening, vs the CPU path's `LogicalRect`. **There is no unified `DamageRegion`
   type** (that is DAMAGE_REGION_PLAN's goal, unfinished). Headless exercises **only the CPU
   half.** A GPU-only damage bug is invisible to this entire plan.
2. **Real input event delivery.** The e2e ops inject synthetic events straight onto the queue.
   They do not test X11 `XNextEvent` / Wayland `wl_pointer` / Win32 message translation, key
   repeat, modifier tracking, or click-vs-drag disambiguation at the OS layer. Those are exactly
   where per-platform input bugs live.
3. **Compositor/vsync/present timing.** Frame pacing, `CVDisplayLink` lifecycle, `wl_buffer`
   release, buffer-age, tearing, and "is a present required when damage is `None`?" (the
   `force_full` path, `headless/mod.rs:194-206`) are timing-dependent and untestable here.
4. **OS HiDPI/scaling, IME, a11y bridges, WM behaviour.** Fractional scaling (Wayland supports
   integer scales only today, per DAMAGE_REGION_PLAN), IME preedit, AccessKit tree pushes, and
   WM-driven expose/maximize/restore are all outside the headless surface.

**Practical consequence:** Phase 5's native-vs-headless screenshot comparison is what covers (1)
and partially (3)/(4). It is a *consistency check between two renderers*, not an oracle for
either. Do not let it gate CI.

---

# PART 2 — THE PLAN

## A. Determinism contract

Thousands of tests are worthless if they flake. Pin all of the following. **Most of it is
already solved in the Rust harness and simply needs to be lifted into the JSON runner's host.**

| # | Must be pinned | Status | Where |
|---|---|---|---|
| A1 | **CPU renderer, no GPU** | **EXISTS** | `AZ_BACKEND=headless` (`common/compositor.rs:107`) → `run_headless` (`run.rs:186`) → `CpuBackend`. Already CI's config (`rust.yml:1944`). |
| A2 | **Embedded test font, NO system font fallback** | **EXISTS in Rust tests; MUST be added to the JSON host** | `headless/mod.rs:1894-1932`: `HARNESS_FONT = include_bytes!("doc/fonts/InstrumentSerif-Regular.ttf")` + `FcFontCache::default()` (empty → **zero disk access, no system scan**) + `font_manager.insert_font(...)`. |
| A3 | **Fixed window size + DPI** | **EXISTS** | `setup.window_width/height/dpi` (`full.rs:3237-3247`, defaults 800×600 @96). Make them **mandatory** in generated tests, not defaulted. |
| A4 | **Frozen clock / no wall-clock animation** | **DOES NOT EXIST — must be added** | Scrollbar fade (`layout/src/window.rs:2968-2980`), scroll momentum (`headless/mod.rs:1548`), cursor blink and `SleepMs` all key off real time. Needs an `AZ_TEST_CLOCK` virtual clock + a `tick_ms` op that advances it deterministically. **Without this, idle-stability tests (Tier 1a) are flaky by construction** — a fading scrollbar legitimately damages every frame. |
| A5 | **Seeded RNG** | audit needed | grep for `rand`/`SystemTime` in the layout+manager path; pin or stub. |
| A6 | **Locale / theme / OS CSS at-rules** | partially exists | `GetComponentPreview` already has `override_os` / `override_theme` / `override_lang` (`full.rs:2049-2057`). The same three overrides must exist in `setup`, or `@os()`/`@theme()` CSS makes tests machine-dependent. |
| A7 | **No `wait { ms }` in generated tests** | policy | `wait` is a real `std::thread::sleep` (`full.rs:6686`). It is the #1 flake source. Generated tests must use `wait_frame` / `tick` only. `hello_world_counter.json` uses `wait: 300` three times — that is a legacy pattern, do not copy it. |

**A2 is the sharp edge and is worth reading `headless/mod.rs:1897-1921` in full.** The naive
approach (register the font in `FcFontCache`) **does not work**: generic families ("serif",
azul's default) are expanded to a hardcoded OS list and the generic is dropped, and the
Unicode-fallback path skips codepoints < U+0400. The *only* thing that works is injecting the
parsed font directly into `FontManager` and relying on the shaper's last-resort glyph probe over
loaded fonts. This is a known rust-fontconfig footgun (it also breaks web/wasm fallback). **Copy
the existing solution; do not re-derive it.**

## B. The three tiers of assertion

This is the central design decision.

### TIER 1 — DETERMINISTIC INVARIANTS (the CI gate)

No oracle. True even if the engine is buggy. Free, fast, reproducible. **Safe for cheap agents
to generate, because the agent never has to know the right answer.** This is the backbone.

Each is already prototyped as a Rust test (§1.3.1) — the work is exposing it to JSON.

---
**(a) IDLE STABILITY — the infinite-redraw detector**

> With no input, frame N+1 must be byte-identical to frame N, **and** the damage set must reach
> `FrameDamage::None` within K ticks and stay there.

A frame that keeps changing, or a damage set that never drains, **is** the infinite-redraw bug.
This is the single highest-value assertion in the plan and it is *strictly better than any LLM
or human at finding this class*.

```json
{ "op": "assert_idle_stable", "ticks": 5 }
```
Semantics: tick K times with no input; assert (i) `frame_report.paint_damage == None` on the last
≥K-1 ticks, (ii) `last_frame` pixels identical across them, (iii) `relayout_iterations == 0`.

*Precedent:* `damage_idle_scrollbar_window_skips` (`headless/mod.rs:2951`) — a real, previously
shipped bug where an idle scrollbar'd window re-rendered and re-presented **forever**.
*Depends on:* `FrameReport` (§1.3) **and A4 (frozen clock)** — an animating scrollbar fade
legitimately damages every frame.

---
**(b) LIVENESS — the stale-screen detector**

> After a state change that must alter pixels, the damage set must be **non-empty** and the
> pixels must **actually differ**.

This asserts the "screen didn't update" class directly.

```json
{ "op": "assert_changed", "min_damage_rects": 1 }
```
Semantics: snapshot `last_frame` + `frame_report` before the preceding step; after it, assert
`paint_damage != None` **and** `pixel_diff(before, after).diff_count > 0`.

The two must be checked **together**. Damage-without-pixel-change is a false-positive (wasted
work). Pixel-change-without-damage is impossible-by-construction in the CPU path but *is* the
GPU bug class. **Damage-non-empty + pixels-identical is the interesting failure** — it means the
engine thinks something changed and repainted it to the same value (the `damage_text_change_repro`
signature, `headless/mod.rs:2051`, where glyph count stayed `[3]` for `AAA`→`BBBBBBBB`).

---
**(c) DAMAGE SOUNDNESS — both directions. SELF-VALIDATING.**

> Render the same frame twice: once **full-repaint**, once **damage-driven**. Assert:
> 1. **COVERAGE (⊇):** every pixel that differs between frame N-1 and the full render of frame N
>    lies inside the union of the repaint patches. *Under-paint ⇒ stale screen.*
> 2. **PIXEL IDENTITY:** the damage-driven buffer == the full-repaint buffer, exactly.
> 3. **TIGHTNESS (≤):** `area(paint_damage) <= tightness_factor * area(changed_pixels_bbox)`,
>    and `paint_damage != Full` unless the case is declared structural. *Over-paint ⇒ the perf
>    bug that hides behind "looks fine".*

**This needs no expected values at all.** The oracle is the engine's own full-repaint path,
which is a *different code path* (`compositor.allocate_layers_from_display_list` +
`render_layers` + `composite_frame`, `headless/mod.rs:779-791`) from the incremental one
(`render_display_list_damaged`, `raster.rs:1049`). Diffing two independent implementations of the
same function is the strongest free assertion available.

```json
{ "op": "assert_damage_sound", "max_overpaint_ratio": 4.0 }
```

*Precedent:* `png_scroll_vertical_fast_matches_full_render` (`headless/mod.rs:3450`) already does
exactly (2) for the scroll fast-path. `damage_disjoint_rects_do_not_erase_content_between`
(`:2702`) is (1). `damage_single_paint_in_large_grid_is_local` (`:2528`) is (3).

*Free full-repaint oracle:* `CallbackInfo::take_screenshot` (`callbacks.rs:3008`) **already
re-renders from scratch with a fresh `GlyphCache`** (§1.5). It is the full-repaint side of the
comparison, at zero implementation cost. `cpu_backend.last_frame` is the damage-driven side.

---
**(d) BOUNDED WORK PER EVENT — the loop trips a counter, not a hang**

```json
{ "op": "assert_work_bounded", "max_relayouts": 2, "max_dom_regens": 1 }
```
Asserts `frame_report.relayout_iterations <= max_relayouts`,
`dom_regenerations <= max_dom_regens`, and **`hit_depth_cap == false`**.

Today an invalidation loop hits `MAX_EVENT_RECURSION_DEPTH = 7` and is **silently swallowed with
a `log_warn`** (`event.rs:4003-4010`). This turns it into a red test. Cheap, mechanical, and it
converts a class of hangs into a class of failures.

---
**(e) RESOURCE LEAK — catches "font never removed"**

```json
{ "op": "snapshot_resources", "as": "baseline" },
{ "op": "assert_resource_counts", "vs": "baseline",
  "fonts": "eq", "images": "eq", "parsed_fonts": "eq" }
```
Reads the `pub` counters via `CallbackInfo::get_layout_window()` (§1.4). Shape the test as:
*baseline → mount a DOM using font F / image I → unmount it → force N GC frames → assert counts
returned to baseline.*

**This will go red immediately for fonts, and that is the point** — it is a real bug (§1.4), the
repo admits it (`resources.rs:1435`), and the images path is a working control group proving the
assertion is sound.

Modes: `"eq"` (returns to baseline), `"le"` (bounded), `"monotonic_over"` (N frames — the
unbounded-growth check).

---
**(f) NO PANIC / NO ABORT / NO UNBOUNDED GROWTH**

Free, and mostly already there. Process exit code ≠ 0, or any panic in the log, is a failure.
Add `{ "op": "assert_no_growth", "frames": 200, "counters": ["*"] }` — run N frames and assert
every counter in `debug_counts()` + `memory_report()` has zero slope. This is the generalisation
of `AZ_E2E_TEST`'s RSS probe (`e2e_test.rs:255-284`) and it catches indexing/leak bugs in the
managers.

---
**(g) MANAGER STRUCTURAL INVARIANTS — the "~50 bugs" area**

The 22 managers (`layout/src/managers/`) key per-node state by `NodeId` / `(DomId, NodeId)`.
Keys are remapped only through `transfer_states` (`core/src/diff.rs:860`) + `create_migration_map`
(`:830`) on DOM rebuild. **`reconcile_dom` produces `node_moves` but no unmount info that anyone
consumes for manager GC** — so entries for unmounted nodes are the classic leak.

```json
{ "op": "assert_manager_invariants", "managers": ["scroll", "hover", "focus", "gesture"] }
```
Asserts, for each: **(i) no key refers to a node that no longer exists in the StyledDom**
(the staleness invariant — this is the direct expression of the manager bug class);
(ii) counts bounded; (iii) no duplicate/overlapping sessions.

**Prerequisite:** `debug_counts()` exists on only 4 of 22 (`virtual_view.rs:95`, `hover.rs:69`,
`scroll_state.rs:460`, `gesture.rs:538`). **Add it to `focus_cursor`, `text_edit`, `gpu_state`,
`undo_redo`, `a11y`, `drag_drop`** — all hold per-node state, none are observable. Extend the
trait to also expose *the key set*, not just the count, so (i) is checkable.

---

### TIER 2 — LLM SANITY CHECK ("vibe test") — ADVISORY, **NEVER A CI GATE**

A judge model sees the headless screenshot + the one-line case description and returns
`looks-right | looks-wrong | suspicious` + a reason.

**It is good at** gross failures no invariant expresses: blank screen, garbled/overlapping text,
element off-screen, content obviously not matching the description, z-order nonsense.
**It is bad at** pixel precision. **Do not ask it for numbers.** Never ask "is this box 100px
wide" — ask "does this look like a broken render?"

**HARD CONSTRAINTS:**
- **Not a blocking gate.** It is nondeterministic, costs tokens, and rate-limits. CI runs
  **Tier 1 only** — deterministic, fast, free.
- The LLM sweep runs **offline / nightly** and emits a triage report, never a pass/fail verdict
  that anything depends on.
- **Rate-limit hazard applies here too:** a rate-limited reply comes back as **plain text with
  exit 0**. It must be detected and counted as a **FAILURE**, never written out as a verdict.
  (Same guard as `autotest_fleet.sh:164-170`.) A judge that silently returns "looks fine"
  because it was throttled is worse than no judge.

**The promotion loop — this is the whole point of Tier 2.** A vibe test that stays a vibe test is
just flaky noise. It compounds into a real suite only if flags get promoted:

```
  nightly LLM sweep over headless PNGs
        │
        ▼
   triage report:  N flags, ranked by confidence
        │
        ▼
   HUMAN (or a strong agent) confirms ONE flag  ─── not confirmed ──▶ drop, record as a
        │                                                              known-good exemplar so
        │ confirmed = a real bug                                       the judge stops re-flagging
        ▼
   PROMOTE: write a deterministic Tier-1 test that fails on it
        │   (usually assert_idle_stable / assert_changed / assert_damage_sound
        │    — the bug almost always reduces to one of these)
        ▼
   goes into the CI gate, guards it forever, LLM never looks at it again
```

Tier 2's output is **candidate Tier-1 tests**, not verdicts. Budget it as such.

### TIER 3 — FORBIDDEN

> **A generator MUST NEVER run azul and record its numeric/pixel output as the expected value.**

That enshrines current bugs as "expected" and yields thousands of change-detector tests that pass
forever and catch nothing. **This is not hypothetical — the harness does it today:**

`eval_assert_screenshot` (`full.rs:3877-3889`): if the reference PNG is missing, it **writes the
actual output as the reference and returns `pass`**. `tests/e2e/contenteditable_overflow_test.json`
points at a directory that **does not exist** → 12 assertions auto-baseline to green, forever.

**Actions:**
1. **Remove the auto-baseline.** Missing reference ⇒ **FAIL**, not pass. Gate baselining behind
   an explicit `BLESS=1`.
2. If a `--record` mode is ever added, recorded expectations are written as
   `"provisional": true` and **excluded from the pass/fail gate** until a human reviews them.
3. Generated tests may use `assert_layout` / `assert_text` **only** when the expected value comes
   from the case description itself (e.g. "a 100px-wide div" → `width == 100`), never from a run.

Note the asymmetry that makes this workable: **Tier 1 needs no expected values at all.** That is
precisely why it is the backbone and why cheap agents can safely generate it.

## C. Test file format

Keep the **existing** `E2eTest`/`E2eStep` schema (`full.rs:3218/3263`) — it already works, it
already runs in CI, and every debug op is already a valid step. Add: one `mount` op, one `setup`
block extension, and six assertion ops.

### C.1 The `mount` op — the one enabling primitive

Without this, thousands of *independent* tests are impossible (§1.1). Two options:

**Option A (recommended) — a dedicated test host + the *existing* `setup.app_state`.**
Build `examples/azul-e2e-host` whose app state is `{ "xml": "...", "css": "..." }` and whose
`layout_callback` calls `parse_xml_to_styled_dom` (`layout/src/xml/mod.rs:196`). Then
**`setup.app_state` (`full.rs:3246`) and the `set_app_state` op (`full.rs:8621`) mount arbitrary
DOM+CSS with ZERO engine changes** — the plumbing already exists (it needs a `RefAny` with a
`deserialize_fn`, `full.rs:8625`, which an azul-owned Rust host trivially provides).
This also fixes determinism A2 for free: the host injects `HARNESS_FONT` at startup.

**Option B — a new `mount` op** (`DebugEvent::Mount { xml, css }`) that swaps the root StyledDom
directly. More invasive, touches the shell, but works against *any* app binary.

**Option A is strictly better**: no engine change, no new op, reuses a tested path, and gives one
binary to compile once and run thousands of JSON files against.

### C.2 Full schema

```jsonc
{
  "name": "string",                       // required, unique — used for dedupe + shard keys
  "description": "string",                // the one-line case; the LLM judge sees this
  "tier": 1,                              // 1 = CI gate, 2 = advisory-only
  "setup": {
    "window_width": 800, "window_height": 600, "dpi": 96,   // A3 — MANDATORY in generated tests
    "app_state": { "xml": "<div class='a'>hi</div>", "css": ".a { width: 100px; }" },  // C.1
    "theme": "light", "os": "linux", "lang": "en",          // A6 — NEW, mirrors full.rs:2049-2057
    "freeze_clock": true                                    // A4 — NEW
  },
  "steps": [
    // ACTIONS — all of these EXIST today (full.rs:1526-2073)
    { "op": "wait_frame" },
    { "op": "tick_ms", "ms": 16 },                          // NEW (A4) — advances the virtual clock
    { "op": "resize", "width": 400, "height": 300 },
    { "op": "click", "selector": ".btn" },                  // or x/y, or node_id, or text
    { "op": "mouse_move", "x": 10, "y": 20 },
    { "op": "key_down", "key": "a" },
    { "op": "text_input", "text": "hello" },
    { "op": "scroll", "x": 50, "y": 50, "delta_x": 0, "delta_y": -100 },
    { "op": "set_node_css_override", "node_id": 3, "property": "width", "value": "200px" },
    { "op": "insert_node", "parent_id": 1, "node_type": "div", "classes": ["x"] },
    { "op": "delete_node", "node_id": 3 },
    { "op": "set_app_state", "state": { "xml": "...", "css": "..." } },   // = remount

    // ASSERTIONS — Tier 1. ALL SIX ARE NEW.
    { "op": "assert_idle_stable",     "ticks": 5 },                            // B.1a
    { "op": "assert_changed",         "min_damage_rects": 1 },                 // B.1b
    { "op": "assert_damage_sound",    "max_overpaint_ratio": 4.0 },            // B.1c
    { "op": "assert_work_bounded",    "max_relayouts": 2, "max_dom_regens": 1 },// B.1d
    { "op": "snapshot_resources",     "as": "baseline" },                      // B.1e
    { "op": "assert_resource_counts", "vs": "baseline", "fonts": "eq" },       // B.1e
    { "op": "assert_no_growth",       "frames": 200 },                         // B.1f
    { "op": "assert_manager_invariants", "managers": ["scroll","hover","focus"] }, // B.1g

    // ASSERTIONS — existing, safe only when the expected value comes from the DESCRIPTION
    { "op": "assert_layout", "selector": ".a", "property": "width", "expected": 100 },
    { "op": "assert_exists", "selector": ".a" },

    // TIER 2 only — never in a tier-1 file
    { "op": "capture", "as": "after_click" }                 // NEW — writes a PNG for the judge
  ]
}
```

### C.3 Worked example 1 — the font leak (Tier 1e). **This should go RED.**

```json
{
  "name": "leak_font_released_after_node_removed",
  "description": "A text node using a distinct font family is added then removed; the font tables must return to baseline.",
  "tier": 1,
  "setup": {
    "window_width": 400, "window_height": 300, "dpi": 96, "freeze_clock": true,
    "app_state": { "xml": "<body></body>", "css": "" }
  },
  "steps": [
    { "op": "wait_frame" },
    { "op": "snapshot_resources", "as": "baseline" },

    { "op": "set_app_state", "state": {
        "xml": "<body><p class='t'>hello</p></body>",
        "css": ".t { font-family: TestFontB; font-size: 16px; }" } },
    { "op": "wait_frame" },

    { "op": "set_app_state", "state": { "xml": "<body></body>", "css": "" } },
    { "op": "wait_frame" },
    { "op": "tick_ms", "ms": 16 }, { "op": "tick_ms", "ms": 16 }, { "op": "tick_ms", "ms": 16 },

    { "op": "assert_resource_counts", "vs": "baseline",
      "images": "eq",
      "fonts": "eq",
      "parsed_fonts": "eq" }
  ]
}
```
`images: eq` is the **control** — the image GC works (`wr_translate2.rs:1917-1919`), so it must
pass. `fonts: eq` / `parsed_fonts: eq` **will fail**, and that failure is the bug in §1.4. Three
`tick_ms` steps clear the `IMAGE_GC_KEEP_EPOCHS = 2` grace window (`wr_translate2.rs:1876`) so
the control is fair.

### C.4 Worked example 2 — damage soundness (Tier 1c). Self-validating, no expected values.

```json
{
  "name": "damage_sound_recolor_one_box_in_grid",
  "description": "Recoloring one box in a 10x10 grid must repaint only that box: full and damage-driven renders must be pixel-identical, and the paint region must not balloon to the whole window.",
  "tier": 1,
  "setup": {
    "window_width": 400, "window_height": 300, "dpi": 96, "freeze_clock": true,
    "app_state": {
      "xml": "<body><div id='g'><div class='c' id='c42'></div></div></body>",
      "css": "#g { display:flex; flex-wrap:wrap; } .c { width:40px; height:30px; background:blue; }"
    }
  },
  "steps": [
    { "op": "wait_frame" },
    { "op": "assert_idle_stable", "ticks": 3 },

    { "op": "set_node_css_override", "node_id": 2, "property": "background-color", "value": "red" },
    { "op": "wait_frame" },

    { "op": "assert_changed", "min_damage_rects": 1 },
    { "op": "assert_damage_sound", "max_overpaint_ratio": 4.0 },
    { "op": "assert_work_bounded", "max_relayouts": 1, "max_dom_regens": 0 },

    { "op": "assert_idle_stable", "ticks": 3 }
  ]
}
```
Note the shape: **idle-stable → change → changed+sound+bounded → idle-stable again.** That
trailing `assert_idle_stable` is what catches "the change kicked the engine into a permanent
redraw loop". This is the canonical template and most generated cases are a variation of it.

### C.5 Worked example 3 — idle stability under a hover (Tier 1a + 1g).

```json
{
  "name": "idle_stable_after_hover_leaves",
  "description": "Hovering a button then moving away must settle: no residual damage, and the hover manager must not retain state for the un-hovered node.",
  "tier": 1,
  "setup": {
    "window_width": 300, "window_height": 200, "dpi": 96, "freeze_clock": true,
    "app_state": {
      "xml": "<body><button class='b'>ok</button></body>",
      "css": ".b { width:80px; height:24px; background:#ccc; } .b:hover { background:#eee; }"
    }
  },
  "steps": [
    { "op": "wait_frame" },
    { "op": "mouse_move", "x": 20, "y": 10 },
    { "op": "wait_frame" },
    { "op": "assert_changed", "min_damage_rects": 1 },

    { "op": "mouse_move", "x": 250, "y": 180 },
    { "op": "wait_frame" },
    { "op": "assert_changed", "min_damage_rects": 1 },

    { "op": "assert_idle_stable", "ticks": 5 },
    { "op": "assert_manager_invariants", "managers": ["hover"] },
    { "op": "assert_work_bounded", "max_relayouts": 1, "max_dom_regens": 0 }
  ]
}
```

**Note how small each file is.** A cheap model can emit one of these from a single one-line
description **without ever reasoning about expected geometry** — which is exactly the property
§B was designed to give it.

## D. The generation pipeline

Mirrors `scripts/autotest_fleet.sh` conventions. **Read that script before writing this one** —
its lessons were expensive.

### Stage 1 — ONE strong agent writes the case list

```
scripts/e2e/cases/<subsystem>.txt     # one one-line case per line
```
Subsystems: `damage`, `idle`, `managers-scroll`, `managers-hover`, `managers-focus`,
`managers-gesture`, `managers-text-edit`, `resources`, `css-cascade`, `layout-flex`,
`layout-text`, `virtual-view`.

Fable/Opus at high effort **reads the subsystem source** (e.g. `layout/src/managers/scroll_state.rs`)
and writes N one-line cases. Bias the prompt hard toward the redraw/staleness/loop classes:

> *"For `layout/src/managers/hover.rs`: write 120 one-line e2e case descriptions. Each must be a
> scenario where a REDRAW bug would show: a state change that must repaint, a settle that must
> reach zero damage, a node removal that must drop manager state, a rapid sequence that must not
> loop. One per line. No expected numbers — describe the SCENARIO, not the answer."*

The "no expected numbers" instruction is load-bearing: it keeps Stage 2 in Tier-1 territory.

### Stage 2 — MANY cheap agents fan out, one line → one `.json`

`haiku`/`sonnet`, `--jobs 12`. Each agent gets: **the schema (§C.2), two worked examples
(§C.3/§C.4), and one line.** It must NOT need to reason about expected geometry.

```
scripts/e2e/tests/<subsystem>/<nnn>_<slug>.json
```

### Stage 3 — the mechanical gate

Three checks, in order. **Anything that fails 1 or 2 is deleted and regenerated. Failing 3 is
FINE — it may be a real bug.**

1. **Schema validation** — parses as `E2eTest`, every `op` is in the known set, every required
   param present. (Reject unknown ops: `evaluate_assertion` silently returns
   `fail("Unknown assertion: …")` (`full.rs:3382`), so a typo'd op looks like a *bug*, not a
   malformed test. Catch it here.)
2. **Executes** — runs to completion, no crash, no timeout, no "Unknown assertion".
3. **Assertions may be red.** A red Tier-1 assertion is a *finding*, not a broken test. Triage
   separately; do not delete.

### The hard-won lessons — all four are mandatory

| Lesson | Source | Rule |
|---|---|---|
| **A rate-limited reply is exit 0 + plain text** | `autotest_fleet.sh:161-170`, `:206-213` | **The single most important one.** A throttled agent returns success with a limit message as its body. If you trust exit codes you will write the limit message into a `.json`, mark the case done, and never revisit it. **Verify the ARTIFACT, not the exit code:** the file must exist, parse as JSON, and contain `"steps"`. Otherwise: delete, count as FAIL, do **not** add to the done-list. |
| **Resume via a done-list** | `autotest_fleet.sh:91-109` | `target/e2e-gen/.done-<subsystem>`, appended only on verified success. Two independent signals: the done-list **and** the artifact itself, so a lost done-list still resumes. `--redo` to force. |
| **`--dry-run`** | `autotest_fleet.sh:59`, `:203` | Print the work list, launch nothing. Non-negotiable before spending a fleet. |
| **Never let an agent run cargo** | `autotest_fleet.sh:199-201` | Agents write files only. One global verify pass at the end (Stage 3). N agents each running a build will lock the machine. |

### Dedupe — the user explicitly does not want overlapping tests

Cheap agents will produce near-duplicates. Three layers:

1. **Stage 1 owns non-overlap.** The strong agent sees the whole subsystem at once and is told
   *"no two cases may differ only in a constant."* This is where dedupe is cheapest.
2. **Structural hash at Stage 3.** Canonicalize each JSON (drop `name`/`description`, sort keys,
   normalize numeric literals to a bucket) → SHA. Identical hash = duplicate, delete.
3. **Coverage-gap overlay (the `--fable` pattern, `autotest_fleet.sh:112-137`).** Build coverage
   over `layout/src/managers/` + `layout/src/cpurender/` with the e2e suite running, keep only
   files with uncovered lines, hand the next Stage-1 agent **the exact uncovered line numbers**
   and tell it to write cases reaching *only* those. This is the same mechanism that already
   works for unit tests and it is the only one that actually converges.

### Sharding / runtime

The dominant cost is **process startup + font/layout warmup**, not the steps (a step is ~ms;
`hello_world_counter.json`'s 9 steps run in ~400 ms *including* three 300 ms sleeps).

- **One process, many files.** `AZ_E2E` already accepts an array of tests in one file
  (`run.rs:82-92`) and the runner loops them (`full.rs`, `RunE2eTests { tests: Vec<E2eTest> }`,
  `:1872`). **Concatenate a shard's JSONs into one array → one process → one warmup.** This is
  the single biggest runtime lever and it needs no code change.
- **Caveat, and it is a real one:** `doc/guide/en/e2e-testing.md:203` — *"A test runs against the
  current application instance — there is no per-test sandbox."* Tests in one process share
  window state, manager state, and resource tables. **For leak tests (1e) that is fatal.**
  ⇒ **Two lanes:** *stateless* tests (damage, idle, layout — batch 200/process) and *stateful*
  tests (resource leaks, no-growth — one process each). Enforce the lane in the schema
  (`"isolated": true`).
- **Parallel shards** in CI: matrix over subsystems, `-P $(nproc)` locally. A 2000-test suite at
  ~10 tests/s in batched mode is ~3 min wall-clock across 8 shards. That fits the budget.
- **Do not** add these to the existing `e2e_native` job (`rust.yml:1917`) — that job is the
  *language-binding* matrix (3 OSes × 3 families × 27 toolchains, already at a 60 min timeout).
  New job, Linux-only, `--features build-dll,debug-server`, reusing the `build_dll_e2e` artifact
  (`rust.yml:1855`) so there is no extra DLL build.

## E. Phasing

**Phase 0 — Unblock (no new tests).** *The whole plan is gated on this.*
- `FrameReport` on `LayoutWindow`: move `FrameDamage` (`headless/mod.rs:159`) into `azul-core`;
  write paint+present damage + iteration counters after `render_frame` at all 7 call sites.
- `relayout_iterations` / `dom_regenerations` / `hit_depth_cap` counters in
  `process_window_events` (`event.rs:4001`).
- `pub` on `damage_area()` (`headless/mod.rs:1969`).
- **Kill the `assert_screenshot` auto-baseline** (`full.rs:3877-3889`) → missing ref = FAIL.
- `debug_counts()` (+ key-set accessor) on the 6 unobserved managers.
- Virtual clock (`AZ_TEST_CLOCK` + `tick_ms`) — **A4; without it Tier 1a is flaky.**

**Phase 1 — Assertions + host.** The six Tier-1 ops (§C.2). `examples/azul-e2e-host`
(XML+CSS via `setup.app_state`, embedded font). Hand-write ~30 tests covering the §1.3.1 Rust
tests to prove the JSON layer reproduces them. **Exit criterion: the font-leak test (§C.3) goes
red and the image control goes green.**

**Phase 2 — Generation.** `scripts/e2e_fleet.sh` (Stages 1-3). Target ~2000 tests. Triage the
red ones — this is where the manager bugs surface.

**Phase 3 — CI.** New Linux job, batched shards, Tier 1 only, blocking.

**Phase 4 — Coverage-gap loop.** `--fable` overlay over `managers/` + `cpurender/`, iterate.

**Phase 5 — LLM sweep (Tier 2) + native-vs-headless.** Nightly, advisory, promotion loop (§B.2).
Native screenshots need a **real window** (headless returns `RawWindowHandle::Unsupported`,
`headless/mod.rs:1717`) and are **X11/macOS/Windows only — Wayland is unsupported**
(`native_screenshot.rs:88`; only `scripts/azshot.py` can do it, out-of-process).
**This is a consistency check between two renderers, not an oracle.** It never gates CI.

## F. Risks

| # | Risk | Mitigation |
|---|---|---|
| **F1** | **Bug-enshrinement.** Tier 3. **Already happening** (`full.rs:3877` auto-baseline; `contenteditable_overflow_test.json`'s 12 phantom-green asserts). | Kill auto-baseline in Phase 0. Tier 1 needs **no** expected values — that is the structural defence. `provisional: true` for anything recorded. |
| **F2** | **Nondeterminism from system fonts.** A system-font scan makes every runner different. | A2: embedded `HARNESS_FONT` + empty `FcFontCache`. **The solution already exists** (`headless/mod.rs:1894-1932`) — copy it, and read `:1897-1921` for why the obvious approach fails. |
| **F3** | **Wall-clock nondeterminism.** Scrollbar fade / momentum / blink damage the frame on a timer. **Tier 1a is flaky without a frozen clock.** | A4 virtual clock. **This is the highest-risk unbuilt item**; do it in Phase 0 or idle-stability tests will be quarantined within a week. |
| **F4** | **Cheap agents produce near-duplicates.** | Three-layer dedupe (§D): strong agent owns non-overlap → structural hash → coverage-gap overlay. |
| **F5** | **Rate-limited agent writes garbage and marks it done.** Exit 0 + plain text. | Verify the artifact, never the exit code (`autotest_fleet.sh:161-170`). Applies to the **Tier-2 judge** too. |
| **F6** | **CI cost.** 2000 tests × process startup. | Batch into one process per shard (`RunE2eTests` takes a `Vec`). Separate lane for `isolated: true` leak tests. Reuse the `build_dll_e2e` artifact. Tier 2 never runs in CI. |
| **F7** | **Shared-process state bleed** (`e2e-testing.md:203`: "no per-test sandbox"). Silently corrupts leak tests. | `"isolated": true` lane, one process each. |
| **F8** | **Testing the DL diff, not the invalidation logic.** Damage is derived post-hoc by re-diffing display lists; `compute_damage(dirty_nodes,…)` is dead (§1.3.3). | Accept and name it. It is what ships today. If DAMAGE_REGION_PLAN §4 lands, the Tier-1 assertions are **unchanged** — they assert *outcomes*, not mechanism. That is a feature of this design. |
| **F9** | **GPU/WebRender damage is a different system** (`DeviceIntRect`, buffer-age) and is entirely untested by this plan. | Explicit residual risk (§1.6). Only Phase 5 touches it, and only as a consistency check. Do not claim GPU coverage. |
| **F10** | **The red-test flood.** Phase 2 will produce hundreds of red Tier-1 tests, most of them the *same* bug. | Triage by **failure signature**, not by test. Cluster on `(op, manager, damage_kind)`. Expect the ~50 manager bugs to collapse into far fewer root causes — as `#11`/`#12` did in DAMAGE_REGION_PLAN §0.5. |

---

## Decisions needed before Phase 0

1. **`FrameReport` on `LayoutWindow` vs. widening the assertion signature.** This plan proposes
   moving `FrameDamage` into `azul-core` and hanging a `FrameReport` off `LayoutWindow` (§1.3),
   because `CallbackInfo::get_layout_window()` already exists and this makes damage assertable on
   **every platform**, not just headless. The alternative (thread `&CpuBackend` into
   `evaluate_assertion`) is headless-only and uglier. **Confirm the `azul-core` move** — it is
   the direction `DAMAGE_REGION_PLAN` already points (`headless/mod.rs:150` calls `FrameDamage`
   "the seed of the unified `DamageRegion` type").

2. **`examples/azul-e2e-host` (Option A) vs. a `mount` op (Option B).** A is zero engine change
   and reuses `setup.app_state`; B works against any app binary. **Recommend A.** Confirm.

3. **The virtual clock (A4) is in Phase 0, not deferred.** It is the least glamorous item and the
   one most likely to get cut — and if it is cut, `assert_idle_stable` (the infinite-redraw
   detector, i.e. the highest-value assertion in the plan) is flaky and gets quarantined.
   **Confirm it stays in Phase 0.**

---

```
  "we already have azul-doc reftest for the actual tests against Chrome, so testing the layout should explicitly be
  EXCLUDED from this - its not about whether the layout is correct (chrome reftest tests that), but whether the
  BEHAVIOUR (i.e.: if a user clicks and drags, then multi-node-selection works and the node starts to scroll, and
  this is animated and repainted incrementally) all works TOGETHER"

  Consequences — restructure the plan accordingly:

  1. EXPLICITLY OUT OF SCOPE: layout correctness, box geometry, CSS conformance, pixel-perfect rendering. `azul-doc
  reftest` (Chrome) already owns that; say so in the doc and do not duplicate it. NO geometry assertions in the
  e2e JSON schema. If you find yourself proposing "assert node X is at (10,20,100,40)", you have gone wrong.

  2. IN SCOPE: BEHAVIOUR — the managers composing correctly OVER TIME. A test is therefore a SCRIPTED INTERACTION
  TIMELINE (a sequence of synthetic input events + time ticks + DOM/state mutations), not a static DOM snapshot.
  Design the JSON schema as a timeline. The canonical example the user gave, and which the schema must express
  cleanly: "user clicks and drags -> multi-node selection works -> the node starts to scroll -> this is animated ->
  and is repainted incrementally".

  3. THE HIGH-VALUE ASSERTION FAMILIES (bias the whole plan here; this is the ~50-bug `layout/src/managers/` area):
     a) COMPOSITION: drag -> selection -> autoscroll -> animation -> incremental repaint all fire, in the right
  order, and reach a fixpoint.
     b) CROSS-MANAGER CONSISTENCY: e.g. scroll_state vs scroll_into_view agree; selection anchor stays valid;
  focus/hit-test/drag agree about the same node. Enumerate the real managers by reading layout/src/managers/ and
  derive the pairwise consistency invariants that must hold.
     c) STATE-MACHINE LEAKS: drag ended but the drag manager is still active; animation finished but is still
  scheduling frames (a direct infinite-redraw source); selection cleared but listeners still armed.
     d) DANGLING INDICES UNDER MUTATION: remove/replace the node that is currently being dragged / selected /
  scrolled / animated, MID-INTERACTION, and assert no panic, no stale index, manager state consistent. This family
  is mechanically generatable and is exactly the "indexing issues" the user cares about — make it a first-class
  generator template.
     e) INCREMENTALITY: the repaint is a PATCH, not a full redraw (the user explicitly says "repainted
  incrementally"), plus the damage soundness/liveness/idle-stability invariants from my previous message.

  4. DETERMINISM GETS MUCH CHEAPER — exploit this and simplify the determinism contract: nearly every Tier-1
  invariant compares azul AGAINST ITSELF WITHIN ONE PROCESS (frame N vs N+1; full-repaint vs damage-driven render;
  manager state before vs after; two identical runs). Self-comparison is immune to font/DPI/runner variance, so
  those tests need NO pinned fonts and run anywhere. ONLY the Tier-2 LLM screenshot pass needs a stable rendering,
  and it is advisory. Do not over-engineer a determinism contract for assertions that do not need it — but DO state
  which few things (the LLM screenshot pass) still want a pinned font.

  5. The LLM's job (Tier 2) is now: (i) GENERATE the interaction scenarios as one-liners ("drag from node A across
  B and C while the list autoscrolls, then delete B mid-drag"), and (ii) VIBE-CHECK the resulting
  screenshot/sequence ("does this look like a drag-selection that scrolled?"). It is NOT asked about geometry or
  numbers. Still advisory, still never a CI gate.

  Rewrite the worked JSON examples to be interaction timelines demonstrating (a)-(e), not layout snapshots.
```

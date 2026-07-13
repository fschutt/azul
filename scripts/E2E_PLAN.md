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

## 0.1 Scope — what this is NOT

**Layout correctness, box geometry, CSS conformance and pixel-perfect rendering are EXPLICITLY
OUT OF SCOPE.** `azul-doc reftest` (Chrome, `doc/src/reftest/pipeline.rs`, 9,629 `.xht` files in
`doc/xhtml1/`, §1.5) already owns that question and answers it against a real browser. This plan
does not duplicate it, does not compete with it, and must never be graded on it.

> **The rule, stated so it cannot be misread:** *no geometry assertions in the e2e JSON schema.*
> If a generated test says **"assert node X is at (10,20,100,40)"**, the generator has gone
> wrong. Delete it. "Is the box the right size?" is reftest's job.

**What IS in scope: BEHAVIOUR — the `layout/src/managers/` composing correctly OVER TIME.**
The bug class this plan exists to catch is not "the box is 3px too wide"; it is:

> *the user clicks and drags → multi-node selection works → the node starts to scroll →
> that scroll is animated → and the result is repainted **incrementally**, and all of it works
> **together**, and it **settles**.*

Every one of those five stages is a different manager (§2.B.1g). The bugs live in the seams
between them, in the state each one keeps *after* the interaction ends, and in what happens when
the DOM is rebuilt underneath them mid-interaction.

**Consequence for the format: a test is a SCRIPTED INTERACTION TIMELINE** — a sequence of
synthetic input events, time ticks and DOM/state mutations, with invariants asserted *between*
the steps — **not a static DOM snapshot.** §2.C designs the schema as a timeline for exactly this
reason.

**Consequence for determinism: it gets much cheaper.** Almost every Tier-1 invariant below
compares azul **against itself, inside one process** (frame N vs N+1; the full-repaint render vs
the damage-driven render; manager state before vs after; run 1 vs run 2). Self-comparison is
immune to font/DPI/runner variance. See §2.A.

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

## A. Determinism contract — small, because self-comparison does the work

**Do not over-engineer this.** Because layout correctness is out of scope (§0.1), *nearly every
Tier-1 invariant compares azul against itself within a single process*:

| Invariant | What it compares | Needs a pinned rendering? |
|---|---|---|
| (a) idle stability | frame N vs frame N+1 | **no** |
| (b) liveness | pixels before vs after the same step | **no** |
| (c) damage soundness | damage-driven render vs full-repaint render, same process | **no** |
| (d) bounded work | a counter against a constant | **no** |
| (e) resource leak | counts vs the same process's own baseline | **no** |
| (f) no growth | a counter's slope | **no** |
| (g) manager families | manager state vs the DOM / vs each other / before vs after | **no** |
| Tier 2 LLM vibe-check | a screenshot vs a human-language description | **yes — advisory only** |

**Whatever font the runner happens to have, both sides of every Tier-1 comparison use it.** Font
metrics, DPI, and machine differences cancel. **Tier-1 tests therefore need NO pinned font and
run on any runner.** That deletes the single most expensive item people usually put in a
determinism contract.

What *does* still have to be pinned — and it is a short list:

| # | Must be pinned | Status | Where |
|---|---|---|---|
| A1 | **CPU renderer, no GPU** | **EXISTS** | `AZ_BACKEND=headless` (`common/compositor.rs:107`) → `run_headless` (`run.rs:186`) → `CpuBackend`. Already CI's config (`rust.yml:1944`). |
| A2 | **Frozen clock / no wall-clock animation** | **DOES NOT EXIST — must be added** | The **only genuinely load-bearing determinism item.** Scrollbar fade (`layout/src/window.rs:2968-2980`, gated by `gpu_state_manager.scrollbar_fade_active`, `gpu_state.rs:54`), scroll momentum/animation (`ScrollManager::tick`, `scroll_state.rs:695`), cursor blink (`text_edit.rs:29` `BlinkState`) and `SleepMs` all key off real time. Needs an `AZ_TEST_CLOCK` virtual clock + a `tick_ms` op. **Without it, idle-stability (Tier 1a) and every animation timeline is flaky by construction** — a fading scrollbar legitimately damages every frame, and an animation that must *finish* can never be asserted to have finished. |
| A3 | **Fixed window size + DPI** | **EXISTS** | `setup.window_width/height/dpi` (`full.rs:3237-3247`, defaults 800×600 @96). Mandatory in generated tests — not for pixel-exactness, but so a scroll container is *actually* overflowing on every machine (an interaction timeline that autoscrolls needs the content to not fit). |
| A4 | **No `wait { ms }` in generated tests** | policy | `wait` is a real `std::thread::sleep` (`full.rs:6686`). It is the #1 flake source. Generated tests use `wait_frame` / `tick_ms` only. `hello_world_counter.json` uses `wait: 300` three times — legacy pattern, do not copy it. |
| A5 | **Seeded RNG** | audit needed | grep for `rand`/`SystemTime` in the layout+manager path; pin or stub. Cheap; do it once. |

**Explicitly NOT required for Tier 1** (and cut from earlier drafts of this plan):
- **A pinned/embedded font.** Both sides of every Tier-1 comparison use whatever font is present.
- **Locale / theme / OS at-rule overrides.** They change *how it looks*, which is reftest's
  problem, not ours. (`GetComponentPreview`'s `override_os`/`override_theme`/`override_lang`,
  `full.rs:2049-2057`, exist if a specific case ever wants them — but do not make them a
  contract.)

**The one exception — Tier 2.** The advisory LLM screenshot sweep *does* want a stable rendering,
so a nightly Tier-2 shard should use the embedded harness font. **That solution already exists —
copy it, do not re-derive it:** `headless/mod.rs:1894-1932`, `HARNESS_FONT =
include_bytes!("doc/fonts/InstrumentSerif-Regular.ttf")` + `FcFontCache::default()` (empty ⇒ zero
disk access, no system scan) + `font_manager.insert_font(...)`. Read `:1897-1921` before touching
it: registering the font in `FcFontCache` **does not work** (generic families like "serif" —
azul's default — are expanded to a hardcoded OS list and the generic is dropped; the
Unicode-fallback path skips codepoints < U+0400). The only thing that works is injecting the
parsed font straight into `FontManager`. A known rust-fontconfig footgun. **Tier 1 does not need
any of this.**

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
*Depends on:* `FrameReport` (§1.3) **and A2 (frozen clock)** — an animating scrollbar fade
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
**(g) THE MANAGER FAMILIES — the "~50 bugs" area, and the backbone of this plan**

This is where §0.1's "BEHAVIOUR, not layout" cashes out. **First, the real managers**, read out of
`layout/src/managers/` and `LayoutWindow` (`layout/src/window.rs:370-469`) rather than guessed:

| Manager | Type & def | Per-node state it keys | Remapped on DOM rebuild? |
|---|---|---|---|
| **scroll** | `ScrollManager` `scroll_state.rs:296` | `states: BTreeMap<(DomId,NodeId), AnimatedScrollState>` `:298`; `scrollbar_states: BTreeMap<(DomId,NodeId,Orientation), ScrollbarState>` `:300`; `scroll_dirty: bool` `:317` | **YES** — `remap_node_ids` `scroll_state.rs:1338`, called `common/layout.rs:1042` |
| **scroll animation** | `AnimatedScrollState.animation: Option<ScrollAnimation>` `scroll_state.rs:339`; driven by `ScrollManager::tick(now) -> ScrollTickResult { needs_repaint, updated_nodes }` `:695/:410`; `has_active_animations()` `:722` | the animation *is* scroll state | (with scroll) |
| **scroll_into_view** | **free functions, no struct** — `scroll_node_into_view` `scroll_into_view.rs:177`, `scroll_cursor_into_view` `:209`, `calculate_axis_delta` `:387`; emits `ScrollAdjustment { container, delta, behavior }` `:47`, applied **into `ScrollManager`** (`&mut ScrollManager` param, `:101/:180/:213`) | none of its own — it *writes* scroll's | n/a (stateless) |
| **gesture / drag** | `GestureAndDragManager` `gesture.rs:456` | `input_sessions: Vec<InputSession>` `:459`; `active_drag: Option<DragContext>` `:461`; `long_press_callbacks_invoked: Vec<u64>`; `touch_sessions: BTreeMap<u64,u64>`; `next_session_id` | **YES** — `remap_node_ids` `gesture.rs:1689` → `DragContext::remap_node_ids` `core/src/drag.rs:702`, called `common/layout.rs:1055` |
| **drag context** | `DragContext` `core/src/drag.rs:365`, `ActiveDragType` `:28` = `TextSelection(TextSelectionDrag)` `:48` / `ScrollbarThumb` / `Node(NodeDrag)` `:106` / `WindowMove` / `WindowResize` / `FileDrop` | source node, drop target, current mouse pos | (with gesture) |
| **selection (text/multi-cursor)** | `MultiCursorState` `core/src/selection.rs:257`; `TextSelection { anchor, focus, ... }` `:642`, `SelectionAnchor` `:588` — *"the anchor remains constant during a drag; only the focus moves"* (`:590`) | anchor/focus `(DomId,NodeId,cursor)` pairs | **YES** — `MultiCursorState::remap_node_ids` `core/src/selection.rs:540`, called `common/layout.rs:1046` (drops selections whose node vanished — `selection.rs:1847` test) |
| **text_edit** | `TextEditManager` `text_edit.rs:91` | `multi_cursor: Option<MultiCursorState>` `:93` (owns the selection above); `blink: BlinkState` `:29`; preedit; `display_list_dirty: bool` | via multi_cursor (above) |
| **focus** | `FocusManager` `focus_cursor.rs:53` | `focused_node: Option<DomNodeId>` `:55`; `pending_focus_request` `:57`; `pending_contenteditable_focus` `:64` | **YES** — hand-rolled at `common/layout.rs:1015-1037` (remap or **clear**), + `remap_pending_focus_node_ids` `focus_cursor.rs:175` @ `layout.rs:1058` |
| **hover / hit-test history** | `HoverManager` `hover.rs:53` | `hover_histories: BTreeMap<InputPointId, VecDeque<FullHitTest>>` `:56` — the last N frames of hit-tests, per pointer | **YES** — `remap_node_ids` `hover.rs:241`, called `common/layout.rs:1052`; also `purge_dom` `:152` |
| **gpu_state (incl. scrollbar fade)** | `GpuStateManager` `gpu_state.rs:43` | `caches: BTreeMap<DomId, GpuValueCache>` `:45`; **`scrollbar_fade_active: bool` `:54`** — set by `synchronize_scrollbar_opacity` (`window.rs:3084`), read by the platform loops to **keep generating frames** (`macos/mod.rs:6047`, `windows/mod.rs:1064`) | **NO** |
| **virtual_view** | `VirtualViewManager` `virtual_view.rs:26` | `states: BTreeMap<(DomId,NodeId), VirtualViewState>` `:28`; `reason_overrides` `:36` | **NO** |
| **undo_redo** | `UndoRedoManager` `undo_redo.rs:192` | `node_stacks: Vec<NodeUndoRedoStack>` keyed by `NodeId` (`:86/:96`) | **NO** |
| **drag_drop (legacy)** | `DragDropManager` `drag_drop.rs:89` — *"**DEPRECATED**: use `GestureAndDragManager`"* (`:85`) | a **second** `active_drag: Option<DragContext>` `:91` | **NO** |
| **a11y** | `A11yManager` `a11y.rs:51` | `tree`, `last_tree_update`, `tree_initialized` | **NO** |
| others | `clipboard` `:110`, `file_drop`, `text_input`, `changeset`, `permission`/`geolocation`/`biometric`/`keyring`/`sensors`/`gamepad` (device managers, out of the redraw path) | | |

`selection.rs` in `managers/` is **only `StyledTextRun`/`ClipboardContent`** (`:24`, `:48`) — the
real selection state lives in `core/src/selection.rs`. Do not look for a `SelectionManager`; it
was removed (`common/layout.rs:1051`: *"SelectionManager removed — multi_cursor remap handled
above"*).

**The remap gap, precisely.** `update_managers_with_node_moves` (`dll/src/desktop/shell2/common/
layout.rs:998`, called `:420`) remaps exactly **five** things — focus, scroll, multi-cursor,
hover, gesture/drag — from `create_migration_map(&diff_result.node_moves)` (`core/src/diff.rs:830`).
`transfer_states` (`diff.rs:860`) is a *different* function: it only merges **datasets** via merge
callbacks; it does **not** touch manager `NodeId`s. **Not remapped, and each keys per-node state:
`undo_redo` (`Vec<NodeUndoRedoStack>` by `NodeId`), `virtual_view` (`(DomId,NodeId)`),
`gpu_state`, the legacy `drag_drop.active_drag`.** And `node_moves` carries *matched* nodes only —
**there is no unmount list any manager consumes for GC**, so entries for nodes that simply
disappeared are only dropped where a manager's own `remap_node_ids` chooses to drop them (scroll,
hover, gesture, multi-cursor do; the four above have no such code at all). **That asymmetry is the
hypothesis family (g4) below is built to prove or refute.**

Five families follow. **(g4) is the mechanically-generatable one and should be the biggest.**

---
**(g1) COMPOSITION — the managers fire together, in order, and reach a fixpoint**

> The canonical scenario, verbatim from the user: *click and drag → multi-node selection works →
> the node starts to scroll → the scroll is animated → and it is repainted incrementally.*

One timeline, five managers: `gesture` (drag detected) → `selection`/`text_edit` (anchor pinned,
focus follows) → `scroll_into_view`+`scroll` (autoscroll at the edge) → `scroll` animation ticks →
`cpurender` damage (a *patch*, not `Full`).

```json
{ "op": "assert_composition", "expect": ["drag_active","selection_grew","scroll_started","scroll_animating","damage_patch"] }
```
Asserts, over the steps since the last checkpoint: each named stage was **entered**, in the
**listed order**, and — critically — **the whole thing reaches a fixpoint**: after the final
`tick_ms`s the drag is over, the animation is over, and `assert_idle_stable` (a) holds.
*Observability today:* `get_drag_state`/`get_drag_context` (`full.rs:1829/1831`),
`get_selection_state`/`dump_selection_manager` (`:1823/:1825`), `get_scroll_states` (`:1737`),
`ScrollManager::has_active_animations()` (`scroll_state.rs:722`) — the state is all reachable from
`CallbackInfo::get_layout_window()`; only the **damage** half needs `FrameReport` (§1.3).

---
**(g2) CROSS-MANAGER CONSISTENCY — the pairwise invariants, derived from the code above**

Not guesses. Each one is a seam that exists in the source:

| # | Pair | Invariant that must hold |
|---|---|---|
| X1 | `scroll_into_view` ↔ `scroll_state` | `scroll_into_view` computes a `ScrollAdjustment` (`:47`) against the *same* `(DomId,NodeId)` that `ScrollManager.states` keys (`:298`) and applies it through `&mut ScrollManager`. **After a `scroll_into_view`, the target must be inside the container's visible rect according to `ScrollManager`'s own offset** — the two must not disagree about which container scrolls or by how much. A `ScrollAdjustment` for a container with no `AnimatedScrollState` entry is a bug. |
| X2 | `scroll` ↔ `scroll` animation | `has_active_animations()` (`:722`) is `true` **iff** some `AnimatedScrollState.animation` is `Some` (`:339`), **iff** `tick()` returns `needs_repaint` (`:412`). An animation that has reached its target must clear itself — see (g3). |
| X3 | `gesture.active_drag` ↔ `hover` hit-test | during a `TextSelection`/`Node` drag, the drag's source node (`core/src/drag.rs:106/:48`) must still exist in the StyledDom and `hover_manager.current_hover_node()` (`hover.rs:193`) must resolve against the **same** DOM. Drag says node 7, hit-test says node 7 doesn't exist ⇒ stale index. |
| X4 | `gesture.active_drag` ↔ `drag_drop.active_drag` | there are **two** `Option<DragContext>` in the tree (`gesture.rs:461`, `drag_drop.rs:91`) and only the first is remapped. **They must never disagree** about whether a drag is active or about its source node. (If they routinely do, delete the deprecated one — that is a finding.) |
| X5 | `selection` anchor ↔ DOM | `SelectionAnchor` is *"constant during a drag; only the focus moves"* (`core/src/selection.rs:590`). **The anchor's node must exist for the whole drag.** If the anchor's node is removed, `remap_node_ids` (`:540`) must **clear** the selection (`selection.rs:1847`), not leave a dangling anchor. |
| X6 | `focus` ↔ `text_edit.multi_cursor` | `multi_cursor` is *"`Some` whenever a contenteditable element has focus"* (`text_edit.rs:93`). **So: `multi_cursor.is_some()` ⇒ `focus_manager.focused_node` is `Some` and points at a contenteditable node that exists.** Blur must clear both (`clear_editing` `:201` / `clear_focus` `focus_cursor.rs:109`). |
| X7 | `focus` ↔ `scroll` | `scroll_cursor_into_view` (`scroll_into_view.rs:209`) scrolls the *focused* editor's cursor. If focus was cleared, no scroll adjustment may still be pending for it. |
| X8 | `text_edit` selection ↔ `scroll` autoscroll | a selection drag past the container edge must produce scroll **through `ScrollManager`**, not by moving the selection focus to a node that is not under the pointer. Selection focus and the scrolled container must stay mutually consistent frame-to-frame. |
| X9 | `gpu_state.scrollbar_fade_active` ↔ damage | `scrollbar_fade_active` (`gpu_state.rs:54`) keeps the platform loop generating frames (`macos/mod.rs:6047`). **It must go `false` within the fade duration of the last scroll**, or the window damages forever — see (g3). |
| X10 | any manager ↔ the StyledDom | **no manager key may refer to a node that no longer exists.** The universal one; it is what (g4) hammers. |

```json
{ "op": "assert_manager_invariants", "managers": ["scroll","hover","focus","gesture","selection","text_edit","virtual_view","undo_redo"],
  "cross": ["X1","X3","X5","X6","X9","X10"] }
```

---
**(g3) STATE-MACHINE LEAKS — "it ended, but the manager didn't notice"**

The interaction is over; the manager is not. Three shapes, all real code paths:

- **drag ended, drag still active.** After `mouse_up`, `gesture_drag_manager.active_drag`
  (`gesture.rs:461`) **and** `drag_drop.active_drag` (`drag_drop.rs:91`) must both be `None`, and
  `input_sessions` (`:459`) must not accumulate (`end_current_session` `:729`,
  `clear_old_sessions` `:856` exist — assert they *ran*).
- **animation finished, still scheduling frames. This is a direct infinite-redraw source.** After
  the scroll animation reaches its target, `has_active_animations()` (`scroll_state.rs:722`) must
  be `false`, `tick().needs_repaint` (`:412`) must be `false`, `scroll_dirty` (`:317`) must be
  cleared, and `gpu_state.scrollbar_fade_active` (`:54`) must return to `false` after the fade —
  otherwise the platform loop keeps generating frames forever (`macos/mod.rs:6047`,
  `windows/mod.rs:1064`). **This is precisely `damage_idle_scrollbar_window_skips`
  (`headless/mod.rs:2951`) — a bug that already shipped once.**
- **selection cleared, listeners still armed.** After a blur / `clear_editing` (`text_edit.rs:201`),
  `multi_cursor` must be `None`, `blink` cleared (`:78`), and `display_list_dirty` (`text_edit.rs:107`)
  must not stay latched `true` (a permanently-dirty flag = a permanent repaint).

Every one of these reduces to: **do the interaction, end it, then `assert_idle_stable` (a) +
`assert_state_machines_idle`.**
```json
{ "op": "assert_state_machines_idle" }
```
Asserts: no active drag (both managers), no active gesture session, no active scroll animation,
`scroll_dirty == false`, `scrollbar_fade_active == false`, no latched `display_list_dirty`,
and `FrameDamage::None`.

---
**(g4) DANGLING INDICES UNDER MUTATION — a FIRST-CLASS GENERATOR TEMPLATE**

> **Remove or replace the node that is currently being dragged / selected / scrolled / animated /
> focused / hovered — MID-INTERACTION — and assert: no panic, no stale index, manager state
> consistent, and the frame still settles.**

This is the family that targets the indexing bugs directly, and it is **mechanically
generatable**: it is a cross-product, not a set of ideas.

```
  for interaction in { text-selection drag, node drag, scrollbar-thumb drag,
                       momentum scroll, scroll_into_view animation, focus+caret,
                       hover, virtual-view scroll, undo stack on a node }
    for mutation   in { delete the node, delete its parent, delete a *preceding*
                        sibling (shifts every NodeId after it — the nastiest),
                        replace the subtree, reorder siblings, full DOM rebuild
                        via set_app_state }
      for phase    in { just after mouse_down, mid-drag, during the animation,
                        one frame before mouse_up }
        emit one JSON timeline
```
That is ~9 × 6 × 4 ≈ **200 tests from one template**, and they are the highest-yield tests in the
whole plan. Each ends with: `assert_no_panic` (free — process exit), `assert_manager_invariants`
(X10 — no key points at a dead node), `assert_state_machines_idle` (g3), `assert_idle_stable` (a).

**The hypothesis this family tests** (from the table above): `undo_redo`, `virtual_view`,
`gpu_state` and the legacy `drag_drop.active_drag` are **not in
`update_managers_with_node_moves`** (`common/layout.rs:998-1060`) at all, and *nothing* consumes an
unmount list for manager GC. **Expect (g4) to go red there first.** Note the "delete a preceding
sibling" case: it does not remove the node under interaction, it **renumbers** it — a manager that
stored a raw `NodeId` and was never remapped will now silently point at *a different, live node*.
That is a wrong-node bug, not a crash, and no other test in this plan would see it.

---
**(g5) INCREMENTALITY — "repainted incrementally" is an assertion, not an aspiration**

The user's fifth stage. During a drag-select-autoscroll, the repaint must be a **PATCH**:
`frame_report.paint_damage` must be `Rects(_)`, **never `Full`**, and the damage-driven buffer
must be pixel-identical to the full-repaint buffer (that is exactly (c) `assert_damage_sound`,
reused here — no new machinery). A scroll is the sharpest case: the fast path memmoves the clip
(large *present* damage) but only *paints* the newly exposed strip (small *paint* damage)
(`headless/mod.rs:829-834`), and `png_scroll_vertical_fast_matches_full_render` (`:3450`) already
proves the pixel identity for it in Rust.

```json
{ "op": "assert_damage_sound", "max_overpaint_ratio": 4.0, "forbid_full": true }
```

---
**Prerequisite for all of (g):** `debug_counts()` exists on only 4 of the managers
(`virtual_view.rs:95`, `hover.rs:69`, `scroll_state.rs:460`, `gesture.rs:538`). **Add it to
`focus_cursor`, `text_edit`, `gpu_state`, `undo_redo`, `a11y`, `drag_drop`** — all hold per-node
state, none are observable. And extend it to expose **the key set**, not just the count, or X10 is
not checkable.

---

### TIER 2 — LLM SANITY CHECK ("vibe test") — ADVISORY, **NEVER A CI GATE**

A judge model sees the headless screenshot **(or the frame *sequence* of an interaction
timeline)** + the one-line scenario description and returns `looks-right | looks-wrong |
suspicious` + a reason.

**Ask it about the BEHAVIOUR, in the language of the scenario:** *"this is a click-drag down a
list — does it look like a multi-node selection that scrolled?"*, *"is anything obviously
garbled, blank, or half-painted?"*, *"did the highlight follow the cursor?"*
**Never ask it about geometry or numbers.** Not "is this box 100px wide", not "is the gap 8px" —
**that is `azul-doc reftest`'s job (§0.1) and the judge is bad at it.** It is good at gross
failures no invariant expresses: blank screen, garbled/overlapping text, content that plainly
contradicts the description, z-order nonsense, a half-applied incremental patch.

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

## C. Test file format — a test is an INTERACTION TIMELINE

**A test file is a scripted timeline, not a DOM snapshot** (§0.1). It reads, top to bottom, as:

```
  mount a DOM  →  [ input event | time tick | DOM/state mutation ]*  →  invariants asserted
                  ─────────── interleaved, order is the test ───────────    BETWEEN the steps
```
The canonical shape the schema must express cleanly — the user's own scenario —
**`mouse_down` on a node → several `mouse_move`s dragging across its siblings (multi-node
selection grows) → the pointer reaches the container edge (autoscroll starts) → `tick_ms` ×N
(the scroll animates) → assert the repaint was a *patch* → `mouse_up` → assert everything
settles.** §C.6 is that file, literally.

Every op needed to write it **already exists**: `mouse_down` (`full.rs:1532`), `mouse_move`
(`:1528`), `mouse_up` (`:1538`), `scroll` (`:1569`), `key_down` (`:1577`), `delete_node`
(`:1898`), `insert_node` (`:1882`), `set_app_state` (`:1851`), plus the state probes
`get_drag_state` (`:1829`), `get_drag_context` (`:1831`), `get_selection_state` (`:1823`),
`get_scroll_states` (`:1737`), `get_focus_state` (`:1863`). **The timeline is not the gap. The
gap is the assertions** (§B) **and `tick_ms`.**

Keep the **existing** `E2eTest`/`E2eStep` schema (`full.rs:3218/3263`) — it already works, it
already runs in CI, and every debug op is already a valid step. Add: a way to mount a DOM, a
`setup` block extension, `tick_ms`, and the Tier-1 assertion ops.

**There is deliberately no geometry assertion in this schema.** `assert_layout` (`full.rs:3360`)
exists and stays for legacy files, but **generated tests must not emit it** (§0.1). The gate in
§D.3 rejects it.

### C.1 The `mount` op — the one enabling primitive

Without this, thousands of *independent* tests are impossible (§1.1). Two options:

**Option A (recommended) — a dedicated test host + the *existing* `setup.app_state`.**
Build `examples/azul-e2e-host` whose app state is `{ "xml": "...", "css": "..." }` and whose
`layout_callback` calls `parse_xml_to_styled_dom` (`layout/src/xml/mod.rs:196`). Then
**`setup.app_state` (`full.rs:3246`) and the `set_app_state` op (`full.rs:8621`) mount arbitrary
DOM+CSS with ZERO engine changes** — the plumbing already exists (it needs a `RefAny` with a
`deserialize_fn`, `full.rs:8625`, which an azul-owned Rust host trivially provides).
It also gives the Tier-2 shard a free home for the pinned `HARNESS_FONT` (§2.A).

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
    "window_width": 800, "window_height": 600, "dpi": 96,   // A3 — MANDATORY (so the container really overflows)
    "app_state": { "xml": "<div class='a'>hi</div>", "css": ".a { width: 100px; }" },  // C.1
    "freeze_clock": true                                    // A2 — NEW. The one that matters.
  },
  "steps": [
    // ── TIMELINE: input events, time ticks and mutations, interleaved ──
    // ACTIONS — all of these EXIST today (full.rs:1526-2073)
    { "op": "wait_frame" },
    { "op": "tick_ms", "ms": 16 },                          // NEW (A2) — advances the virtual clock
    { "op": "mouse_down", "x": 10, "y": 20, "button": "left" },   // full.rs:1532
    { "op": "mouse_move", "x": 40, "y": 90 },                     // full.rs:1528  (drag)
    { "op": "mouse_up",   "x": 40, "y": 90, "button": "left" },   // full.rs:1538
    { "op": "click", "selector": ".btn" },                  // or x/y, or node_id, or text
    { "op": "key_down", "key": "a" },
    { "op": "text_input", "text": "hello" },
    { "op": "scroll", "x": 50, "y": 50, "delta_x": 0, "delta_y": -100 },
    { "op": "resize", "width": 400, "height": 300 },
    { "op": "set_node_css_override", "node_id": 3, "property": "background-color", "value": "red" },
    { "op": "insert_node", "parent_id": 1, "node_type": "div", "classes": ["x"] },
    { "op": "delete_node", "node_id": 3 },                  // ← the (g4) weapon: fire it MID-drag
    { "op": "set_app_state", "state": { "xml": "...", "css": "..." } },   // = remount

    // ── ASSERTIONS — Tier 1. ALL OF THESE ARE NEW. ──
    { "op": "assert_idle_stable",     "ticks": 5 },                            // B.1a
    { "op": "assert_changed",         "min_damage_rects": 1 },                 // B.1b
    { "op": "assert_damage_sound",    "max_overpaint_ratio": 4.0,
                                      "forbid_full": true },                   // B.1c + B.1g5
    { "op": "assert_work_bounded",    "max_relayouts": 2, "max_dom_regens": 1 },// B.1d
    { "op": "snapshot_resources",     "as": "baseline" },                      // B.1e
    { "op": "assert_resource_counts", "vs": "baseline", "fonts": "eq" },       // B.1e
    { "op": "assert_no_growth",       "frames": 200 },                         // B.1f
    { "op": "assert_composition",     "expect": ["drag_active","selection_grew",
                                                 "scroll_started","scroll_animating",
                                                 "damage_patch"] },            // B.1g1
    { "op": "assert_manager_invariants", "managers": ["scroll","hover","focus","gesture",
                                                      "selection","text_edit","undo_redo"],
                                         "cross": ["X1","X3","X5","X6","X9","X10"] },  // B.1g2
    { "op": "assert_state_machines_idle" },                                    // B.1g3

    // ASSERTIONS — existing, and still fine: they are about EXISTENCE and CONTENT, not geometry
    { "op": "assert_exists", "selector": ".a" },
    { "op": "assert_text", "selector": ".a", "expected": "hi" },
    // FORBIDDEN in generated tests (§0.1): assert_layout, assert_css, assert_screenshot.
    // Geometry belongs to `azul-doc reftest`. The §D.3 gate rejects these.

    // TIER 2 only — never in a tier-1 file
    { "op": "capture", "as": "after_drag" }                 // NEW — writes a PNG for the judge
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

### C.6 Worked example 4 — **THE CANONICAL ONE.** drag → multi-select → autoscroll → animate → incremental repaint (Tier 1g1/g2/g5).

This is the user's scenario, as a timeline. Note: **not one geometry assertion in it.**

```json
{
  "name": "compose_drag_multiselect_autoscroll_animate_incremental",
  "description": "The user presses in the first list row and drags down past the bottom edge: the multi-node text selection must grow, the list must autoscroll, the scroll must animate to a stop, every repaint must be an incremental patch (never a full redraw), and on mouse-up everything must settle to zero damage with no manager left active.",
  "tier": 1,
  "setup": {
    "window_width": 400, "window_height": 200, "dpi": 96, "freeze_clock": true,
    "app_state": {
      "xml": "<body><div id='list'><p class='row'>row one</p><p class='row'>row two</p><p class='row'>row three</p><p class='row'>row four</p><p class='row'>row five</p><p class='row'>row six</p><p class='row'>row seven</p><p class='row'>row eight</p></div></body>",
      "css": "#list { height:100px; overflow-y:scroll; } .row { height:30px; user-select:text; }"
    }
  },
  "steps": [
    { "op": "wait_frame" },
    { "op": "assert_idle_stable", "ticks": 3 },
    { "op": "snapshot_resources", "as": "baseline" },

    // ── press inside row 1 ──────────────────────────────────────────────
    { "op": "mouse_down", "x": 20, "y": 10, "button": "left" },
    { "op": "wait_frame" },

    // ── drag down: selection must GROW across rows ─────────────────────
    { "op": "mouse_move", "x": 60, "y": 40 }, { "op": "wait_frame" },
    { "op": "mouse_move", "x": 60, "y": 70 }, { "op": "wait_frame" },
    { "op": "assert_changed", "min_damage_rects": 1 },
    { "op": "assert_damage_sound", "max_overpaint_ratio": 4.0, "forbid_full": true },

    // ── drag PAST the bottom edge: autoscroll must start ───────────────
    { "op": "mouse_move", "x": 60, "y": 130 },
    { "op": "wait_frame" },
    { "op": "tick_ms", "ms": 16 }, { "op": "tick_ms", "ms": 16 }, { "op": "tick_ms", "ms": 16 },
    { "op": "tick_ms", "ms": 16 }, { "op": "tick_ms", "ms": 16 },

    // ── every stage fired, in order, and the repaint stayed a PATCH ────
    { "op": "assert_composition",
      "expect": ["drag_active", "selection_grew", "scroll_started",
                 "scroll_animating", "damage_patch"] },
    { "op": "assert_damage_sound", "max_overpaint_ratio": 4.0, "forbid_full": true },
    { "op": "assert_work_bounded", "max_relayouts": 2, "max_dom_regens": 0 },
    { "op": "assert_manager_invariants",
      "managers": ["gesture", "selection", "text_edit", "scroll", "hover", "focus"],
      "cross": ["X1", "X3", "X5", "X6", "X8", "X10"] },

    // ── release, let the momentum/fade run out, and SETTLE ─────────────
    { "op": "mouse_up", "x": 60, "y": 130, "button": "left" },
    { "op": "tick_ms", "ms": 400 },
    { "op": "assert_state_machines_idle" },
    { "op": "assert_idle_stable", "ticks": 5 },
    { "op": "assert_resource_counts", "vs": "baseline", "images": "eq" }
  ]
}
```
Read what each block buys: the drag block is **(g1) composition**; `forbid_full` is **(g5)
incrementality**; `cross` is **(g2)**; the tail is **(g3) state-machine leaks** — after `mouse_up`
+ 400 ms of virtual time, `active_drag` must be `None` in **both** drag managers (X4),
`has_active_animations()` must be `false`, and `scrollbar_fade_active` must have gone back down
(X9). **The trailing `assert_idle_stable` is what catches the infinite-redraw tail** that a
scroll-fade or a never-cleared animation leaves behind.

### C.7 Worked example 5 — mid-interaction mutation: delete the node being dragged (Tier 1g4).

The generator template. **This one is expected to find bugs.**

```json
{
  "name": "mutate_delete_dragged_node_midflight",
  "description": "While a node-drag is in flight, the dragged node is deleted from the DOM. Nothing may panic, no manager may keep a key to the dead node, the drag state machine must terminate, and the window must settle.",
  "tier": 1,
  "setup": {
    "window_width": 400, "window_height": 200, "dpi": 96, "freeze_clock": true,
    "app_state": {
      "xml": "<body><div id='list'><p class='row' id='r1'>one</p><p class='row' id='r2'>two</p><p class='row' id='r3'>three</p></div></body>",
      "css": "#list { height:60px; overflow-y:scroll; } .row { height:30px; }"
    }
  },
  "steps": [
    { "op": "wait_frame" },

    { "op": "mouse_down", "x": 20, "y": 10, "button": "left" },
    { "op": "mouse_move", "x": 20, "y": 45 },
    { "op": "wait_frame" },

    // the drag is live and the managers have keys pointing at node 2 …
    { "op": "delete_node", "node_id": 2 },          // … now it is gone. MID-DRAG.
    { "op": "wait_frame" },

    // no panic (free: process exit code), no dangling key, no zombie drag
    { "op": "assert_manager_invariants",
      "managers": ["gesture", "hover", "focus", "scroll", "selection", "text_edit",
                   "undo_redo", "virtual_view"],
      "cross": ["X3", "X4", "X5", "X10"] },

    { "op": "mouse_move", "x": 20, "y": 55 },        // keep dragging a node that no longer exists
    { "op": "wait_frame" },
    { "op": "mouse_up", "x": 20, "y": 55, "button": "left" },
    { "op": "tick_ms", "ms": 400 },

    { "op": "assert_state_machines_idle" },
    { "op": "assert_work_bounded", "max_relayouts": 2, "max_dom_regens": 1 },
    { "op": "assert_idle_stable", "ticks": 5 }
  ]
}
```
**Why this is the highest-yield template in the plan.** `update_managers_with_node_moves`
(`common/layout.rs:998`) remaps five things and **`undo_redo`, `virtual_view`, `gpu_state` and the
legacy `drag_drop.active_drag` are not among them** (§B.1g). And `node_moves` only describes
*matched* nodes — **nothing hands any manager an unmount list.** Vary `delete_node` → *delete a
preceding sibling* and the node under the drag is not deleted but **renumbered**: a manager that
was never remapped now points at a live but **wrong** node. Silent. This is the exact bug shape
the family exists to surface.

**Note how small each file is.** A cheap model can emit one of these from a single one-line
*scenario* **without ever reasoning about expected geometry** — which is exactly the property
§0.1 and §B were designed to give it.

## D. The generation pipeline

Mirrors `scripts/autotest_fleet.sh` conventions. **Read that script before writing this one** —
its lessons were expensive.

### Stage 1 — ONE strong agent writes the case list. **The one-liners are INTERACTION SCENARIOS.**

```
scripts/e2e/cases/<subsystem>.txt     # one one-line INTERACTION SCENARIO per line
```
Subsystems, and note what is **not** here: no `layout-flex`, no `layout-text`, no `css-cascade`
— **that is `azul-doc reftest`'s territory (§0.1).** The list is behavioural:
`compose-drag-select-scroll`, `mutate-midflight` *(the (g4) cross-product — the biggest shard)*,
`managers-scroll`, `managers-scroll-into-view`, `managers-gesture-drag`, `managers-selection`,
`managers-focus-caret`, `managers-hover`, `managers-text-edit`, `managers-undo-redo`,
`virtual-view`, `state-machine-leaks`, `damage`, `idle`, `resources`.

A one-liner looks like this — **a story, not a measurement**:

> *"drag from node A across B and C while the list autoscrolls, then delete B mid-drag"*
> *"scroll a container with momentum, then blur the window mid-fling"*
> *"focus a contenteditable at the bottom of a scroll container, type until the caret scrolls
> itself into view, then rebuild the DOM"*

Fable/Opus at high effort **reads the subsystem source** (e.g.
`layout/src/managers/scroll_state.rs`) and writes N of them. Bias the prompt hard:

> *"For `layout/src/managers/gesture.rs` + `core/src/drag.rs`: write 120 one-line e2e INTERACTION
> SCENARIOS. Each is a sequence a user performs — press, drag, tick, mutate, release. Each must be
> a scenario where a BEHAVIOUR bug would show: managers that must compose (drag→selection→
> autoscroll→animation→incremental repaint); a state machine that must terminate; a node deleted
> or renumbered MID-interaction; a settle that must reach zero damage. **Layout correctness is NOT
> your problem — Chrome reftests own that. Never mention a pixel, a size, or a coordinate as an
> expected value.** One per line. Describe the SCENARIO, not the answer."*

The "never an expected value" instruction is load-bearing: it keeps Stage 2 in Tier-1 territory,
where the harness itself is the oracle.

### Stage 2 — MANY cheap agents fan out, one line → one `.json`

`haiku`/`sonnet`, `--jobs 12`. Each agent gets: **the schema (§C.2), the two canonical timeline
examples (§C.6 composition, §C.7 mid-flight mutation), and one line.** It must NOT need to reason
about expected geometry — and per §D.3 it is *not allowed* to emit a geometry assertion at all.

**The (g4) shard is not written by an LLM at all** — it is the cross-product in §B.1g4 emitted by
a ~50-line script over `{interaction} × {mutation} × {phase}`. ~200 tests, zero token spend, and
the highest expected bug yield in the suite.

```
scripts/e2e/tests/<subsystem>/<nnn>_<slug>.json
```

### Stage 3 — the mechanical gate

Three checks, in order. **Anything that fails 1 or 2 is deleted and regenerated. Failing 3 is
FINE — it may be a real bug.**

1. **Schema validation** — parses as `E2eTest`, every `op` is in the known set, every required
   param present. (Reject unknown ops: `evaluate_assertion` silently returns
   `fail("Unknown assertion: …")` (`full.rs:3382`), so a typo'd op looks like a *bug*, not a
   malformed test. Catch it here.) **Also reject, hard: any `assert_layout`, `assert_css` or
   `assert_screenshot` step, and any numeric coordinate/size used as an *expected* value.** That
   is out of scope (§0.1) and a generator that emits one has misunderstood the task — delete and
   regenerate. And reject a "test" with no input events and no ticks: it is a snapshot, not a
   timeline.
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
- Virtual clock (`AZ_TEST_CLOCK` + `tick_ms`) — **A2; without it Tier 1a and every animated
  interaction timeline is flaky.**

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
| **F2** | **Nondeterminism from system fonts.** | **Mostly a non-risk, by design (§2.A):** every Tier-1 invariant compares azul against *itself* in one process, so the runner's font cancels on both sides. Only the advisory Tier-2 screenshot sweep wants a pinned rendering: embedded `HARNESS_FONT` + empty `FcFontCache`. **That solution already exists** (`headless/mod.rs:1894-1932`) — copy it, and read `:1897-1921` for why the obvious approach fails. |
| **F3** | **Wall-clock nondeterminism.** Scrollbar fade / momentum / blink / scroll animation damage the frame on a timer, and an interaction timeline cannot assert that an animation *finished*. | A2 virtual clock. **This is the highest-risk unbuilt item and now the ONLY hard determinism requirement**; do it in Phase 0 or idle-stability and every animated timeline gets quarantined within a week. |
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

3. **The virtual clock (A2) is in Phase 0, not deferred.** It is the least glamorous item and the
   one most likely to get cut — and if it is cut, `assert_idle_stable` (the infinite-redraw
   detector, i.e. the highest-value assertion in the plan) is flaky and gets quarantined.
   **Confirm it stays in Phase 0.**

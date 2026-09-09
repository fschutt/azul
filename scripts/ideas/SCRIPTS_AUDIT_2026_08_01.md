# scripts/ documentation audit — 2026-08-01

**Scope:** all 185 `.md` files under `scripts/` (plus `text3_review/confirmed_findings.json`), each read and then
checked against the current tree at `aaa700097`. Plus a full-codebase sweep for `TODO`/`FIXME`/`HACK`/WIP markers.

**Method.** 16 topic-clustered agents read their files, extracted the concrete claims (types, functions, file
paths, flags), and verified each claim with `rg` against the code that exists *today* — never trusting a doc's own
status line. A 17th agent produced the marker inventory in Section B. Findings that two agents disagreed on were
re-verified by hand; those corrections are in §6.

**How to read this.** Section A is one entry per file with a verdict. Section B is the codebase marker sweep.
Nothing in the repo was moved, edited, or deleted to produce this report.

---

## 1. Verdicts at a glance

| Verdict | Count | Meaning |
|---|---:|---|
| **DELETE** | 85 | Implemented, superseded, or a dead end. Recoverable from git. |
| **RESEARCH** | 52 | Durable design rationale or cross-toolkit comparison → `scripts/research/`. |
| **ACTIVE** | 28 | Still tracks unfinished work not recorded anywhere else. |
| **ARCHIVE** | 21 | Historical session/handoff log. No forward value; git holds it. |

Two structural facts frame everything below:

- **144 of 185 files were committed once and never revised.** They are snapshots, not living documents. Only six
  have real churn (`MOBILE_SESSION_LOG.md` 264 commits, `MANAGER_FIX_PROGRESS.md` 46, `CLEANUP_PLAN.md` 31,
  `WEB_1TO1_SUPERPLAN.md` 20, `WIDGETS_RELEASE_PLAN.md` 16, `HIGHLEVEL_1_5_PLAN.md` 13). A snapshot that was
  accurate when written and never updated is the default state of this folder.
- **The drift is bidirectional.** Docs under-report progress as often as they over-report it. `SCROLLBAR_BUGS.md`
  leaves four boxes unchecked that are all in fact fixed; `VEC_ITERATOR_PLAN`'s checkboxes badly understate what
  landed; `COMPONENT_SYSTEM_STATUS.md` has ~6 rows that are stale-wrong in the optimistic direction. **Do not use a
  checkbox in this folder as evidence of anything.**

---

## 2. Before deleting anything: two hard constraints

**2.1 — 36 `scripts/*.md` files are load-bearing.** They are cited from source comments, `doc/guide/`,
`doc/autodoc-groups.toml`, and `.github/workflows/rust.yml`. Deleting one breaks `scripts/check_links.py` and, in
three cases, an actual build input. Most-cited:

| File | Refs | Cited from |
|---|---:|---|
| `WACOM_TOUCH_API_RESEARCH.md` | 5 | X11 + Wayland XInput2 ABI tables in `dll/src/desktop/shell2/linux/` |
| `ARCHITECTURE.md` | 5 | `doc/autodoc-groups.toml:35,793`, `doc/guide/en/internals/code-organization.md` |
| `RED_FFI_FINDINGS.md` | 4 | `doc/src/codegen/v2/lang_red/mod.rs:12`, `examples/red/hello-world.red:9` |
| `BINDING_STRATEGY_PER_LANGUAGE.md` | 4 | `layout/src/thread.rs:441`, `doc/src/codegen/v2/` ×3 |
| `HIGHLEVEL_SUPERPLAN.md` | 4 | `doc/src/reftest/autoreview.rs:2078` **appends to it programmatically** |
| `SUPER_PLAN_2.md` | — | cited by section number from five `core/src/*.rs` files |

Three are *live build inputs*, not just references:
- `DEBUG_API.md` → embedded verbatim by `doc/autodoc-groups.toml:572` + `doc/src/reftest/autodoc.rs:512,522` to
  generate `doc/guide/en/debugging.md`. It is stale (predates ~30 ops, all assertions, `mount`/`tick_ms`).
  **Regenerate in place — do not move or delete.**
- `HIGHLEVEL_SUPERPLAN.md` → `autoreview.rs` appends bullets to it on each run.
- `ARCHITECTURE.md` → an autodoc group member.

**2.2 — 10 references are already dangling.** Previous cleanups deleted files without fixing referrers:

`BENCH_REPORT_M11_2026_05_19.md` (cited by `examples/c/README-web.md:112`), `HACKS_REVIEW_2026_05_16.md` (×2),
`ICON_SYSTEM_ANALYSIS.md` (cited by `doc/guide/en/reference.md:125` as the basis for a doc that still needs
writing), `STATUS_REPORT_2026_05_18.md`, `SVG_CLIP_MASKS_AGENT_PROMPT.md`, and the scripts `find_layout_commits.py`,
`screenshot.sh`, `ios-runner.sh`, `ios/entitlements.plist`, `detect.rs`.

**Recommended order for any cleanup: fix the 10 dangling refs → rewrite the 36 referrers → then delete.**

---

## 3. Which ideas were superseded by better ones

This is the substantive answer to "what did we learn." In each case the *later* idea is in the tree today.

| Superseded idea | Won instead | Why the winner is better |
|---|---|---|
| `scroll4`'s render-loop `physics_tick` | `scroll3`'s **timer-based** scroll (ScrollManager as pure input recorder → reserved-ID physics timer → `CallbackChange::ScrollTo`) | Decouples physics from frame rate; works when the render loop is idle. Live in `layout/src/managers/scroll_state.rs` + `scroll_timer.rs`. |
| `CLIPPING_ANALYSIS_REPORT`'s `clip_rect` intersection | `WEBRENDER_CLIPPING_ANALYSIS` §8.4 **spatial-vs-clip separation** | The intersection fix conflated two independent WebRender concepts; it was correctly rejected. |
| `DAMAGE_RENDERING`'s "don't set `draw_previous_partial_present_regions`" | The opposite — **shipped against its central premise** | Doc's premise was simply wrong about WebRender's present model. |
| `OPENGL_TEXTURE_SWAP_OPTIMIZATION`'s API | `OPENGL_DOM_DIFF_OPTIMIZATION`'s approach | Same problem solved without a new public API surface. |
| `IFRAME_ANALYSIS` → `IFRAME_INVESTIGATION_REPORT` → **`IFRAME_SCROLL_DISPLAY_LIST_ARCHITECTURE`** | `DisplayListItem::VirtualView` / `VirtualViewPlaceholder` / `VirtualViewCallback` | "IFrame" was WebRender's name for a compositing mechanism, not azul's concept. Renaming removed a persistent category error. |
| `XmlComponentTrait` (`dyn Trait` XML components) | `ComponentDef` / `ComponentFieldType` / `ComponentDataModel` in `core/src/xml.rs` | Data-driven and FFI-expressible; the trait object could not cross the C ABI. |
| Imperative permission lifecycle API | **Permissions as DOM nodes** (`SUPER_PLAN_2.md` §1.5, `research/08`) | Invisible probe `NodeType`s + `EventFilter::*Permission*` + a permission-diff pass after layout. Shipped as designed. |
| A `wasm32-unknown-unknown` port | **remill x86/ARM64 → WASM lift** | Explicitly rejected at `dll/Cargo.toml:336`: a port forks the codebase, a lift does not. |
| CanvasKit-style glyph blitting to canvas | **Browser DOM as a passive render target** (`WEB_BACKEND_1TO1_PLAN` §6/§6b) | WASM owns cascade/layout/text and emits only semantic patches — never glyph positions, never `getBoundingClientRect`. Fidelity comes from spec accuracy, not measurement. |
| Browser-reftest geometry comparison | **Self-comparison inside one process** (`E2E_PLAN`) | Font/DPI/runner variance cancels. Tier-1 assertions carry no expected values, so they cannot enshrine a bug. |
| `CssProperty::Scrollbar` nested struct | Flat variants (`css/src/props/property.rs:733`) | Removed a 1520-byte `CssProperty`; `tier3_overflow` deleted outright. |
| Compact-cache Tier 3 | **Hot/cold split + negative caching** (`CompactNodePropsCold`, `hot_flags`/`extra_flags`/`dom_declared_flags`) | The code went past all three doc generations; Tier 3 was never needed (`rg tier3` → 0 hits). |
| `debug_server.rs` | `layout/src/e2e/full.rs` (deleted in `80704c8fe`) | Every component-system doc still cites the old path. |
| Android "zero Java" premise | A small Java shim | The premise didn't survive contact with the APK/activity lifecycle. |
| `GETTER_MIGRATION_PLAN`'s motivation | Achieved by a different design | Getters are now macro-generated: `rg "pub fn get_"` returns 11, not the 113 the audit counted. |

**One superseded claim worth remembering as a lesson, not a design:** `BTREEMAP_TO_VEC_PLAN.md`'s risk section
asserted the sentinel swap was "safe, no semantic change." It was not — `POSITION_UNSET` (`f32::MIN`) leaked into
the display list twice, five months later (`7ac52d301`, `584f5797f`, the 624×0 drop warnings).

---

## 4. What is still missing — the consolidated gap list

Ranked by user-visible impact. Every item verified present in the tree today.

### 4.1 Functional gaps that users would notice

1. **No text input on either mobile platform.** iOS has no `UIKeyInput`/`insertText:`/`becomeFirstResponder`;
   Android has no soft-keyboard/IME bridge (`NativeInputConnection` exists only in docs). iOS also has no
   `UIPasteboard` clipboard. Both platform plans are otherwise ~90% shipped.
2. **macOS text input silently swallows relayout.** `RegenerateLayoutIncremental` falls into `_ => {}` at
   `dll/src/desktop/shell2/macos/events.rs:746` — an edit needing relayout gets neither relayout nor redraw.
3. **Paged/PDF rendering drops clips, stacking contexts, opacity and filters** (`display_list.rs:5120,5193`).
4. **Rich clipboard is dead end-to-end.** `layout/src/window.rs:9154` still carries
   `// TODO(superplan): styled_runs left empty`; `core/src/events.rs:407 ClipboardEventData` is still untyped.
5. **Skip-ink underline: zero implementation.** `rg 'skip_ink|has_descender'` is empty; underlines are one
   continuous rect per run (`display_list.rs:4441-4470`).
6. **`::selection` does not parse as a pseudo-element.** Styling landed as ad-hoc `SelectionBackgroundColor` /
   `SelectionColor`; `CssPathPseudoSelector` (`css/src/css.rs:1748`) has no Selection variant.
7. **Multi-node icon replacement keeps only the root** (`core/src/icon.rs:621`).
8. **~105 CSS spec non-conformances** in `SPEC_CONFORMANCE_REVIEW.md`; 12 re-verified as still reproducing, e.g.
   `overflow:hidden` on inline-only content silently loses its BFC (`layout_tree.rs:3340`).
9. **`ua_css.rs:834`** — `(Html, Height) => HEIGHT_100_PERCENT` is commented out since 2026-06-02 (jump-table
   mis-dispatch). This is the foundation of body-level scrolling; the real fix named in the comment is not done.

### 4.2 Correctness / safety

10. **AVX gated on CPUID leaf-1 ECX[28] with no XGETBV check** (`core/src/gpu.rs:132-133`). Reports that the CPU
    *implements* AVX, not that the OS enabled YMM state → **SIGILL** on a kernel that didn't `XSETBV`-enable it.
    The robust gate is `is_x86_feature_detected!`. Verified: zero XGETBV hits anywhere in the tree.
11. **`core/src/refany.rs:1291` `set_serialize_fn` data race** (UB).
12. **Font / font-instance GC leak.** Image GC shipped (`DeleteImage` at `wr_translate2.rs:1826`), but
    `DeleteFont`/`DeleteFontInstance` have no emitter; `remove_font_families_with_zero_references`
    (`core/src/resources.rs:1472`) has only test callers, admitted in a comment at `:1448`.
    `scan_used_images` still ignores `_css_image_cache` (`window.rs:3785`).

### 4.3 Architectural work with a written design and no implementation

13. **`FrameChanges` consume-once newtype: zero hits.** Both enums are exhaustive (`SystemChange`
    `core/src/events.rs:2645`, `apply_system_change` `dll/.../event.rs:3157`) but nothing forces a backend to drain
    both lists. This is the concrete, designed fix for the 7-hand-rolled-event-loop problem. Today's stand-in is a
    grep test (`dll/tests/backend_feature_parity.rs`) whose own header records two features silently missing on
    whole platforms.
14. **Overlay refactor is ~80% consumed, not complete.** `ContentChange` has no `Text` or `Structural` arm, so text
    and structural edits bypass the chokepoint and are **never journaled**; two in-place `set_node_type` DOM
    mutations survive at `dll/src/desktop/shell2/common/event.rs:2095` and `:2960` — precisely the rule the plan
    exists to enforce. GPU epoch still `layout_window.epoch`; no `overlay`/`journal` rows in `manager_fingerprints`.
15. **Async task API: nothing implemented.** `MapWidget` still spawns one OS thread per tile
    (`layout/src/widgets/map.rs:1090`, `MAX_SPAWN_PER_CALL: 16`). Four design questions need a ruling.
16. **Damage is still display-list-diff, not layout-level.** `DAMAGE_REGION_PLAN` §4/§5 never landed;
    `css_property_damage` / `DamageCollector` have zero hits. `is_visually_equal` falls to `_ => false` for `Image`
    and gradients (`display_list.rs:1088`), so **any image re-damages every frame**. Dead-but-dangerous:
    `scroll_layer` / `compute_exposed_rects` (`compositor.rs:588,762`) with a documented inverted sign convention.
17. **OS style metrics are queried but reach no consumer.** No `GestureDetectionConfig::from_input_metrics`; zero
    `GestureManager::new` call sites in `dll/`; `CURSOR_BLINK_INTERVAL_MS = 530` still const;
    `windows/mod.rs:3785` still `* 20.0`; no COLR emoji. Only Task F (xdg-decoration) landed.
18. **Mobile gamepad backends are literal stubs** — `extra/gamepad/{apple,android}.rs` are 17- and 16-line
    `pub fn start() {}`. No rumble on any platform. **Wacom pad has no producer anywhere**: `update_pad_state` is
    called only from a unit test (`gesture.rs:3112`).
19. **Smooth zoom never built** — `map_on_scroll` (`widgets/map.rs:907`) still jumps ±0.5 zoom instantly.
20. **No CDP bridge and no FLIP/layout-animation layer** (`ARCH_TODO` ch.4/5). The keys landed
    (`core/src/dom.rs:1922`); everything downstream did not.

### 4.4 Test / release infrastructure

21. **The e2e corpus was never generated.** Ops, assertions, runner, xfail and a blocking CI gate
    (`rust.yml:2496,2558`) all shipped — but `e2e/gen/` does not exist. 9,530 case lines (`E2E_TESTS.txt`) plus
    6,812 Wave-1 lines sit unexpanded next to 38 hand-written scenarios.
22. **`assert_no_silent_fallbacks` and `AZ_E2E_NEUTER` were designed and never built** — the two named defences
    against the false-green class that recurred six times in eleven days.
23. **~10 e2e protocol input holes** verified open (0 hits in `layout/src/e2e/full.rs`): IME preedit, file drop,
    clipboard, pen eraser/barrel, scroll source, mouse-leave, theme, WM frame state, monitors.
24. **The `+spec:` hash index is broken** — 1385 annotations, none resolvable, and `azul-doc spec show` does not
    exist. This blocks working the CSS conformance list at all.
25. **Release-size levers untouched.** Hyphenation is still `embed_all` **and default**
    (`layout/Cargo.toml:64,159`) — 2.8 MB in every artifact. No core/full feature split (−8–12 MB). Zero hits for
    `--remap-path-prefix`, `thumbv7neon`, `--icf=all`, RELR.
26. **`/dl` indirection never built** — choco/brew still emit version-pinned `azul.rs/ui/release/{V}/…`
    (`build_registry_mirrors.sh:581`), the exact P0 fragility flagged three months ago.
27. **mini.wasm is ~25 MB against a 500 KB budget.** Only L0 landed (brotli, `server.rs:658-733`); the diet plan's
    prerequisite `AZ_LIFT_REPORT` size accounting does not exist.
28. **No vec-iterator safety smoke tests in any language** (`scripts/test_vec_iter_safety_all.sh` absent) — the
    iterate/close/use bug class is unverified even where marked FIXED. Option/Result outer-free is still missing in
    Haskell/Go/PHP/Node (only Ruby + Lua landed) → per-call heap leak.
29. **Red has never been compiled by any toolchain.** `rust.yml:2921` deliberately omits `redc` and calls it
    "ALPHA/broken" — contradicting its BETA tier, `tabOrder` slot (`api.json:29-31`) and frontpage entry.

### 4.5 Regressions against a doc's own goal

30. **`HashMap`→`BTreeMap` regressed.** `CLEANUP_PLAN` counted ~322 sites; today it is 560. `core/src/events.rs`
    grew 3686 → 6318 lines with its test-split still unchecked.
31. **x86/Windows web lift still blocked.** `dll/src/web/loader_js.rs:520` reads `if (false && initRc === 0 …)` —
    the hydrate gate disabled in June — and the three `__remill_*` impls it needs are still absent. (aarch64/macOS
    lift is *done*; all source workarounds were deleted in `b5e6a7e55`.)
32. **C++ deducing-`this` is dead code** behind a literal `let use_deducing_this = false;`
    (`doc/src/codegen/v2/cpp20.rs:907,1017`). `lang_red` still emits `TODO2` pointer-width union blobs and maps
    `i64`→`byte-ptr!`. `WriteBackCallback` is absent from `HOST_INVOKER_KINDS`; no emitter actually calls
    `PyGILState_Ensure`/`rb_thread_call_with_gvl`, so the per-VM lock table is documented but unimplemented.

---

## 5. Proposed `scripts/research/` layout

52 files carry durable design rationale. `scripts/research/` already holds 8 (`01`–`08`); all eight should stay,
with two caveats noted below. Suggested grouping:

**`research/architecture/`** — `ARCHITECTURE.md`, `ARCH_TODO.md`, `SUPER_PLAN.md` (headless-window cross-compile
thesis), `SUPER_PLAN_2.md` (permissions as DOM nodes), `OVERLAY_JOURNAL_REFACTOR_PLAN.md`, `dump.md` (callback
pointer → `file:line` → "Open in VS Code"; unimplemented and still wanted).

**`research/layout-css/`** — `COMPACT_CACHE_PLAN.md` (the DOD case study: flamegraph → property-frequency histogram
→ tier-by-access-phase → cache budget → bit-packing), `SCROLL_ARCHITECTURE.md` (three-sizes, no-viewport-scroll),
`SCROLL_COORDINATE_ARCHITECTURE.md` (coordinate-space newtypes), `PERF2.md` (f16 vs i16×10), plus an **excerpt** of
`DEFERRED_CASCADE_DESIGN.md` §1.1/§3.3/§5.4 — components as asymmetric CSS scope boundaries. (The rest of that file
is DELETE; the agent that classified it also named it a top research keeper. Excerpt, don't keep whole.)

**`research/text/`** — `report-selection.md` (why an IFC is the indivisible unit of text layout, hence hit-testing
needs a node→IFC-root indirection; plus the "store logical cursors only, compute rects at render time" ruling),
`TEXT_SELECTION_ARCHITECTURE.md`, `TEXT3_HINTING_REVIEW_2026_07_06.md` (divergence-class → suspect-code map for
CoreText parity), `FONT_INVALIDATION_AND_MEMORY_LAYOUT_ANALYSIS.md`.

**`research/events/`** — `EVENT_ARCHITECTURE_ANALYSIS_DOC.md` ← **the single best doc in the folder**:
exhaustive-enum-per-actor plus a consume-once newtype as the type-level substitute for a hand-written
frame-lifecycle contract. Also `EVENT_ARCHITECTURE_ANALYSIS.md` (W3C parity matrix), `DRAG_DROP_REPORT.md`,
`IFRAME_SCROLL_DISPLAY_LIST_ARCHITECTURE.md`, `TAG_ID_SYSTEM_BUGS.md` (the only written record of the tag-id/
hit-test architecture + the manager-remap-after-DOM-regeneration invariant).

**`research/rendering/`** — `DAMAGE_REGION_PLAN.md` (render/present two-channel model; the asymmetric "PresentDamage
must never silently be ∅" rule), `WEBRENDER_CLIPPING_ANALYSIS.md`, `webrender-diff-report.md` (the only inventory of
what the vendored fork actually changed), `ANIMATION_SHADER_DESIGN.md` (View Transitions vs FLIP vs Framer Motion
`layoutId` vs Rive, arguing for live-node FLIP with interruption/velocity-retarget as a first-class criterion).

**`research/components/`** — `COMPONENT_TYPE_SYSTEM_DESIGN.md` (repr(C) reflection across a C ABI),
`WIDGET_JSON_FEASIBILITY_REPORT.md` (§5.2 empirically shows NodeGraph is 3764 LOC but ~20 data-model fields →
"the data model is always simple, the complexity is in the render fn"), `WIDGETS_RESEARCH.md`.

**`research/platform/`** — `SYSTEMSTYLE.md` (maps each styling dimension to the API returning the *resolved* value;
names CLI/registry parsing as the anti-pattern — and zero `process::Command` remains in any system-style path),
`X11_API_REFERENCE.md`, `PLATFORM_DND_MENU_RESEARCH.md` (4-protocol DnD map), `CROSS_COMPILE_COMPAT.md`
(dlopen-everything as portability policy + API introduction dates).

**`research/bindings/`** — `LANGUAGE_EXPANSION_RESEARCH.md` ← **best in cluster**: archetype A/B taxonomy, "the C
ABI is the floor" as the single requirement, a falsification test that finds only removed-FFI DSLs, ~35 cited
sources. Plus `CI_ONLY_LANGS_RESEARCH_2026_07_06.md` (the sharper C1/C2 bindability test),
`BINDING_STRATEGY_PER_LANGUAGE.md` (per-VM callback/threading contract, cited normatively by 4 source files),
`RED_FFI_FINDINGS.md` (a deliberate falsification test of the "bind any language" thesis).

**`research/web/`** — `WEB_LIFT_BUG_COMPENDIUM.md` ← **the single durable artifact of the whole web-lift effort**:
25 lifting failure modes each tagged INHERIT/ISA/ABI/OPEN (i.e. which fixes port across ISAs and which don't), plus
nine methods — the best being *"a no-op stub looks identical to a mis-lift at the call site: grep `class=` before
debugging any garbage value."* Two durable findings are **missing** from it and should be folded in first: Rust
niche-discriminant mis-reads (→ `#[repr(C,u8)]`, live at `core/src/dom.rs:1069`) and the SwissTable/
auto-vectorization hazard. Also `WEB_BACKEND_1TO1_PLAN.md` (§6/§6b), `WASM_SHIPPING_OPTIONS.md`,
`M8_7_HYDRATION_PLAN_2026_05_16.md` (why JSON hydration, not a memory dump), `mechb_harness/README.md`.

**`research/product/`** — `E2E_PLAN.md`, `STARTUP_LATENCY.md`, `HTTPS_TLS_ANALYSIS.md` (TLS stack choice for a
no-C-code toolkit), `PACKAGE_DISTRIBUTION_PLAN.md`, `AZMEET_TRANSPORT_DESIGN.md` (transport comparison + an
original codec/layout insight; unimplemented), `RELEASE_SIZE_MEMORY_AUDIT_2026_07_04.md` (a cdylib cannot
dead-strip what its export table roots → feature composition, not `--gc-sections`, is the only size lever for a
C-API product).

**Existing `research/01`–`08` — keep all, with two required edits:**
- `07_libsql_sqlite.md` — **its top recommendation was inverted in implementation.** Add a prominent status header
  or it will mislead.
- `06_mvt_pdf.md` — both halves shipped, but via a different node model than proposed. Note the delta.

**If only three files survive:** `EVENT_ARCHITECTURE_ANALYSIS_DOC.md`, `LANGUAGE_EXPANSION_RESEARCH.md`,
`WEB_LIFT_BUG_COMPENDIUM.md`.

---

## 6. Corrections made during verification

Two agents disagreed; both were re-checked by hand before publishing.

- **RTL text is *not* broken.** The marker sweep flagged `layout/src/text3/cache.rs:7155`
  (`TODO(text3-review): RTL glyph-level visual reversal is NOT applied`) as the single highest-impact gap — all
  Hebrew/Arabic rendering backwards. It is a **stale comment**. `apply_l2_visual_reversal` exists at
  `cache.rs:8642` and is called at `:9087`; the three tests the comment names as failing are live and not
  `#[ignore]`d. Ran them: `hebrew_run_is_rtl_reversed_and_33px_wide ... ok`,
  `bidi_mixed_run_is_80px_and_reverses_hebrew ... ok` — 2 passed, 0 failed. **The comment should be deleted.**
- **The web/WASM backend is alive, not dormant.** 21,228 LOC in `dll/src/web/` (13 modules) wired at
  `dll/src/lib.rs:180-184`, last touched 2026-07-29; live CI publishing `ghcr.io/fschutt/azul-web-base`;
  `wasm32-unknown-unknown` in the CI target matrix (`rust.yml:424`); shipping layout code `#[cfg]`s on a real
  `web_lift` feature (`layout/src/solver3/fc.rs:394`). Code markers run to **M12.7** — past every doc in that
  cluster. The *lift* effort is dormant and split by arch (§4.5 item 31); the backend is not.

Two caveats on scope, stated plainly:
- The bindings cluster's "landed" means *present in source and wired*, not *proven green on a runner* — codegen was
  not re-run.
- `HANDOFF_web_vec_return_len_mislift_2026_06_06.md` and `PROMPT_web_helloworld_NEXT.md` assert a root cause that
  was later **disproven**. They are misleading if read standalone; flag before archiving.

---
---

# Section A — By file

Each entry: what the file contained, what actually shipped (with evidence), what superseded it, what is still open,
and what is worth keeping.



## Part 01 — Overall architecture & super-plans (14 files)

Audit date 2026-08-01, verified against `master` @ `f1c43ba60`.

---

#### scripts/ARCHITECTURE.md

- **Verdict:** RESEARCH — durable onboarding code-map of the whole pipeline; needs a link refresh.
- **Was:** A 190-line "architecture overview for new maintainers": crate table (core/css/layout/dll/webrender), entry points, the `UI = f(data)` / `RefAny` state-graph model, the **two complementary event systems** (window-state diffing vs manager-based accumulation), a per-OS input/decoration table, a manager inventory, solver3+text3 layout, CSS struct table, font/image loading, and the display-list → WebRender → present pipeline. Ends with 8 "open questions for new maintainers".
- **Landed:** Structure is broadly accurate today. Verified: `layout/src/solver3/` (cache/fc/sizing/positioning/taffy_bridge/display_list…), `layout/src/solver3/taffy_bridge.rs`, `dll/src/desktop/compositor2.rs`, `layout/src/window.rs:714 pub struct LayoutWindow`, `layout/src/managers/scroll_state.rs:296 ScrollManager`, `layout/src/managers/focus_cursor.rs:53 FocusManager`, `layout/src/managers/gpu_state.rs:48 GpuStateManager`, `layout/src/text3/cache.rs:860 FontManager<T>`. **Stale names/paths:** `GestureManager` → actually `GestureAndDragManager` (`layout/src/managers/gesture.rs:445`); `CursorManager` (`managers/cursor.rs`) does not exist → `TextEditManager` (`managers/text_edit.rs:135`) + `FocusManager`; `SelectionManager` was deleted (superseded by `MultiCursor`, commit `42b68f940`) — `managers/selection.rs` now only holds `StyledTextRun`/`ClipboardContent`; `IFrameManager` (`managers/iframe.rs`) does not exist → `VirtualViewManager` (`managers/virtual_view.rs:26`); `LayoutCache` is now `LayoutCacheEntry` (`solver3/cache.rs:135`); `shell2/common/event_v2.rs` is now `shell2/common/event.rs`.
- **Superseded by:** partially — `doc/guide/en/architecture.md` (746 lines, published) covers the *conceptual* side (OOP vs React vs IMGUI paradigms, backreferences, state-graph rationale) but contains **no** code map, so it does not replace this file. Still consumed by `scripts/build_gemini_prompt.sh:195` (with a stale hardcoded `/Users/fschutt/...` path).
- **Still open:** ~7 stale symbol/path references above; would mislead a new maintainer. Either fix in place or promote into `doc/guide/en/`.
- **Research value:** The **"two complementary event systems"** framing — simple user-modifiable state handled by frame-to-frame *diffing*, temporally-complex input (gestures, IME composition, drags) handled by *accumulating managers* — with the explicit goal of "compartmentalization, not elimination of complexity". That is a genuine "why azul does X differently" argument vs Qt/GTK signal dispatch and vs browsers' single event-queue model.

---

#### scripts/ARCH.md

- **Verdict:** DELETE — one-shot Gemini patch-merge triage; every architectural fix it demanded has shipped.
- **Was:** "Architecture Review — Cross-Patch Analysis" for the **run2** batch of 800 AI-generated CSS-spec patches (committed 2026-03-06, `30cfe3547`). Three cross-patch contradictions (table anonymous boxes, `word-break`/`line-break`, abspos width/height), three "tunnel vision gaps" (display blockification, float clearance vs margin collapsing, white-space Phase I/II ordering), plus ABI/regression warnings about `#[repr(C)] LayoutNode` cache tiers and the Taffy 9+1 cache slots.
- **Landed:** All three "architectural fixes" exist. Centralized blockification: `layout/src/solver3/getters.rs:2562 blockify_display()` + `:2599 get_computed_display()`, with tests at `getters.rs:6655`, `:6677`, `:6700`. Clearance-inhibits-margin-collapsing: `layout/src/solver3/fc.rs:1455/1533/1580/1587/1734/2253` (spec text quoted verbatim). White-space pipeline: `split_text_for_whitespace` is a shared entry consumed from `solver3/sizing.rs:1166`. The `+spec:` traceability annotations landed too (1386 occurrences across `*.rs`).
- **Superseded by:** `scripts/run3-arch.md` (same document shape, later patch batch) and, functionally, the shipped code above.
- **Still open:** none. Patch IDs it names (`table-layout_001`, …) refer to `doc/target/skill_tree/all_patches/run2_patches/`, which is not in the tree.
- **Research value:** One transferable observation only: parallel single-paragraph spec agents systematically produce *scattered `if`-chains where the spec implies one centralized predicate* — the recurring fix is "extract the matrix/predicate first, then apply patches". Better captured once (see run3-refactoring.md) than three times.

---

#### scripts/ARCH_TODO.md

- **Verdict:** RESEARCH — comparative-GUI feature/design brainstorm; ~40% unbuilt, incl. the CDP-bridge and FLIP-animation designs.
- **Was:** A 630-line LLM brainstorm dump (Dec 2025, last touched `ccf94a8ca`) in five unrelated chapters: (1) ICU4X↔`AzString` interop + an advanced-i18n API checklist (plurals, select, list/date/number formatting, BiDi, **UI mirroring**, locale-aware font fallback/Han unification, collation, segmentation, locale fallback chains, hot-swap); (2) a "Rust-Bootstrap" widget library + a **remote "smart icon"** async fallback chain (Remote→Cache→Disk→Default) + OFL font research (Inter/Roboto/Noto, Material Symbols variable font); (3) a "pro-status" roadmap (IME, a11y, OS integration, DnD, DPI, DX, theming); (4) a **Chrome DevTools Protocol bridge** (`--debug-port=9222`) so Puppeteer/Playwright can drive a native Rust app; (5) **React-style keyed reconciliation + FLIP layout animations + "zombie DOM" exit transitions + shared-element (`layout-id`) magic-move**.
- **Landed:** Ch.1 — `impl AsRef<str> for AzString` (`css/src/corety.rs:204`) and `impl From<String>` (`:449`) both exist; `layout/src/icu.rs` is a full ICU layer. Ch.2 — the Bootstrap widget set shipped: `layout/src/widgets/{alert,badge,card,accordion,breadcrumb,modal,popover,tabs,chip,ribbon,segmented,stepper,…}.rs`; `dll/src/desktop/material_icons.rs` exists. Ch.5 partial — **keys landed**: `core/src/dom.rs:1922 pub key: Option<u64>` with the doc-comment "Stable key for reconciliation … track this node across frames even if its position in the array changes", plus `dataset_merge_callback` for resource preservation across frames.
- **Superseded by:** n/a for the unbuilt parts; the widget chapter is superseded by the shipped `layout/src/widgets/`.
- **Still open:** (a) **No CDP bridge** — `rg chromiumoxide|DOM.getDocument` finds only `doc/src/reftest/*` (azul *drives* Chrome for reftests) and `dll/src/web/transpiler_remill.rs`; azul is not a CDP *target*, so Puppeteer/Playwright automation of native azul apps does not exist. The in-house `shell2/common/debug_server/` + `debugger/` speak a private protocol instead. (b) **No FLIP/layout animation**: no `prev_rect`/`visual_rect`/`target_rect` in `solver3/cache.rs`, no zombie/exit-transition layer, no `layout-id` shared-element map; `core/src/animation.rs` (cited by ARCHITECTURE.md) no longer exists. (c) No `IconManager`/`IconSource` remote-icon system.
- **Research value:** Two keepers. **(i)** "Speak CDP and you inherit the world's debugger + E2E automation ecosystem for free" — a concrete argument for protocol-compatibility over a bespoke inspector, and a direct comparison point vs Electron/browsers. **(ii)** The **FLIP + keyed-reconciliation + zombie-DOM** blueprint: how a retained-layout-cache toolkit gets Framer-Motion-class transitions without a virtual DOM, including the `visual_rect` vs `target_rect` split and the "shared element by `layout-id`, not by node identity" trick.

---

#### scripts/HIGHLEVEL_1_5_PLAN.md

- **Verdict:** ARCHIVE — a 9-item worklog where every item is marked DONE and verified done.
- **Was:** Work plan for branch `feat/highlevel-items-1-5`, derived from the HIGHLEVEL_SUPERPLAN audit. Items 1–5: macOS file-drop end-to-end, display_list pagination text no-op, cpurender `backdrop-filter`+`text-shadow`, Wayland tooltip text shaping, `shape-outside: path()` + ruby shaping. Round 2 items 6–9: macOS global menu bar/context menu, Windows OLE `IDropTarget` DnD, X11 XDND, Wayland `wl_data_device` DnD. Includes per-item verification notes and a "don't ship plausible-but-unverified visual output" convention.
- **Landed:** All verified. `layout/src/cpurender/raster.rs:2473 render_text_shadow` + `text_shadow_stack` threading (`:949/:1181/:1254/:1519/:2015`) and tests at `:3582 mod text_shadow_tests`, `:3660 text_shadow_paints_offset_colored_pixels`. `shape-outside: path()`: `layout/src/text3/cache.rs:10885 flatten_svg_to_path_segments`, `:10936 path_segments_line_intersection`, used at `:3691`. Ruby: `cache.rs:79 RUBY_ANNOTATION_FONT_SCALE = 0.5`, `:86 ruby_reserved_box`, with a test asserting the scale is *not* the old `0.6` magic (`:11752`). XDND >3-type + hover was the last commit to touch this file (`1e76084cf`).
- **Superseded by:** n/a — it is the completion record of HIGHLEVEL_SUPERPLAN's five remaining gaps.
- **Still open:** two documented deferrals, both still true in the tree: (1) `layout/src/text3/cache.rs:7924 TODO2` — ruby annotation glyphs are sized and reserve space but are **not emitted as a separately-positioned centered run**; needs a ruby-aware `ShapedItem` variant. (2) macOS-only lossy `EventProcessResult` vs core `ProcessEventResult` was explicitly SKIPPED (101 `EventProcessResult` references remain).
- **Research value:** The convention block is a reusable rule for agent-driven rendering work: *"conservative on rendering — prefer reftest verification; don't ship plausible-but-unverified visual output; if too risky, leave a `TODO2:` with the reason and mark PARTIAL"*. Also the proof-test pattern (render to in-memory `AzulPixmap`, assert real pixels; skip gracefully with no system font). Not enough for `scripts/research/` on its own.

---

#### scripts/HIGHLEVEL_SUPERPLAN.md

- **Verdict:** ACTIVE — ~85% shipped, but a handful of named gaps are still literally in the tree.
- **Was:** The 2026-06-20 machine-generated ("`azul-doc autoreview summarize-highlevel`") architectural cleanup plan: ~35 items grouped into **9 parallelizable groups by file ownership**, with an explicit hot-file conflict map so one agent per group can run without collisions. Plus a "Deletion audit results" section recording a 10-agent re-check of 67 auto-applied mid-level commits (verdict: no regressions, every deletion SUPERSEDED or genuinely DEAD).
- **Landed:** Most of it. Group 1: `layout/src/fragmentation.rs` and `layout/src/paged.rs` are **gone** — consolidation onto `solver3/pagination.rs` done. Group 2: the global `static IFC_ID_COUNTER` is now a thread-local `Cell` (`solver3/layout_tree.rs:24/44/53`). Group 5: `sync_clipboard` dead copies dropped on macOS+Windows and the Wayland/X11 ones are now *called* (`linux/wayland/mod.rs:933-937`, `linux/x11/mod.rs:1183`, dispatched from `linux/mod.rs:102`). Group 7: `get_window_display_info` **deleted everywhere** (0 hits); GNOME menu conversion now wired (`gnome_menu/manager.rs:211` calls `MenuConversion::convert_menu`). Group 8: `enable_tab_navigation` removed (0 hits); `impl Ord for CssPropertyWithConditions` at `css/src/dynamic_selector.rs:1313`; `ColorOrSystem`/`parse_color_or_system` now imported in `css/src/props/style/background.rs:17/28`. Group 9: `EventData::TextInput(TextInputEventData)` landed at `core/src/events.rs:461`; `gpu_state.rs` dead fade-tick subsystem removed (no `fn tick`/`ScrollbarFadeState`). Group 4: real `child_rect` via `compute_scroll_child_rect` (`wr_translate2.rs:801/891`); `get_image_ref_for_image_source` now a real match on `ImageSource` (`display_list.rs:4867`). Group 3: `delete_range` multi-run implemented (`text3/edit.rs:241`, 12 tests incl. `delete_range_spanning_runs_merges_matching_styles`).
- **Superseded by:** partially by `scripts/HIGHLEVEL_1_5_PLAN.md` (its 5 "remaining gaps") and by the 2026-07-31 seam audit (memory: `azul-seam-audit-2026-07-31`).
- **Still open:** concrete, verified-present leftovers: **(1)** rich-clipboard `styled_runs` is *still* always empty — `layout/src/window.rs:9154` carries a literal `// TODO(superplan): styled_runs left empty` and `:9159 styled_runs: Vec::new().into()`, so `StyledTextRun`/`to_html()` remain FFI-exported dead machinery. **(2)** `OverflowInfo.overflow_items` is still never populated — `layout/src/text3/cache.rs:5822-5833` and `:9178` ("`overflow_items` stays empty by …"), i.e. the field is documented-dead rather than wired or removed. **(3)** `IcuLocalizerHandle` still uses the manual `run_destructor: bool` refcount (`layout/src/icu.rs:1164/1167`) instead of `Arc`. Groups 6 (drag-path consolidation) and the shell2 dedup items (macOS GLView/CPUView, Windows/X11 a11y adapter) were not spot-verified.
- **Research value:** The **"one agent per file-ownership group, with an explicit shared-hot-file conflict map"** planning format is the transferable artifact — it is the only doc in this cluster that solves parallel-agent collision *structurally* rather than by ordering rules. Secondary: the deletion-audit methodology (re-check every deletion with `git grep` for callers + `api.json` for public surface + `git log -S` for re-adds, then classify SUPERSEDED vs DEAD).

---

#### scripts/SUPER_PLAN.md

- **Verdict:** RESEARCH — the "headless window" cross-compile thesis is durable; the sprints are all done.
- **Was:** The 2026-05-19 iOS+Android backend plan (branch `mobile-ios-android`). Core idea: reuse the desktop `headless` CPU pipeline (`LayoutWindow → DisplayList → cpurender::render() → AzulPixmap → {PNG | UIView | ANativeWindow}`) so mobile-target binaries can be **built and pixel-verified on a macOS host with no simulator, no Xcode project, no Gradle, no Android Studio**. 13 sprints (A–N) with GOAL/FILES/GATE each, a host-tool inventory, 8 architectural decisions, and Sprint N: cross-compiling iOS `.app` bundles **from Linux** (extracted iOS SDK + `lld -flavor darwin`/`cctools-port` + an `xcrun` shim + `ldid -S` fake-signing).
- **Landed:** `dll/src/desktop/shell2/{ios,android}/` both exist (`mod.rs` + `accessibility.rs`); `scripts/build-ios.sh`, `scripts/build-android.sh`, `scripts/ios/{Info.plist,entitlements.xcent}`, `scripts/android/{AndroidManifest.xml,AzulActivity.java,AzulFilePicker.java,AzulAccessibilityBridge.java,NativeGestureBridge.java}`, `scripts/check-prereqs-mobile.sh`, `scripts/MOBILE_SESSION_LOG.md` all present. Recent master commits (`4e6568581` APK native libs at `lib/<abi>/`, `780a34066` x86_64-android target) show the pipeline is live in CI.
- **Superseded by:** `scripts/SUPER_PLAN_2.md` for everything above the backend layer.
- **Still open:** small, mostly cosmetic drift from the plan: Sprint C's `ios/coregraphics.rs` and `ios/display_link.rs` were never created as separate files (folded into `ios/mod.rs`); Sprint J's `scripts/mobile-headless-snapshot.sh` + `scripts/mobile/golden/*.png` do not exist; Sprint K's `scripts/CROSS_COMPILE_MOBILE.md` and `.github/workflows/mobile.yml` do not exist (workflows are `rust.yml`, `rust9x.yml`, `dockery.yml`, `docker-base.yml`, `post-release.yml`). Sprint N (Linux-host iOS) is unverified.
- **Research value:** Two keepers. **(i)** *"The headless CPU backend is simultaneously the regression harness and the screen-less window"* — one `cpurender::render()` implementation, two consumers (on-device pixmap + CI golden PNG). That is a real architectural argument against the winit/wgpu norm of a GPU-first mobile port. **(ii)** Sprint N's **Linux-host iOS cross-compile recipe** (SDK extraction + `lld` Mach-O + `cctools-port` ld64 + `xcrun` shim + `ldid -S`) is genuinely hard-won, non-obvious, and not documented anywhere else in the repo.

---

#### scripts/SUPER_PLAN_2.md

- **Verdict:** RESEARCH — the "permission-aware DOM node" model is a real design thesis; also still cited by shipped source.
- **Was:** The mobile-era integration super-plan: 13 feature topics × 5 platforms (camera, screen-share, biometric, sensors, gamepad, Wacom, mobile file pickers, mobile IME, geolocation, MVT map tiles, PDF both directions, libsql/SQLite), a **dependency-isolation rule** (`dll/src/desktop/extra/<feature>/`), a **PDF path decision** (printpdf → SVG → existing SVG renderer, explicitly *no* `pdfium-render`), **"goal apps"** (AzulPaint / AzulMaps / AzulVault / AzulDoc) as a scope-discipline device, §1.5 the **permission-aware DOM architecture**, a per-feature deliverable template, 8 research-agent output files, a P1–P6 priority ordering, and a tracker with explicit GATED items.
- **Landed:** Essentially all of it. `scripts/research/01..08_*.md` all exist. `dll/src/desktop/extra/` contains `audio, biometric, camera, capability.rs, file_picker, gamepad, geolocation, keyring, map, pdf, permission, screencap, sensors, sqlite, video_codec, webtransport`. Managers exist: `layout/src/managers/{permission,biometric,geolocation,keyring,sensors,gamepad,clipboard,virtual_view}.rs`. Goal apps shipped as real crates: `examples/{azul-paint,azul-maps,azul-vault,azul-writer,azul-camera,azul-gamepad,azul-screenshare,azul-video,azul-meet,azul-spirit-level}`. The tracker's three GATED items are now **closed**: `HoverEventFilter::{PenSqueeze,PenDoubleTap,PenHover}` at `core/src/events.rs:1727/1730/1736`; `CallbackInfo::get_location_fix` in use at `examples/azul-self-test/src/lib.rs:262` and `examples/azul-maps/src/lib.rs:41`.
- **Superseded by:** n/a — this plan *is* the current mobile architecture; `scripts/research/*` are its expanded outputs.
- **Still open:** only the P6 tail: `dll/src/desktop/extra/wacom_pad/` (ExpressKeys / touch-rings) was never created. The "Blocking on the user" disk-pressure note is stale (see memory `azul-machine-build-safety`). **Caution before deleting:** shipped source doc-comments cite this file by section — `core/src/keyring.rs:2` (§4 P4.2), `core/src/screencap.rs:2` (§4 P6), `core/src/biometric.rs:2` (§1 feature 4), `core/src/sensors.rs:2` (§1 feature 5), `core/src/audio.rs:1` (§4 P7). Deleting it orphans those references.
- **Research value:** The headline concept: **permissions as DOM nodes, not lifecycle API calls.** `NodeType::{CameraPreview, ScreenCapture, GeolocationProbe, BiometricGate, SensorProbe, MapTile, Pdf, Database}` (some invisible zero-size "probe" nodes) + `EventFilter::*PermissionRequired/Granted/Denied`, with a permission-diff pass after every layout that requests on node-appear and releases on node-disappear. The doc argues this explicitly against the imperative `App::request_camera_permission(...)` model and maps it onto the W3C Permissions API / `permissionchange`. Secondary keepers: the **"goal app" scope-discipline device** ("if a sub-feature doesn't unblock the goal app, defer it") and the **printpdf→SVG→existing-renderer** decision that avoided a `pdfium` dependency.

---

#### scripts/REFACTOR.md

- **Verdict:** DELETE — pre-patch groundwork list for a vanished patch corpus; the surviving ideas landed under different names.
- **Was:** "Architecture Groundwork Outline" (Gemini pipeline, `30cfe3547`, 2026-03-06): 10 abstractions to introduce *before* applying 800 run2 patches — cascade-bridge getters, box-model math helpers (`margin_box()`/`padding_box()`/`content_box()`), a phase-based white-space pipeline, table-generation traversal helpers, `resolve_absolute_containing_block()`, a centralized display-blockification matrix, CSS §10.3/§10.6 dimension **equation solvers**, inline-fragment edge tracking for `box-decoration-break`, an `establishes_stacking_context()` predicate, and a `LineBoxMetrics` half-leading accumulator.
- **Landed:** 4 of 10, under the plan's own names or close: `blockify_display`/`get_computed_display` (`solver3/getters.rs:2562/2599`); `establishes_stacking_context` in `solver3/display_list.rs`; `margin_box`/`padding_box` helpers in `solver3/geometry.rs` (+ `fc.rs`, `positioning.rs`, `text3/cache.rs`). **Not landed under these names:** `resolve_absolute_containing_block` (0 hits), `solve_horizontal_formatting_equation` (0), `LineBoxMetrics` (0), `is_first_visual_fragment` (0), `wrap_nodes_in_anonymous_box` (0), `find_consecutive_non_cell_children` (0) — the code solved those problems differently (procedurally in `sizing.rs`/`fc.rs`).
- **Superseded by:** `scripts/run3-refactoring.md` (same document *form*, later batch, and a much higher landing rate — 12 of 14).
- **Still open:** nothing tracked here that isn't better tracked elsewhere. The unbuilt items (a real §10.3/§10.6 equation solver, a `LineBoxMetrics` accumulator) are legitimate refactors but nobody is waiting on them.
- **Research value:** none unique — the "extract the abstraction *before* letting N parallel agents patch the same function" method is stated more clearly and with better evidence in `run3-refactoring.md`.

---

#### scripts/CLEANUP_PLAN.md

- **Verdict:** ACTIVE — 12 unchecked items, and at least 4 are verifiably still open in the tree.
- **Was:** The 2026-06-20 investigation-backed cleanup checklist (53 items, REMOVE/REFACTOR/BUILD-OUT/KEEP/INVESTIGATE, effort-tagged 🟢🟡🔴) across core/, layout/, dll/web/, dll/, and cross-cutting. Carries a detailed "Execution status" header recording what shipped that pass: the **`RefAny` on-update hook** (`update_fn` on `RefCountInner`, fired on `downcast_mut`), the generic **`RefAnyUndoManager`** (JSON-snapshot mini-git), AzJson serde-parity tests, the web-server bounded worker pool (no tokio), the **`misc` API module elimination** (47 types → 15 proper modules, shipped `15f46f992`), and undo/redo E2E in `rust.yml`.
- **Landed:** 41 of 53 boxes checked. Two *unchecked* items are actually done: **cpurender.rs split** — `layout/src/cpurender/` is now `{mod,compositor,raster,svg,pixmap}.rs`; **clippy de-liberalization** — both `layout/src/lib.rs` and `dll/src/lib.rs` now carry the "extreme-lint lockdown" (`clippy::pedantic/nursery/cargo` + `unreachable_pub`, `unsafe_op_in_unsafe_fn`, `variant_size_differences`, …), not blanket allows.
- **Superseded by:** partially by the 2026-07-31 seam audit for the rendering-correctness items.
- **Still open (verified):** **(1) HashMap→BTreeMap** — the plan says ~322 sites; today `rg -c HashMap` over `core/src layout/src dll/src` totals **560**, i.e. it regressed. **(2) `core/src/events.rs` test-split** — plan cites 3686 lines; the file is now **6318** lines. **(3) CPU hit-test CSS transforms** — `layout/src/headless.rs` now has transform-chain machinery (`:150 from_transform_3d`, `:226 has_transform`), so this may be closed; needs a real check, not the checkbox. **(4) Rich clipboard typed content** — `core/src/events.rs:407 ClipboardEventData` still carries just `content: Option<…>` (`:5127` constructs `content: None`); pairs with the HIGHLEVEL_SUPERPLAN `styled_runs` gap. **(5)** SVG DOM-path unification, web-server dirty-sync `update_fn` registration, ICU cross-backend parity tests, `source_language`, swappable `<icon>` — all still `[ ]`.
- **Research value:** Low but nonzero: the **`RefAny` on-update hook → generic undo/redo + web-state dirty-sync** chain is a nice demonstration that one reactive primitive (a callback on `downcast_mut`) unlocks two unrelated features. Also the KEEP/DEFER-with-reasoning discipline (recording *why* an item was not done) is worth imitating.

---

#### scripts/run3-arch.md

- **Verdict:** DELETE — run3 patch-merge triage; every architectural fix it prescribed exists in the tree.
- **Was:** Same shape as ARCH.md but for the **run3** batch of 78 patches (`55ca586cc`, 2026-03-12). Four cross-patch contradictions (inline-block baseline fallback, display blockification, `display:contents` on replaced elements, upright-LTR override), three tunnel-vision gaps (`scrollbar-gutter` reserved in the wrong layer, scattered `visibility:hidden` checks ignoring inheritance, `text-box-trim` shifting physical item positions), three architectural changes (unified font metrics, `unicode-bidi: plaintext`, a sticky-positioning module), and ABI/regression concerns.
- **Landed:** Verified present: `layout/src/solver3/positioning.rs:961 adjust_sticky_positions<T: ParsedFontTrait>` (the prescribed dedicated module); `layout/src/solver3/cache.rs:1531 compute_scrollbar_info_core` (the single scrollbar-reservation chokepoint shared by BFC and Taffy paths); `layout/src/solver3/layout_tree.rs:3223 const fn is_replaced_element` + test at `:5533` covering "the CSS Display 3 Appendix B set"; `layout/src/solver3/getters.rs:2599 get_computed_display`; `layout/src/text3/cache.rs:7023 get_base_direction_from_logical`; `LayoutFontMetrics` gained `x_height: Option<f32>` / `cap_height: Option<f32>` at `text3/cache.rs:2156/2159`.
- **Superseded by:** the shipped code, and by `scripts/run3-refactoring.md` which restates the same conclusions as an actionable checklist.
- **Still open:** one prescription did **not** land under its name — the centralized `is_node_visible()` visibility-inheritance helper (0 hits). `visibility` inheritance may be handled in the property cache instead, but the doc's specific concern (scattered paint-time `get_visibility() == Hidden` checks that miss `visible` children of `hidden` parents) is unverified.
- **Research value:** One genuinely transferable design ruling: **`text-box-trim` must shrink the container's bounds, never shift physical glyph positions** — shifting breaks logical→physical coordinate mapping and therefore hit-testing, selection and cursors. That is a real "invariant you only learn by breaking it". Otherwise superseded.

---

#### scripts/run3-refactoring.md

- **Verdict:** DELETE — a 14-item groundwork checklist; 12 items verifiably shipped.
- **Was:** "Refactoring Groundwork Plan (GROUNDWORK.md)" — the actionable distillation of run3-arch.md: 14 numbered abstractions to build *before* applying the run3 patches, each with What / Why / Where / "Needed for patches".
- **Landed:** 12/14 verified: (1) unified blockification → `getters.rs:2599`; (2) replaced-element `display:contents` → `layout_tree.rs:3223`; (5) `x_height`/`cap_height` on `LayoutFontMetrics` → `text3/cache.rs:2156/2159`; (6) scrollbar-gutter in `compute_scrollbar_info_core` → `cache.rs:1531`; (7) `adjust_sticky_positions` → `positioning.rs:961`; (8) overflow-clip-margin helper → `css/src/props/layout/overflow.rs` (32 refs); (10) `unicode-bidi: plaintext` → `text3/cache.rs:7023`; (11) `is_hanging_punctuation_char` → `text3/cache.rs` (11 refs); (12) `is_visible_or_clip` → `solver3/getters.rs` (5 refs) + `layout_tree.rs`; (13) `text-box-trim` → `css/src/props/property.rs`, `core/src/prop_cache.rs`, `solver3/getters.rs`; (14) `fixed_position_item_ranges` → `solver3/display_list.rs` (14 refs) + `cpurender/compositor.rs`. Items 3 (inline-block baseline) and 4 (upright-LTR in `WritingModeContext`) not individually confirmed.
- **Superseded by:** the shipped code. Note the run2-era `scripts/REFACTOR.md` is the *earlier, worse* version of this same document — this one has the far higher landing rate (12/14 vs 4/10), plausibly because it named smaller, concrete helpers rather than large solvers.
- **Still open:** item 9, the centralized `is_node_visible()` helper (0 hits) — same gap as run3-arch.md.
- **Research value:** The **method** is the keeper, and this is its best exemplar: *before letting N parallel single-spec-paragraph agents loose on the same function, extract the shared predicate/helper first, then each patch becomes a one-liner.* If any one file from the run2/run3 cluster is kept for `scripts/research/`, it should be this one — but the concept compresses to a paragraph and is already stated in HIGHLEVEL_SUPERPLAN's group-ownership design, so DELETE stands.

---

#### scripts/run3-review.md

- **Verdict:** ARCHIVE — a per-patch grading table for 78 patches that no longer exist in the tree.
- **Was:** Code review of the run3 patches in three tables: **A. Refactoring Needed** (13 conflict clusters, same content as run3-arch/run3-refactoring), **B. Lazy/Misleading Patches to Redo** (2 patches that claimed a feature but only added a `// +spec:` comment — `box-model_3393da` "shape-margin" and `height-calculation_b32921`), **C. Good Implementation Patches** (~33 rows praising individual patches).
- **Landed:** Table A's conclusions all shipped (see run3-refactoring above). Table C is unverifiable per-row without the patch corpus; spot-checks are consistent (e.g. `overflow-clip-margin` `<visual-box>` parsing exists in `css/src/props/layout/overflow.rs`).
- **Superseded by:** `scripts/run3-refactoring.md`, which is the same information in actionable form.
- **Still open:** one thing worth carrying forward: the doc flags `box-model_3393da` as REJECT-and-rewrite (`shape-margin` on `layout_initial_letter` was never implemented, only annotated). Whether `shape-margin` for initial letters exists today was not checked — a plausible latent "annotated but unimplemented" spec claim.
- **Research value:** One durable warning, worth one sentence in a methodology note: **AI patch batches produce "annotation-only patches that claim implementation"** — a `// +spec:` comment next to unchanged geometry reads as coverage in any grep-based audit. This is the same class of defect as memory's `azul-gates-with-wrong-premises`. Not enough to justify keeping the file.

---

#### scripts/RUN2.md

- **Verdict:** ARCHIVE — census + merge-order plan for the run2 patch corpus; corpus gone, downstream artifact shipped.
- **Was:** "Run 2 Patch Review — Verified Summary" (2026-03-05): 800 patches in `doc/target/skill_tree/all_patches/run2_patches/` split 373 CODE / 427 ANNOT across 16 CSS features; per-feature and per-file conflict tables (fc.rs 171 patches, text3/cache.rs 92, sizing.rs 83); a high-impact patch list with diffstats; **15 verified conflict clusters** each labelled PICK_ONE or MERGE; a 3-phase application order (ANNOT bulk first, then independent CODE, then clusters); and instructions for feeding the result into `review-arch`.
- **Landed:** The downstream artifact exists — `scripts/merge-groups.json` (group_id / action / patches / preferred / agent_context, with real SKIP verdicts such as *"This patch is a regression that replaces the comprehensive `get_display_type()` blockification logic … with a simpler version"*). The `+spec:` traceability layer landed at 1386 sites. **But the ID scheme in the tree is run3's hex form** (`+spec:block-formatting-context:ef493f`, 1346 matches) — RUN2's documented `+spec:feature-pNNN` form has only **2** matches, so the run2 ANNOT bulk was regenerated or dropped, not applied as described.
- **Superseded by:** `scripts/ARCH.md` (its architectural read) and the run3 trio (later, hex-ID batch that actually landed).
- **Still open:** none actionable. The patch corpus path `doc/target/skill_tree/all_patches/run2_patches/` is not in the tree (`doc/target/skill_tree/` has only `preview/`, `prompts/`, `tree.json`).
- **Research value:** Marginal — the "ANNOT-first, then independent CODE, then conflict clusters" ordering rule for bulk-applying machine-generated patches, and the observation that comment-only patches must be applied *first* because CODE patches cause context drift. One paragraph, not a file.

---

#### scripts/dump.md

- **Verdict:** RESEARCH — an unimplemented, still-wanted debugger feature: C callback pointer → source `file:line` → "Open in VS Code".
- **Was:** A raw LLM Q&A dump (last touched `159bb47e2`, 2026-05-23) in three parts: (1) resolving an arbitrary `extern "C" fn` pointer to `SourceLocation { file, line, symbol_name }` via `backtrace::resolve` — with a well-argued case for `backtrace` over raw `addr2line` (**ASLR base-address subtraction**, Windows PDB via `dbghelp`, inlined-frame iteration); (2) opening the result in an editor (`code -g file:line`, or the `vscode://file/{path}:{line}` URL scheme); (3) a hybrid fallback for binaries built without `-g` — symbol name survives even when DWARF doesn't, so **grep the project source for the function name**, with a "ripgrep-lite" implementation sketch over the `ignore` + `grep-searcher` crates and an `AZ_SRC` env var for the source root.
- **Landed:** Nothing of the feature. `backtrace` is an optional `azul-dll` dependency (`dll/Cargo.toml:31`, in the default feature list at `:617`) but is used **only for panic logging** — the sole use site is `dll/src/desktop/logging.rs:65 use backtrace::{Backtrace, BacktraceFrame}`. No `get_function_source_location`, no `symbol.filename()` call, no `vscode://`, no `AZ_SRC` (0 hits each). The `ignore` crate appears only in `doc/Cargo.toml:53` for an unrelated purpose. The debugger UI it was meant for exists (`dll/src/desktop/shell2/common/debugger/{debugger.html,debugger.js,debugger.css}`) but has no jump-to-source.
- **Superseded by:** n/a — nothing replaced it.
- **Still open:** the entire feature. The dependency (`backtrace`) is already in the default build, so the golden path is a small addition; the `-g`-less heuristic fallback and the `AZ_SRC` knob are the larger half.
- **Research value:** Two transferable pieces. **(i)** The `backtrace`-vs-`addr2line` argument (ASLR base-address handling, cross-platform PDB/DWARF, inline frames) — non-obvious and exactly the kind of thing re-litigated from scratch every time. **(ii)** The **hybrid symbol-resolution strategy**: DWARF gives file+line on the golden path; when the user compiled without `-g` the symbol table usually still has the *name*, so fall back to searching the project tree for it. That is a genuine "how do you make a native GUI debugger feel like a browser's DevTools" design idea, and pairs directly with ARCH_TODO.md's CDP-bridge chapter.

---

### Tally

| Verdict | Files |
|---|---|
| DELETE (4) | ARCH.md, REFACTOR.md, run3-arch.md, run3-refactoring.md |
| RESEARCH (5) | ARCHITECTURE.md, ARCH_TODO.md, SUPER_PLAN.md, SUPER_PLAN_2.md, dump.md |
| ACTIVE (2) | HIGHLEVEL_SUPERPLAN.md, CLEANUP_PLAN.md |
| ARCHIVE (3) | HIGHLEVEL_1_5_PLAN.md, run3-review.md, RUN2.md |

Secondary flags: `ARCHITECTURE.md` is also ACTIVE (stale symbol/path fixes). `SUPER_PLAN_2.md` is also ACTIVE (P6 `wacom_pad`) **and must not be deleted without first fixing the five `core/src/*.rs` doc-comments that cite it by section**.


## Part 02 — Component system / XML-DOM component model / widgets

All 12 files were added in a single tree-move commit `88b319b27` (2026-02-28) except
`WIDGETS_RELEASE_PLAN.md` (`b2e17c058`, 2026-06-23) and `WIDGETS_RESEARCH.md` (`0b5bf1e69`,
2026-06-23). None have been touched since.

**Big picture for this cluster:** the XML/component work is one of the most *completely
delivered* plan clusters in `scripts/`. The old `dyn Trait` XML component system is
verifiably gone (repo-wide grep for `XmlComponentTrait|XmlComponentMap|DynamicXmlComponent|
FilteredComponentArguments|html_component!|ComponentArgumentTypes` returns exactly **one**
hit, a comment in `tests/src/xml.rs:10`). The replacement (`ComponentDef`/`ComponentMap`/
`ComponentFieldType`/`ComponentDataModel`) lives in `core/src/xml.rs` (11 886 lines) and is
exported through `api.json` (`component` module, 30 classes). The GUI-builder debug API moved
from the deleted `debug_server.rs` (`80704c8fe`, 2026-05-29) into `layout/src/e2e/full.rs`
(~15 900 lines) — **every doc in this cluster still points at the dead path
`dll/src/desktop/shell2/common/debug_server.rs`**, which is the single biggest staleness in
the set.

---

#### scripts/COMPONENT_SYSTEM_REFACTORING.md

- **Verdict:** DELETE — every proposed removal verifiably done; superseded by XML_COMPONENT_REFACTORING_PLAN.
- **Was:** A side-by-side table of the "two parallel component systems": the old
  `XmlComponentTrait` / `XmlComponentMap` / `FilteredComponentArguments` (all non-FFI-safe:
  `Box<dyn>`, `BTreeMap<String,String>`, `Vec<(String,String)>`) versus the new `repr(C)`
  `ComponentDef` / `ComponentMap`. Diagnosed the real defect: `ComponentRenderFn` /
  `ComponentCompileFn` still took `&XmlComponentMap` + `&FilteredComponentArguments`, and the
  "fix" of replacing those args with opaque `usize` in api.json hid the problem. Proposed 7
  execution steps ending in ~-670 net LOC.
- **Landed:** Fully, and *better* than proposed. The doc proposed keeping
  `FilteredComponentArguments` but making it `StringPairVec`-backed; the code instead deleted
  it outright and routes everything through `ComponentDataModel`. Current signatures at
  `core/src/xml.rs:2380-2390`:
  `ComponentRenderFn = fn(&ComponentDef, &ComponentDataModel, &ComponentMap) -> ResultStyledDomRenderDomError`
  and `ComponentCompileFn = fn(&ComponentDef, &CompileTarget, &ComponentDataModel, usize) -> ResultStringCompileError`
  — no old-system type in either. `register_builtin_components()` at `core/src/xml.rs:4333`
  is now the single registration path (52 builtins via `builtin_component_def()` at `:3228`).
  The 52 `*Renderer` structs and `html_component!` are gone.
- **Superseded by:** `scripts/XML_COMPONENT_REFACTORING_PLAN.md` (same goal, cleaner
  `ComponentDataModel`-centric design) and `scripts/COMPONENT_TYPE_SYSTEM_DESIGN.md` §12.
- **Still open:** none. (`ComponentArguments` — singular-plural sibling of the removed types —
  survives at `core/src/xml.rs:165`, used only by the Rust `compile_component` path at
  `:5318`/`:5368`. That deliberate retention is already documented in the successor plan.)
- **Research value:** none beyond the successor docs.

---

#### scripts/COMPONENT_SYSTEM_REPORT.md

- **Verdict:** ARCHIVE — v2 investigation report, superseded by STATUS + TYPE_SYSTEM_DESIGN.
- **Was:** Dated 2025-02-21. The founding investigation: inventory of the XmlComponent system,
  the `repr(C)` callback pattern, CSS scoping, codegen, debugger + debug server; then a
  redesign proposing `AzComponentDef`, `AzCompileDomContext`, component libraries/collections,
  a `ComponentMap` replacing `XmlComponentMap`, debugger integration (component tab, "create
  component from DOM", grey internals, preview with app-state snapshots), project JSON
  persistence, per-language codegen, a 6-phase migration path, a files-to-modify table, and 4
  open questions.
- **Landed:** Most of it, but under different names and in a different file. `ComponentDef` (8
  fields, not the proposed `AzComponentDef` with `parameters`/`callback_slots`/`child_policy`/
  `template`/`example_xml`) at `core/src/xml.rs:2438`; `ComponentLibrary` at `:2492`;
  `ComponentMap` at `:2539`. Format specifiers (§2.4 `{var:spec}`) landed —
  `DynamicItem::Var { name, format_spec }` at `core/src/xml.rs:6109` with tests at `:9775`.
  Debugger: grey internals shipped (`debugger.js:665` `component-internal` class, styled at
  `debugger.css:285-296`), preview shipped (`get_component_preview` op →
  `layout/src/e2e/full.rs:15686` → `azul_layout::cpurender::render_component_preview` at
  `layout/src/cpurender/raster.rs:3224`), ZIP+base64 code export shipped
  (`full.rs:15297` builds `zip_entries`; `ExportedCodeResponse` at `:348`), import/export
  library at `full.rs:15354`/`:15441`. Phase 6 source-aware export partially landed:
  `source_file: Option<String>` at `full.rs:333`.
- **Superseded by:** `COMPONENT_SYSTEM_STATUS.md` (the checklist against this report) and
  `COMPONENT_TYPE_SYSTEM_DESIGN.md` (the actual delivered type system).
- **Still open:** `<For>`/`<If>`/`<Map>` structural components — **not implemented anywhere**
  (no `control:for` library, no such builtin in `register_builtin_components()`), so open
  questions #1 and #2 are still unanswered. `// USER CODE START/END` preservation markers and
  re-export change detection: not found. Shadow-boundary CSS scoping (Q3) and component
  versioning (Q4): unanswered.
- **Research value:** Low-moderate. The durable idea (component data model vs. compiled render
  fn) is stated far better in `WIDGET_JSON_FEASIBILITY_REPORT.md` §5.2.

---

#### scripts/COMPONENT_SYSTEM_STATUS.md

- **Verdict:** ACTIVE — the cluster's only real ❌ checklist, but ~6 rows are now stale-wrong.
- **Was:** Phase-by-phase ✅/❌ checklist of COMPONENT_SYSTEM_REPORT (Phases 1–6), plus §2 "new
  requirements from user feedback on the debugger screenshot" (library selector, mutability
  flag, two-column detail, structure+functionality editing modes, structured data models,
  CPU preview, visual-noise removal), §3 data-architecture changes, §4 an 8-step ordered
  implementation plan, §5 type-system summary, §6 a "nothing dropped" requirements checklist.
- **Landed:** Phase 1 ✅ verified (`ComponentDef` `core/src/xml.rs:2438`, `ComponentId` `:1196`,
  `ComponentDataField` `:1871`, `ComponentSource` `:2327`, `CompileTarget` `:2351`,
  `RegisterComponentFn` `:2403`, `ComponentLibrary` `:2492` with `modifiable`/`data_models`/
  `enum_models`, `ComponentMap` `:2539`). Phases 2–4 ✅ in `layout/src/e2e/full.rs`
  (`build_component_registry` `:9485`, `build_exported_code` `:9929`, CRUD at `:15553`
  `CreateComponent` / `:15598` `DeleteComponent` / `:15625` `UpdateComponent`). **Several ❌
  rows are now WRONG:** "grey rendering of component internals" is done
  (`debugger.js:665`, `debugger.css:285`); "drag-and-drop components into DOM tree" is done
  (`debugger.js:2220` `ondragstart` on component palette entries, drop zones at `:4128`,
  `:4198`, `:4228`); "ZIP packaging + base64 response" is done (`full.rs:2350`, `:15297`).
- **Superseded by:** partially by `COMPONENT_TYPE_SYSTEM_DESIGN.md`'s own status header, which
  points back here — a citation loop. Neither has been updated since Feb 2025.
- **Still open (verified today):**
  1. `For`/`If`/`Map` structural components + per-language iteration/conditional codegen —
     no trace in the codebase.
  2. Phase 6 source-aware export: `source_file` field exists (`full.rs:333`) but change
     detection and `USER CODE START/END` preservation do not.
  3. Component module structure in generated projects (`components/mod.rs`) — export still
     emits a flat file map.
  4. Enum-model / struct-model editors and a sidebar "Types" panel — no `add_enum_model` /
     `update_enum_model` / `delete_enum_model` op exists (grep = 0); `debugger.js:3755` only
     *consumes* `config.enumModel` in `FieldInput`.
  5. Every `debug_server.rs:NNNN` line reference in this file is dead (file deleted
     `80704c8fe`); the code is in `layout/src/e2e/full.rs`.
- **Research value:** none (pure checklist).

---

#### scripts/COMPONENT_TYPE_SYSTEM_DESIGN.md  (95 KB, 2 400 lines — sampled)

- **Verdict:** RESEARCH (secondary ACTIVE) — a full FFI-safe reflection/type system for a GUI
  builder, largely shipped.
- **Was:** The master design doc for `ComponentFieldType` — a `#[repr(C, u8)]` structured type
  descriptor (20 variants) replacing stringly-typed component metadata, so that one definition
  can drive: the browser debugger's type-appropriate editing widgets, multi-language code
  generation (Rust/C/C++/Python), JSON import/export, and live CPU preview. Sections: §3 core
  type + callback signature + enum model + typed defaults, §4 child-slot system (StyledDom
  fields as named slots), §5 callback advertising & merging `parameters`+`callback_slots` into
  one `data_model`, §6 enum-variant component states, §7 a PEG grammar for a type-definition
  string parser, §8 JSON serialization, §9 FFI/api.json considerations, §10 graph-based
  composition, §11–12 codegen + 6-phase migration, §14 a self-critique ("does this design make
  sense?") resolving 5 inconsistencies, §15 instance editing / data binding /
  dynamic→compiled pipeline, §16 component-local CSS as a data-bound template string with
  OS-specific preview.
- **Landed:** Most. Verified: `ComponentFieldType` `core/src/xml.rs:1432` (incl. `StructRef`
  `:1457`, `OptionType`/`VecType` via `ComponentFieldTypeBox` `:1277`);
  `ComponentCallbackSignature { return_type, args }` `:1267` with `ComponentCallbackArg`
  `:1231`; `ComponentEnumModel` `:1630` / `ComponentEnumVariant` `:1595`;
  `ComponentDefaultValue` `:1664` — **including the `Json(AzString)` variant the header calls
  missing**; `ComponentFieldValueSource {Default, Literal, Binding}` `:1749`;
  `ComponentFieldValue` (18 runtime variants) `:1763`; `ComponentInstanceDefault` `:1705`;
  `ComponentFieldOverride` `:1717`. Dead types the header wanted removed (`ComponentParam`,
  `ComponentCallbackSlot`, `ChildPolicy`) are gone — grep = 0. §8 structured JSON, marked
  "⚠️ Not done", **is now done**: `ComponentDataFieldInfo.field_type_structured:
  StructuredFieldType` at `layout/src/e2e/full.rs:448`, populated by `field_type_to_structured()`
  at `:9502`. Whole `component` module is in api.json (30 classes).
- **Superseded by:** n/a — it supersedes the other four component docs.
- **Still open:**
  - **§11 codegen still string-matches.** `map_type_to_rust(type_str: &str)` at
    `layout/src/e2e/full.rs:10347` takes a *string*, not the structured enum — the design's
    central payoff is unrealized on the codegen side.
  - **§7 `parse_field_type()`/`format_field_type()`** were never added to Rust; the debugger
    reimplements it client-side in JS (`debugger.js:4048 _parseFieldType`), and
    `field_type_to_string()` (`core/src/xml.rs:2017`) is still the serde `Serialize` impl
    (`:2006`) — so the flat string remains the wire default, structured is additive.
  - **§15.6 dynamic → compiled pipeline is a stub.** `UpdateComponentRenderFn`
    (`full.rs:15915`) literally comments `hot-replacement not yet supported` and only stores
    `render_fn_source`. Since the `template` field was removed by design,
    `user_defined_render_fn` (`core/src/xml.rs:2885`) renders a *field dump* (one div per
    default value) rather than the component — user-defined components have no real renderer.
  - §16 CSS-as-template with `{{field}}` bindings: `format_args_dynamic` exists
    (`core/src/xml.rs:6062-6091`) but is applied to XML attributes, not to `ComponentDef.css`.
- **Research value:** **HIGH — the keeper of this cluster.** A `#[repr(C)]` structural type
  descriptor with a matching runtime-value enum (`ComponentFieldType` ↔ `ComponentFieldValue`,
  "class vs. instance") is how azul gets a reflection layer across a C ABI with no runtime and
  no proc-macro derive — the thing XAML gets from .NET reflection, Qt from moc, Flutter from
  Dart mirrors/codegen. §14's self-critique and §4's "child slots are just `StyledDom`-typed
  fields" (so `ChildPolicy` is *derivable*, not stored) are both genuinely transferable.

---

#### scripts/PLAN_COMPONENT_HIERARCHY.md

- **Verdict:** DELETE — implemented essentially end-to-end, including both named JS bug fixes.
- **Was:** Analysis of DOM-node vs. component-invocation trees. Key proposal: stamp each
  component's rendered root with a `ComponentOrigin { component_id, data_model_values }` in
  `NodeDataExt`, expose it in the debug API, and render a second "Component Tree" view;
  plus `dataset` serialization, `accepts_text`/`child_policy`/`template` removal, and two
  concrete JS bugs (`scoped_css` vs `css` field-name mismatch; `field_type` arriving as a flat
  string but `FieldInput` dispatching on `ft.type`).
- **Landed:** All of it. `ComponentOrigin` at `core/src/dom.rs:1619`;
  `NodeDataExt.component_origin: Option<ComponentOrigin>` at `core/src/dom.rs:1931`; setter
  `set_component_origin` at `:2779`. `HierarchyNodeInfo.component: Option<ComponentOriginJson>`
  at `layout/src/e2e/full.rs:1031`. JS consumes it: component badge at
  `debugger.js:738-745`, node-detail row at `:817-819`, `isComponentRoot`/`insideComponent`
  grouping at `:661-665`. Both JS bugs fixed — `scoped_css` and `example_xml`/
  `component.template` are grep-0 in `debugger.js`, and `_parseFieldType` exists at
  `debugger.js:4048`. Phase-4 codegen: `ComponentCompileFn` now takes `&ComponentDataModel`
  (`core/src/xml.rs:2384`).
- **Superseded by:** n/a.
- **Still open:** the §7.2 "best-effort JSON extraction of `dataset`" was downgraded to the
  §7.3 fallback: `HierarchyNodeInfo.has_dataset: Option<bool>` (`full.rs:1036`) — a bool, not
  the JSON tree. Minor and deliberate.
- **Research value:** low-moderate. "Two trees" (physical DOM vs. logical component
  invocations, reconstructed from per-node origin stamps rather than retained instance
  objects) is the same problem React DevTools solves with a retained fiber tree; azul's
  stamp-the-root-and-infer-by-position approach is a cheaper alternative worth a paragraph
  if a research doc is written.

---

#### scripts/PLAN_DATA_MODELS_AND_API.md

- **Verdict:** DELETE — 7 of 8 phases verifiably shipped; the leftovers are already in STATUS.
- **Was:** "Plan 1 of 3" — the Rust/API half of the type-system rollout. 8 phases: define new
  types + `impl_vec!`/`impl_option!` wrappers + api.json entries; migrate
  `ComponentDataField.field_type` from string to enum; unify `parameters`+`callback_slots`
  into `data_model`; change `ComponentRenderFn` to take `ComponentFieldNamedValueVec`; update
  the debug-server API to structured `ComponentDataFieldInfo`; add `preview_component` with
  debouncing/incremental updates; remove the old system; add enum/struct model storage + CRUD
  endpoints. Includes a dependency graph, a testing strategy and an effort estimate.
- **Landed:** Phases 1–3 ✅ (types above; `ComponentDef` is 8 fields with a single
  `data_model`). Phase 4 ✅ in spirit — the render fn takes `&ComponentDataModel` rather than
  the proposed `ComponentFieldNamedValueVec`, but `ComponentFieldNamedValue` /
  `ComponentFieldNamedValueVec` exist (`core/src/xml.rs:1798`) and back
  `ComponentOrigin.data_model_values`. Phase 5 ✅ (`ComponentDataFieldInfo` with
  `field_type_structured`, `full.rs:442-455`). Phase 6 ✅ (`get_component_preview` op with
  `width/height/dpi/background/css_override/args/override_os/override_theme/override_lang` —
  see the op descriptor at `debugger.js:168` — served at `full.rs:15686`). Phase 7 ✅
  (old system grep = 1 comment). api.json `component` module has 30 classes.
- **Superseded by:** the delivered code; the residue is tracked in `COMPONENT_SYSTEM_STATUS.md`.
- **Still open:** Phase 8's enum/struct-model **API endpoints** — no `add_enum_model` /
  `add_data_model` / `add_component_field` op exists (grep = 0). Storage landed
  (`ComponentLibrary.enum_models`/`data_models`), management didn't. §6.5 incremental preview
  updates: preview is full-rerender.
- **Research value:** none — pure execution plan.

---

#### scripts/PLAN_JS_REUSABLE_COMPONENTS.md  (49 KB)

- **Verdict:** ACTIVE — 6 of 10 named widgets shipped; W4/W5/W9/W10 never built.
- **Was:** "Plan 2 of 3" — a widget library `app.widgets` for the browser debugger, to replace
  two inconsistent DOM-creation patterns and monolithic renderers. Specs 10 widgets: W1
  `FieldEditor`, W2 `TypeBadge`, W3 `FieldInput` (per-primitive controls), W4
  `ValueSourceToggle` (Default/Literal/Binding), W5 `BindingInput` (app-state path
  autocomplete), W6 `CssEditor`, W7 `PreviewPanel` (OS/theme switcher), W8 `DataModelEditor`,
  W9 `AddFieldDialog`, W10 `ComponentDragHandle`; plus helper utilities, a refactor of
  `showComponentDetail()`, new handlers, single-file organization ("Option A, recommended"),
  a 6-step migration and a "what NOT to build" list.
- **Landed:** `app.widgets` exists in `dll/src/desktop/shell2/common/debugger/debugger.js`,
  single file as recommended. Present: `TypeBadge:3448`, `FieldInput:3481`, `FieldEditor:3806`,
  `DataModelEditor:3832`, `CssEditor:3903`, `PreviewPanel:3954` — plus four widgets the plan
  never specified: `MiniHtmlTree:4112`, `ComponentPalette:4268`, `ContextMenu:4328`,
  `SourceEditor:4391`. Call sites at `:2317` (DataModelEditor), `:2371` (CssEditor), `:2512`
  (PreviewPanel), `:2552` (ComponentPalette).
- **Superseded by:** partially — `SourceEditor` + `ComponentPalette` replaced the planned
  template-editing and drag-handle stories once `ComponentDef.template` was dropped by design.
- **Still open (verified grep = 0 in `debugger.js`):** `ValueSourceToggle` (W4),
  `BindingInput` (W5), `AddFieldDialog` (W9), `ComponentDragHandle` (W10). W4/W5 matter most:
  `ComponentFieldValueSource::{Default, Literal, Binding}` exists in Rust
  (`core/src/xml.rs:1749`) with **no UI to set it** — the data-binding half of the GUI builder
  is backend-only. W9's absence is partly covered: `DataModelEditor` renders a
  `+ Add Field` button that fires `callbacks.onAddField` (`debugger.js:~3888`), so the dialog
  is the missing piece, not the entry point.
- **Research value:** low — implementation-level JS widget specs.

---

#### scripts/PLAN_UI_AND_INTERACTION.md  (38 KB)

- **Verdict:** ACTIVE — most views shipped; the Types/enum/struct editors and their flows did not.
- **Was:** "Plan 3 of 3" — the UX half. Inventories the 3 existing ActivityBar views, then
  specs: redesigned two-column Component Detail, enhanced sidebar component list, Add Field
  dialog, **Enum Model Editor**, **Struct Model Editor**, a sidebar **"Types" sub-panel**,
  Create Component / Create Library dialogs, Export/Import panel, 7 end-to-end interaction
  flows (create component from scratch → drag into a `StyledDom` slot → CSS templating →
  custom enum → export as Rust → OS-specific preview), a full new-CSS-class inventory, HTML
  changes, accessibility and responsive notes, and an implementation priority order.
- **Landed:** Create Library / Create Component shipped as ops + handlers
  (`create_library` op `debugger.js:163`, handler `:2228`; `create_component` op `:165`,
  handler `:2245`; server side `full.rs:15553`). Export/Import panel shipped
  (`export_code` `:161`/`:1628`, `importComponentLibrary` `:1532`). OS/theme preview flow
  shipped (Flow 7 — `get_component_preview` accepts `override_os` / `override_theme` /
  `override_lang`, `debugger.js:168`). Drag-into-slot (Flow 3) shipped —
  `ondragstart` on palette items `:2220`, drop zones `:4128`/`:4198`/`:4228`.
- **Superseded by:** n/a.
- **Still open:** Enum Model Editor, Struct Model Editor and the sidebar "Types" sub-panel are
  **absent** (no CRUD ops, no UI — the only enum awareness is read-only consumption at
  `debugger.js:3755`), which also kills Flow 5 ("create and use a custom enum"). Flow 4
  (CSS template expressions) is UI-only: `CssEditor` exists but `ComponentDef.css` is never
  run through `format_args_dynamic`. Create Component/Library use `prompt()`-style input, not
  the specced dialogs.
- **Research value:** low — UI spec, though the 7 interaction flows are a decent acceptance-test
  script if someone resumes the GUI builder.

---

#### scripts/XML_COMPONENT_REFACTORING_PLAN.md

- **Verdict:** DELETE — executed exactly, phase for phase.
- **Was:** The actionable successor to COMPONENT_SYSTEM_REFACTORING. Core simplification: *no
  validation layer* — JSON parsing (API path) or a flat attribute copy (XML path) IS the
  validation; both paths converge on `(def.render_fn)(&def, &data_model)`. 5 phases: add
  `xml_attrs_to_data_model()` + rewrite the XML render path; rewrite the compile path; delete
  ~1 500 lines of old system (13 enumerated items); update 8 external consumers; compile+fix.
  Explicitly decided to *keep* `ComponentArgument`/`ComponentArgumentVec` for dynamic string
  formatting.
- **Landed:** Completely. `xml_attrs_to_data_model()` at `core/src/xml.rs:4053` (with three
  dedicated tests at `:11536`, `:11556`, `:11575`); `xml_node_to_dom_fast()` at `:5776`;
  `render_dom_from_body_node_fast()` at `:5996`; `str_to_dom(root_nodes, component_map:
  &ComponentMap, max_width)` at `:5122`; `str_to_rust_code(root_nodes, imports,
  component_map: &ComponentMap)` at `:5221`. `tag_to_node_type()` at `:2596`,
  `builtin_data_model()` at `:3286`, `user_defined_render_fn()` at `:2885`. All 13 deletions
  confirmed by the repo-wide grep (1 residual comment).
- **Superseded by:** n/a.
- **Still open:** none. The only deviation is the deliberate one the doc itself sanctions —
  `ComponentArguments` (plural) survived alongside `ComponentArgument` because
  `compile_component` (`core/src/xml.rs:5368`) still needs `accepts_text` for the Rust
  function-signature emitter.
- **Research value:** low-moderate — "make the deserializer the validator" (delete the
  validation layer entirely because every entry point already has one) is a clean, quotable
  design ruling, but it's one paragraph.

---

#### scripts/WIDGET_JSON_FEASIBILITY_REPORT.md

- **Verdict:** RESEARCH — proves "data model is always simple, complexity is in the render fn."
- **Was:** A rigorous 17-widget audit answering: can every widget in `layout/src/widgets/` be
  described as pure JSON using `ComponentFieldType`, with callbacks as stubs? Answer: 13/17
  fully, 4 with caveats (TextInput = custom callback return type; ListView = `DomVec` cells +
  `Menu`; Ribbon = static not data-driven; NodeGraph = compiled rendering). A summary table
  with LOC/fields/callbacks/slots/enums/aux-structs per widget, a two-way gap analysis
  (7 things the type system handles, 4 minor extensions needed), and conclusions.
- **Landed:** The type-system prerequisites all exist —
  `ComponentCallbackSignature { return_type: AzString, args }` (`core/src/xml.rs:1267`) covers
  both §4.1 (custom return types) and §4.4 (0–4 extra args, `args` is a Vec);
  `ComponentFieldType::StructRef` (`:1457`) covers §4.3. **But the payoff never happened:**
  there is no widget `ComponentLibrary` — `register_builtin_components()`
  (`core/src/xml.rs:4333`) registers only the 52 HTML tags, and grep for
  `register_widget_components` / `widget_component_def` = 0. Instead the widgets went the
  *compiled* route: 48 files in `layout/src/widgets/` exported as 166 api.json classes.
- **Superseded by:** effectively by `WIDGETS_RESEARCH.md` + `WIDGETS_RELEASE_PLAN.md`, which
  chose "write more compiled Rust widgets" over "describe widgets as JSON".
- **Still open:** whether to ever register the widget set as a JSON `ComponentLibrary` so the
  GUI builder / debugger can inspect and codegen against widget data models. §5.4's three
  recommended additions (pre-registered core `StructRef` names for `Menu`/`LogicalPosition`/
  `LogicalSize`/`PixelValue`; documenting that `return_type` is not fixed to `Update`;
  template-expression support for computed CSS) are all unaddressed.
- **Research value:** **HIGH (second keeper).** §5.2 is the load-bearing insight, empirically
  demonstrated rather than asserted: NodeGraph is 3 764 lines but its data model is ~20 fields
  across ~15 structs — *the data model is always simple; the complexity lives in rendering and
  event handling*. That justifies azul's split between a JSON-definable interface and a
  compiled `render_fn`, and it is a direct argument against the Qt-Designer/XAML model where
  widget *behavior* is expected to be declarative. The per-widget table also doubles as a
  usable widget-complexity census.

---

#### scripts/WIDGETS_RELEASE_PLAN.md

- **Verdict:** ARCHIVE — a workstream log whose every row says DONE, all verified.
- **Was:** The 2026-06-20 "max effort" branch plan (`feat/widgets-and-demo-fixes`): W1 paint
  HiDPI click, W2 maps jumbled tiles, W3 widget-gap research, W4 build 24 new widgets, WX
  api.json export, W5 an `azul-widgets` showcase crate, W6 swap the release-page demo from
  `azul-spirit-level` to `azul-widgets` across 6 duplicated sync points. Contains a detailed
  W2 root-cause writeup and a resolved BLOCKER section on why the showcase couldn't compile.
- **Landed:** Everything, verified. 48 widget files in `layout/src/widgets/` (all 24 queue
  entries present: `switch.rs`, `divider.rs`, `card.rs`, `badge.rs`, `slider.rs`,
  `segmented.rs`, `radio_group.rs`, `tooltip.rs`, `text_area.rs`, `alert.rs`, `accordion.rs`,
  `avatar.rs`, `chip.rs`, `spinner.rs`, `popover.rs`, `combobox.rs`, `modal.rs`, `toast.rs`,
  `breadcrumb.rs`, `pagination.rs`, `stepper.rs`, `split_pane.rs`, `date_picker.rs`,
  `time_picker.rs`). api.json `widgets` module = 166 classes. `examples/azul-widgets/`
  exists (Cargo.toml + Dockerfile + src). Release swap done: `Cargo.toml:6` lists
  `examples/azul-widgets`, `azul-spirit-level` is gone from the members list;
  `doc/src/dllgen/deploy.rs:1595` has `("azul-widgets", "AzWidgets", "a showcase of all Azul
  widgets")` and `:1627` `ANDROID_READY` includes it.
- **Superseded by:** n/a.
- **Still open:** the two "Export wins" checkboxes are genuinely unticked — `Label`
  (`layout/src/widgets/label.rs:19`) and `TabContent` (`layout/src/widgets/tabs.rs:1384`)
  exist in Rust but are **not** in api.json's `widgets` module (the two `"Label"` hits at
  `api.json:18350`/`:20053` are unrelated enum members), so neither is reachable from C or
  any binding. Also still true: the widgets have never been runtime-verified — the plan's
  closing note ("user will report which widgets misbehave") was never answered.
- **Research value:** the W2 writeup has one transferable nugget: **fractional-zoom tile
  seams** — deriving each tile's size as `round(next_origin) − round(this_origin)` per axis
  instead of a fixed `round(tile_px)`, plus `f64` for global pixel coords past `2^24`. That
  belongs in the slippy-map notes, not here.

---

#### scripts/WIDGETS_RESEARCH.md

- **Verdict:** RESEARCH (secondary: 2 open export rows) — the "add a widget" recipe is the
  repo's only widget-authoring guide.
- **Was:** A 2026-06-20 gap list + build spec: what's already covered (do not rebuild), two
  cheap "export wins", a 24-widget prioritized build queue in 3 tiers with a `copy:` pointer
  to the existing widget whose pattern to mirror, and — most valuable — **THE RECIPE**: a
  5-step, file:line-referenced procedure for authoring an azul widget (DOM/style const-static
  `CssPropertyWithConditionsVec` slices; the 3-type split for stateful widgets
  `Widget`/`WidgetStateWrapper`/`WidgetState`; the `impl_widget_callback!` +
  `impl_managed_callback!` macro pair; builders with `set_/with_` pairs and
  `swap_with_default`; `.dom()` + the internal `extern "C"` handler that downcasts `RefAny`
  and patches live CSS via `info.set_css_property`; registration in `mod.rs` + api.json **via
  azul-doc autofix, never hand-edits**).
- **Landed:** The entire queue — all 24 widgets exist as files (see above) and all are
  exported. The recipe's anchors still resolve: `layout/src/widgets/mod.rs` is the registry,
  `core/src/host_invoker.rs` holds `impl_managed_callback!`, `check_box.rs` /
  `button.rs` / `number_input.rs` / `drop_down.rs` remain the canonical patterns.
- **Superseded by:** n/a — nothing else in the repo documents widget authoring.
- **Still open:** the two "Export wins" (`Label`, `TabContent` → api.json) — the same two
  rows left unticked in WIDGETS_RELEASE_PLAN. Nothing else.
- **Research value:** **MODERATE-HIGH as documentation, not as research.** It is not a
  comparative-toolkit argument, but it is the only written form of azul's widget-authoring
  contract, and the line-number anchors will rot. Best outcome is promotion into
  `doc/guide/` (a real "writing a widget" chapter) rather than `scripts/research/`; if that's
  out of scope, keep it. The genuinely transferable idea is the **3-type split** —
  `Widget` (styles) / `StateWrapper` (state + user callback) / `State` (plain data crossing
  the FFI boundary by value) — which is how a `repr(C)` toolkit gets React-style
  controlled-component semantics without closures or `dyn Trait`.


## Part 03 — CSS / cascade / spec-conformance planning docs

Audited 2026-08-01 against master @ `f1c43ba60`. All claims below were checked with
`rg`/`cargo test` against the live tree; nothing in the repo was modified.

**Baseline fact-check of the prompt's premise** — CONFIRMED:
`with_css` is `@scope`-like subtree CSS and the cascade retains author CSS on the cache.
- `core/src/styled_dom.rs:2346` `scope_inline_css()` walks the `Dom` pre-order and calls
  `CssPath::push_front_scope(start, start + estimated_total_children)` on every rule of every
  node's `.css`, so inline rules carry a `Root(CssScopeRange)` subtree marker.
- `css/src/css.rs:1562` `push_front_scope` picks **node-only** `[start,start]` for bare `*`
  wrapper rules (a plain `with_css("background: red")` decl) and the **full subtree**
  `[start,end]` when the rule has a real selector (`add_component_css`) — i.e. literally the
  `@scope` semantic.
- `core/src/style.rs:474` `Root(range) => range.contains(node_id.index())` is the matcher arm.
- `core/src/prop_cache.rs:718` `pub retained_author_css: Css`, written at
  `core/src/styled_dom.rs:1086` with the comment that dropping it broke runtime-inserted nodes
  (`e2e/bug-inserted-node-no-author-css.json`); re-read at `styled_dom.rs:1425/1450/1999`.

---

#### scripts/CSS_CACHE_OPTIMIZATION_PLAN.md

- **Verdict:** ACTIVE — phases 1/2/5/7 landed; 3, 4, 6 never implemented.
- **Was:** A 7-phase plan (dated 2025-02-19, committed 2026-02-19 `5f1171a1a`) to kill the
  ~4× duplication of every CSS property across `css_props` / `computed_values` /
  `resolved_cache` / `compact_cache`, and to replace per-node `BTreeMap`s with sorted `Vec`s.
  Carries a baseline benchmark table (git2pdf corpus, 77K-node `deserialize.rs` at 24.8 s) and
  a phase-by-phase memory/CPU budget (~92 MB → ~30 MB projected).
- **Landed:**
  - Phase 1 DONE — `core/src/prop_cache.rs:721`
    `pub user_overridden_properties: Vec<Vec<(CssPropertyType, CssProperty)>>` (no BTreeMap).
  - Phase 2 PARTIAL — `dependency_chains` is **gone** (zero hits repo-wide) and
    `computed_values` is de-BTreeMap'd (`prop_cache.rs:733`
    `Vec<Vec<(CssPropertyType, CssPropertyWithOrigin)>>`), but the field was *not* deleted as
    the plan wanted. `resolved_cache` as a field is **gone** entirely (only
    `invalidate_resolved_cache()` at `prop_cache.rs:3732` survives as a name).
  - Phase 5 DONE — sorted-vec + `binary_search_by_key` everywhere in the cascade path
    (`prop_cache.rs:956, 2131, 2342, 2370, 3520, 3591, 3613`), with an ordering-invariant test
    at `prop_cache.rs:5388`.
  - Phase 7 DONE, exactly as specified — `apply_ua_css` now builds a per-node
    `Vec<[u128; 2]>` "already set" bitset (`prop_cache.rs:3331-3371`); the plan's sketch used
    one `u128`, the code needed two because `CssPropertyType` has ~178 variants.
- **Superseded by:** the `FlatVecVec<T>` structure (`prop_cache.rs:367`) — a build/flatten
  two-phase container (`build: Vec<Vec<T>>` → `data: Vec<T>` + `offsets: Vec<(u32,u32)>`) now
  backs `css_props` and `cascaded_props` (`prop_cache.rs:726,730`). That is a *better* answer
  to the same "Vec-of-Vecs header overhead" problem than anything in the plan, and it is not
  mentioned in the document. `core/src/compact_cache_builder.rs` (named in §6) is now
  `core/src/compact.rs`.
- **Still open:**
  - Phase 3 `strip_normal_state_props()` — zero hits.
  - Phase 4 `CssPropertyType::is_compact_cached()` + skipping compact-cached props when
    building the resolved cache — zero hits.
  - Phase 6 `restyle_for_print()` — zero hits; PDF still runs the full 6-pseudo-state restyle.
  - `computed_values` still exists as a separate storage layer (Phase 2's stated goal was to
    delete it).
  - The benchmark numbers were taken on an external `git2pdf` corpus with a hardcoded
    `/Users/fschutt/...` path (§7.2) — unrunnable as written; no re-baseline exists.
- **Research value:** the §1.2/§1.4 per-node memory accounting method (measure the *same
  property stored N times in N encodings*, then pick one authority) is transferable; and the
  concrete finding that `apply_ua_css` scaled O(n^2.03) because of a nested
  `iter().any()` over property types is a reusable anti-pattern.

---

#### scripts/CSS_FEATURES.md

- **Verdict:** DELETE — user-facing guide, superseded by `doc/guide/en/styling*.md`.
- **Was:** A short end-user styling guide (`:lang()`, `@media screen/print`, pseudo-states,
  OS-specific and theme-specific styling via `CssPropertyWithConditions`, plus a quick-reference
  table of helper constructors and supported pseudo-classes). Committed 2026-02-28 as part of a
  pure file-move commit (`88b319b27`), never revised since.
- **Landed:** The CSS-side features are real: `CssPathPseudoSelector::Lang(AzString)`
  (`css/src/css.rs:1762`), `NthChild(CssNthChildSelector)` (`css/src/css.rs:1754`),
  `MediaType::{Screen,Print}` (`css/src/dynamic_selector.rs:767-768`, parsed at
  `dynamic_selector.rs:1851-1852`), and the helper constructors the table lists —
  `with_conditions` (`css/src/dynamic_selector.rs:1355`), `on_hover` (`:1363`),
  `dark_theme`/`light_theme` (`:1400`,`:1405`), `on_macos` (`:1415`) — all exist.
- **Superseded by:** `doc/guide/en/styling.md` (373 lines) and
  `doc/guide/en/styling/themes.md` (397 lines), which cover the same ground *and* the newer
  `@os` / `@theme` at-rules, `@media (prefers-reduced-motion)` / `(prefers-contrast)` /
  `(max-width)` / `(orientation)` viewport queries, and `system:accent` colors — none of which
  this file knows about.
- **Still open:** the Rust snippets here are **wrong API**: `Dom::with_inline_css_props(...)`
  does not exist (it is `with_css_props(CssPropertyWithConditionsVec)`,
  `core/src/dom.rs:2990` / `:6027`), and `with_normal_css_property` /
  `with_hover_css_property` / `with_active_css_property` / `with_focus_css_property` (lines
  125-128 of the doc) have **zero** definitions anywhere. If any of this is copied forward,
  fix those names first.
- **Research value:** none — pure how-to, and a stale one.

---

#### scripts/CSS_PROPERTY_CACHE_AUDIT.md

- **Verdict:** DELETE — a line-number census, fully drifted, mechanically regenerable.
- **Was:** An exhaustive 2026-02-17 inventory (`611c1251f` "Stash planning documents") of every
  `CssPropertyCache::get_*` method (claimed 113) with its line number, then every call site
  across `layout/src/solver3/{getters,fc,taffy_bridge,cache,display_list}.rs`, `core/src/*`
  and tests, ending in hot/cold summary tables and a "already in CompactCache? YES/NO" column
  per property.
- **Landed:** As a *snapshot* it was accurate; as a *reference* it is dead. Every line number
  is wrong: the doc puts `get_property` at 1454 and `get_property_slow` at 1475 — they are now
  at `core/src/prop_cache.rs:2028` and `:2102`. `core/src/compact_cache_builder.rs` (§2.14,
  ~60 rows) no longer exists; it is `core/src/compact.rs`.
- **Superseded by:** the getters are now **macro-generated** — `macro_rules! impl_get_prop`
  (`core/src/prop_cache.rs:1844`) with ~100 invocations from `:2441` onward; `rg -c "pub fn
  get_"` in `prop_cache.rs` returns **11**. The hand-typed method table can no longer be kept
  in sync even in principle, and `rg 'impl_get_prop!'` reproduces it in one second.
- **Still open:** the only non-mechanical content is the "Hot Path Properties … Already in
  CompactCache?" table, whose `NO` rows are still `NO` — notably `border-*-radius` (~12 hot
  call sites), `background`, `text-indent`, `table-layout`, `caption-side`, and the whole grid
  family (`gap`, `grid-template-*`, `grid-auto-flow`, `grid-column/row`) go through the slow
  path in `taffy_bridge.rs`. That's ~2 sentences worth of TODO, not a 649-line file.
- **Research value:** none beyond "generate the census, don't commit it".

---

#### scripts/CSS_ROOT_SCOPE_REFACTOR.md

- **Verdict:** DELETE — all 5 steps plus both follow-ups verified done in-tree.
- **Was:** The plan (and running log) for issue #47: `Dom::set_css("background: red")` on a
  non-root node painted the *whole window*, because `parse_inline` wraps declarations in
  `* { … }` and `restyle` routed any `[Global]`-only rule into `global_css_props`. Fix: add a
  `CssPathSelector::Root(CssScopeRange{start,end})` scope marker, push it onto the front of
  every inline rule at flatten time (when the flat NodeId is known), and add a range-test arm
  to the matcher. §5 deliberately *prepares but does not build* a parallel per-subtree cascade.
- **Landed:** Every checkbox verified:
  - Step 1 — `CssPathSelector::Root(CssScopeRange)` at `css/src/css.rs:1631`, struct at
    `:1601`, `Display` as `:root(s..=e)` at `:1707`, codegen arm at `css/src/codegen/rust.rs:351`.
  - Step 2 — `CssPath::push_front_scope` at `css/src/css.rs:1562`; tests at `css.rs:1884`,
    `:3011`, `:3026`, `:3037`, `:3048` (incl. a `usize::MAX` boundary case), `:3078`.
  - Step 3 — matcher arm `core/src/style.rs:474`; matcher tests `style.rs:1582`, `:2201`.
  - Step 4 — `scope_inline_css` at `core/src/styled_dom.rs:2346`, called from `:1207`;
    regression test `core/tests/css_scope_47.rs`. **I ran it: 2 passed, 0 failed.**
  - FOLLOW-UP B (FastDom/XML path) — **now DONE**, contrary to the doc's open checkbox:
    `create_from_fast_dom` computes `owner + hierarchy.subtree_len(...)` and calls
    `rule.path.push_front_scope(owner, end)` per rule (`core/src/styled_dom.rs:976-997`).
  - FOLLOW-UP A (bare `set_css` width/height not reaching layout) — **no longer reproducible**.
    I ran `cargo test -p azul-layout --test map_widget_fill`: 2 passed. That test lays out
    `Dom::create_div().with_css("flex-grow: 1; position: relative; …")` inside
    `with_css("display: flex; flex-direction: column; height: 100%;")` and asserts non-zero
    resolved geometry, i.e. bare-`with_css` layout-hot properties do reach the solver today.
  - A second semantic decision landed that the plan did not anticipate and is worth keeping:
    bare decls scope **node-only**, not subtree — see the test
    `parent_non_inherited_prop_does_not_leak_to_child` in `core/tests/css_scope_47.rs`, whose
    comment records that subtree matching made the red/blue boxes render white because the
    body's `background:white` covered everything.
- **Superseded by:** n/a (this doc *is* the shipped design; `58bcce130`, `6f5df0569`,
  `438d8d46a`).
- **Still open:** only the two explicitly-deferred optimizations, neither of which is
  scheduled: (a) §4.4's note that a `[Root(n)]` rule matches exactly one node and could be
  applied directly instead of scanned against all nodes; (b) §5's parallel cascade — there is
  **no** rayon/parallel/fan-out marker anywhere in `core/src/prop_cache.rs` or
  `core/src/compact.rs`, so even the "leave a comment marking the seam" step was not done.
- **Research value:** §5 is the keeper. The observation is that the flat arena lays every
  subtree out **contiguously**, so once each author rule is tagged with its owner's range, the
  tree partitions into disjoint rule-slices `[a,b)` that can be cascaded independently —
  scoping and arena contiguity together are what make a parallel cascade possible. That's a
  transferable argument about why `@scope`-style scoping is a *performance* feature, not just
  an encapsulation one, and it is worth extracting to `scripts/research/` before the rest goes.

---

#### scripts/CSS_STYLESHEET_COLLAPSE_PLAN.md

- **Verdict:** DELETE — implemented verbatim in `679b91513`.
- **Was:** Proposal (2026-05-08) to delete the `Css → Vec<Stylesheet> → Vec<CssRuleBlock>`
  triple indirection. It first *audits* the wrapper's stated purpose (layer-aware specificity
  sorting, per `css/src/css.rs:25-30`) and shows it is unrealised — every producer emits
  exactly one `Stylesheet`, and the merge sites flatten, actively erasing any boundary. It then
  recovers the capability as an explicit `priority: u8` on `CssRuleBlock` with named slots and
  deliberate numeric gaps, forward-compatible with `@layer`.
- **Landed:** `pub struct Css { pub rules: CssRuleBlockVec }` at `css/src/css.rs:30-34`, with
  the doc-comment "Sort by `(priority, specificity)` via `sort_by_specificity`". The
  `rule_priority` module is at `css/src/css.rs:605-625` with exactly the proposed constants
  `UA=0 / SYSTEM=10 / AUTHOR=20 / INLINE=30` (plus `RUNTIME`). `Stylesheet` / `StylesheetVec`
  are gone — the only surviving hits for "stylesheet" in `css/src` and `core/src` are prose in
  doc-comments and test names.
- **Superseded by:** n/a.
- **Still open:** `@layer` parsing itself was never added (the plan only claims
  forward-compatibility); `rule_priority::RUNTIME` is reserved-but-unused, and runtime
  overrides still go through `user_overridden_properties` as the doc predicted. The §5
  `!important` note (encode on `CssDeclaration`, never on `priority`) remains the design.
- **Research value:** moderate — §2/§3 is a clean worked example of "audit whether an
  abstraction's stated purpose is actually exercised before deleting it", and §5.4's five
  reasons to prefer a numeric layer field over a boundary type (cheap / explicit / mergeable /
  fine-grained / `@layer`-compatible) is a reusable argument about CSS cascade layering.

---

#### scripts/DEFERRED_CASCADE_DESIGN.md

- **Verdict:** DELETE — the architecture shipped; `Dom` is recursive and the callback returns it.
- **Was:** Design doc arguing the layout callback should return `Dom`, not `StyledDom`.
  Diagnosis: each component called `dom.style(css)` → a full 5-pass `StyledDom::create()`, then
  `append_child()` merged arrays *without* re-running inheritance or rebuilding the compact
  cache — so cross-component inheritance was silently broken and Tier 1/2/2b entries were stale.
  Proposal: `Dom { root, children, css: Vec<Css> }`, `.style()` just pushes, `append` is a
  `Vec::push`, and one cascade runs after composition. Includes a triage of the `origin/perf-fixes`
  branch (keep Fix 3 + Fix 5, drop Fix 4A/4B/fingerprint as superseded) and a §5.4 honest
  re-assessment concluding the win is *correctness*, not throughput.
- **Landed:** All of it.
  - `pub struct Dom { root: NodeData, children: DomVec, css: azul_css::css::CssVec, … }` at
    `core/src/dom.rs:3456-3464`, with the field doc "Stylesheets are applied in push order
    during the single deferred cascade pass. Later entries override earlier ones."
  - `pub type LayoutCallbackType = extern "C" fn(RefAny, LayoutCallbackInfo) -> Dom` at
    `core/src/callbacks.rs:113`.
  - §7.4 scoping semantics implemented: `collect_css_from_dom`
    (`core/src/styled_dom.rs:2366`) recurses children *first* so inner/component CSS lands
    before outer/root CSS — "outer CSS has higher cascade priority", exactly rule 3.
  - The "Drop" recommendations were honoured: `CompactInlineProps`, `InlineStyleTable` and
    `tier3_overflow` have **zero** hits repo-wide.
- **Superseded by:** partly by `scripts/CSS_ROOT_SCOPE_REFACTOR.md` — this doc's §7.4 left the
  scoping mechanism as "maintain a CSS stack during flatten"; what actually shipped is the
  cheaper `Root(CssScopeRange)` selector marker (no stack, survives the specificity sort for
  free). The `#47` doc is the authority on scoping semantics now.
- **Still open:** §1.1's `ComponentOrigin`-on-`NodeDataExt` component-tree story and the
  VirtualView/component distinction are documentation-only claims not re-verified here. §8's
  risk "NodeDataFingerprint needs to include css[] hash" is worth a spot-check by whoever owns
  reconciliation.
- **Research value:** high, and the most *conceptually* durable of the seven. Two transferable
  ideas: (1) "a component is a CSS scoping boundary with **asymmetric** leakage — parent CSS
  cascades in, child CSS cannot leak out", which is the same call React/Vue scoped styles and
  CSS `@scope` make; (2) §5.4's discipline of re-scoring a refactor's claimed benefits
  *after* analysis and finding most of them are correctness, not perf. Worth extracting §1.1 +
  §3.3 + §5.4 to `scripts/research/`.

---

#### scripts/SPEC_CONFORMANCE_REVIEW.md

- **Verdict:** ACTIVE — live CSS-spec defect list; a verified sample is still unfixed today.
- **Was:** 255 KB / 889 (very long) lines, generated by `autotest_fleet.sh css-review`
  (model=fable), last touched 2026-07-25 `f1649a65b`. **Structure:** a two-line preamble, a
  tooling note, then one `## <source file>` section per audited file (26 sections, from
  `core/src/dom.rs` through `layout/src/text3/knuth_plass.rs`), each holding bullets keyed to a
  `+spec:<topic>:<hash>` annotation in the source. Each bullet is a verdict
  (`CORRECT`/`PARTIAL`/`MISSING`/`INCORRECT`, written either as a bold prefix or as
  `STATUS(...)` — the two spellings are the two generation passes) followed by a dense
  paragraph quoting the spec text and citing `path:line` for the deviation.
  **Verdict distribution:** `STATUS(...)` form — 95 CORRECT / 37 PARTIAL / 6 MISSING /
  2 INCORRECT; bold form — 49 CORRECT / 45 PARTIAL / 10 MISSING / 5 INCORRECT.
  Roughly **105 non-conformant items** total.
- **Landed:** It is a review, not a plan, so "landed" = "are the flagged defects still real".
  I verified 12 (below); **all 12 still reproduce**. The doc's own tooling complaint also still
  holds: `azul-doc spec` accepts only `status` / `paragraphs` / `annotations`
  (`doc/src/spec/mod.rs:129-137`) — there is still no `spec show <prop>:<hash>` subcommand
  (`doc/src/main.rs:1556`, help text at `:2410`). The repo now carries **1385** `+spec:`
  annotations across 32 files (up from the 1237 the doc reports), so the hash-resolution drift
  has widened, not closed.
- **Superseded by:** n/a — nothing else in the repo enumerates spec deviations at this
  granularity. Its companion is `doc/reftest_baseline.txt` (the Chrome-reftest regression
  gate), which measures the *symptoms* this document explains.
- **Still open:** see the verified list below. In addition, the whole `+spec:` hash-resolution
  pipeline is broken (paragraph extraction drifted from the annotations committed in
  `5c3b9a17a`), which means none of these items can currently be looked up by hash — that is
  itself a blocking tooling bug for anyone trying to work the list.
- **Research value:** the *method* is the keeper: annotate source with
  `+spec:<topic>:<content-hash-of-the-spec-paragraph>`, then have a model audit each annotation
  against the downloaded spec text and emit CORRECT/PARTIAL/MISSING/INCORRECT with a
  `path:line` citation. That produces a conformance ledger that survives refactors better than
  a test list. The failure mode is also instructive and worth recording: the hashes went stale
  because paragraph extraction was not pinned, so the ledger's own index rotted.

##### Still-unimplemented / non-conformant items (verified against the tree today)

Twelve sampled; every one still reproduces. `path:line` is what I found *now*, not what the
doc says.

| # | Item | Doc verdict | Verified-now evidence |
|---|------|-------------|-----------------------|
| 1 | `overflow-block` / `overflow-inline` are parsed and have getters but **nothing consumes them**; no mapping onto `overflow-x/y` | MISSING `overflow:17654b` | Only definitions exist: `layout/src/solver3/getters.rs:1267-1275`, `core/src/prop_cache.rs:2741-2742`. Zero consumer call sites in `layout/src`. |
| 2 | `unicode-bidi: embed` / `bidi-override` never inject LRE/RLE/PDF control codes | MISSING `writing-modes:3e2632` | No `LRE`/`RLE`/`U+202A` handling anywhere in `layout/src/text3/cache.rs`. |
| 3 | `alignment-baseline` is stored and has a getter but is **never read** — no per-value baseline selection | MISSING `font-metrics:fa4489`, `writing-modes:cc8e70` | `get_alignment_baseline` generated at `layout/src/solver3/getters.rs:1395-1398`; zero callers outside `getters.rs`. |
| 4 | `scroll-behavior` CSS property is never consulted; root-element→viewport propagation absent | MISSING `containing-block:03528c` | `resolve_scroll_behavior` is a **`const fn`** at `layout/src/managers/scroll_into_view.rs:436` — it structurally cannot read the property cache. |
| 5 | `text-box-edge` cannot express the spec's two-value over/under grammar | INCORRECT `writing-modes:daad86` | `pub enum StyleTextBoxEdge { Auto, TextEdge, CapHeight, ExHeight }`, `css/src/props/style/text.rs:2720-2731` — still a single keyword. |
| 6 | `display` is a flat single-keyword enum, not the css-display-3 (outer, inner) pair | PARTIAL `display-property:cf1820` | `pub enum LayoutDisplay { None, Block, Inline, … }`, `css/src/props/layout/display.rs:13`. |
| 7 | `overflow: hidden` is not treated as a *scrollable* value (css-overflow-3 §3.1) | PARTIAL `overflow:44ef3b` | `layout/src/solver3/cache.rs:2202-2203` matches only `Exact(Scroll \| Auto)`. |
| 8 | Orthogonal-flow auto inline size unimplemented; auto width always resolves against the CB width | MISSING `width-calculation:472065` | Self-admitted in code at `layout/src/solver3/sizing.rs:1547-1548`. |
| 9 | Orthogonal-flow child block size not fed into a shrink-to-fit parent | MISSING `table-layout:93b13c` | `layout/src/solver3/sizing.rs:1513` — "orthogonal flows would require child block size as input (not yet implemented)". |
| 10 | `text-box-trim` is applied only to the IFC-root block container, never to an **inline box's** content box | MISSING `display-property:db5125`, `dceb24` | Sole consumer is `layout/src/solver3/fc.rs:3019-3061` (block-container path). |
| 11 | `word-break` / `line-break` / `overflow-wrap` do not reach Knuth-Plass break opportunities | MISSING `line-breaking:16e64c` | `fn convert_items_to_nodes(items, hyphenator, fonts)` — `layout/src/text3/knuth_plass.rs:107-111` takes no `constraints` argument at all. |
| 12 | `FormattingContext::Float(..)` / `OutOfFlow(..)` are dead variants — out-of-flow boxes go through blockification instead | PARTIAL `display-property:844893` | Constructed **only** in `tests/src/layout.rs:1473,1489,1524,1540,1615,1637,1691,1695`, a file that is not compiled (`tests/src/lib.rs` includes `layout-test.rs`). Also confirmed: `determine_formatting_context_for_display` returns `FormattingContext::Inline` for `display:block` whenever `has_only_inline_children` is true **without** consulting the BFC predicate — `layout/src/solver3/layout_tree.rs:3340-3352` — so `<div style="overflow:hidden">text</div>` loses its BFC while `display:flow-root` with identical content keeps it (`layout_tree.rs:3337-3339`). |

Additional flagged-and-unverified items worth carrying forward (not spot-checked, but no
counter-evidence found): `text-align: left/right` collapsed to `Start`/`End` instead of
line-relative (`text-alignment-spacing:43ea0a`); ligature suppression under non-zero
letter-spacing / justification not implemented (`text-alignment-spacing:4357e6` —
`build_feature_mask_for_script` at `layout/src/text3/default.rs:415` always starts from
`FeatureMask::default_mask()` which includes `CLIG`/`LIGA`/`RLIG`); hyphenation applied
regardless of cluster direction (`display-property:508895`); margin/padding percentages not
resolved against the containing block's *inline* size in vertical writing modes
(`box-model:66e123`); replaced-element constraint-violation table gated only on
`has_intrinsic_ratio` so it fires with explicit sizes (`width-calculation:ef71c4`); and
`width:auto`/`height:auto` resolution keyed to physical rather than logical axes
(`block-formatting-context:c6fb58`).


## Part 04 — Layout engine / incremental layout / data-structure & memory optimization

Audit date 2026-08-01. All verification done with `rg` against the working tree at `master` (f1c43ba60).

---

#### scripts/INCREMENTAL_LAYOUT_ARCHITECTURE.md

- **Verdict:** DELETE — all five phases landed; file paths in it are already stale.
- **Was:** A ~40 KB plan (commit `1ec9d50c2`, 2026-02-20) to connect the *already existing but disconnected* change-detection infrastructure (`RelayoutScope`, `RestyleResult`, `DirtyFlag`, `mark_dirty`) to the layout pipeline. Proposed a new `ChangeAccumulator` / `NodeChangeReport` / `TextChange` / `NodeDataFingerprint` in `core/src/diff.rs`, a `paint_dirty` set on `ReconciliationResult`, a new `ProcessEventResult::ShouldIncrementalRelayout` level, activation of three "dead fields" (`words_changed`, `css_properties_changed`, `images_changed`), and compositor damage rects. Included a per-phase file/change table.
- **Landed:** Nearly everything, verbatim in naming.
  - `core/src/diff.rs:1191` `TextChange`, `:1200` `NodeChangeReport`, `:1247` `ChangeAccumulator.per_node: BTreeMap<NodeId, NodeChangeReport>`, `:1276` `needs_layout()`, `:1281` `needs_paint_only()`, `:1591` `NodeDataFingerprint` (+ tests at `:3969`).
  - `layout/src/solver3/cache.rs:487` `pub paint_dirty: BTreeSet<usize>`; `:495`/`:505` the `is_clean()`/`needs_paint_only()` predicates; written at `:1416`.
  - `ProcessEventResult::ShouldIncrementalRelayout` is handled on **every** shell: `dll/src/desktop/shell2/{macos,windows,headless,linux/wayland,linux/x11}/mod.rs` + `common/event.rs`.
  - `incremental_relayout()` lives at `dll/src/desktop/shell2/common/layout.rs` (called e.g. `linux/wayland/mod.rs:2475`, `:2703`).
  - `ChangeAccumulator` is actually fed from the restyle path: `dll/src/desktop/shell2/common/event.rs:458-531` ("Feed `RestyleResult` through `ChangeAccumulator` for granular classification").
  - Dedicated test suite exists: `core/tests/reconciliation/{change_accumulator,css_scope,fingerprint,node_change_set,text_reconciliation,dom_reconciliation}.rs` (39 `RelayoutScope` refs in `change_accumulator.rs` alone).
  - Phase 5 damage tracking landed too, but by a **different route** (see below).
- **Superseded by:** (a) Damage rects were NOT built as `ChangeAccumulator.damage_rects` / `compute_damage_rects()` — `rg damage core/src/diff.rs` returns **nothing**. Instead damage is computed from a *display-list diff* in `layout/src/solver3/display_list.rs:338-432` (`compute_text_damage_rect`, image-swap damage at `:392`), commit `4f2844d1e` "feat(display_list): text glyph patching and damage rect computation", and consumed by `dll/src/desktop/shell2/{wayland,x11,macos,headless}` + `layout/src/cpurender/{raster,compositor}.rs`. Diffing the produced display list is more robust than accumulating damage from CSS-change classification, because it also catches damage the classifier can't see (glyph reflow inside an unchanged box). (b) The doc's `*_v2.rs` filenames (`layout_v2.rs`, `event_v2.rs`) no longer exist — those are `common/layout.rs` and `common/event.rs` now, so every link in the doc is dead.
- **Still open:** `compute_text_edit_range()` (Phase 4 item in `core/src/diff.rs`) does not exist — incremental IFC reshape re-shapes the whole IFC rather than a byte range. `words_changed` / `css_properties_changed` appear only once each in `dll/src/desktop/shell2/common/layout.rs:865` and that hit is a **doc comment**, so it is worth re-confirming they are genuinely consumed and not merely documented.
- **Research value:** The "two parallel systems that don't talk" framing — a per-property `RelayoutScope` lattice (`None < IfcOnly < SizingOnly < Full`) collapsed into a 3-level `DirtyFlag`, with an explicit mapping table and stated correctness guarantees (fallback-to-`Full` on fingerprint miss, IFC-height cascade upgrade, debug-mode cross-check of the fast classifier against the slow one). That mapping table + the "hash as verification in debug builds" trick is transferable. Moderate value; the code now documents most of it.

---

#### scripts/PERCENTAGE_LAYOUT_ANALYSIS.md

- **Verdict:** DELETE — every finding fixed; premises about the solver are now wrong.
- **Was:** A short (6 KB, 2026-01-16) report claiming percentages work in `taffy_bridge.rs` but are handled redundantly in `sizing.rs`, that `ProgressBar` uses a `flex-grow: 10_000_000` hack instead of `width: N%`, and that `vw/vh/vmin/vmax` are unsupported. Proposed 3 phases: fix ProgressBar, clean up `sizing.rs`, add viewport units.
- **Landed:** Phase 1 done — `layout/src/widgets/progressbar.rs:474` and `:607` now emit `PixelValue::percent(percent_done)` / `PixelValue::percent(100.0 - percent_done)`; no `10000000.0` constant survives anywhere in the tree. Phase 2 done — `rg "SizeMetric::Percent" layout/src/solver3/sizing.rs` returns **zero hits**, so the redundant `Percent => None` arms are gone. Phase 3 done — viewport units are fully resolved: `css/src/props/basic/pixel.rs:531-548` resolves `Vw`/`Vh`/`Vmin`/`Vmax` against `context.viewport_size` (with a `0.0` fallback at `:484` when no context is available), and the parser handles them at `:801-804` including the documented "vmin before in" ordering trap.
- **Superseded by:** n/a
- **Still open:** none. Taffy 0.10 is still the flexbox/grid backend (`layout/Cargo.toml:69`), so the doc's architectural premise holds — it just has nothing left to fix.
- **Research value:** none (one durable trivia: `"vmin"` must be matched before `"in"` in the unit-suffix table, and that lesson is already a code comment at `pixel.rs:802`).

---

#### scripts/NODEDATA_OPTIMIZATION_PLAN.md

- **Verdict:** DELETE — all 5 steps landed and the code went further than the plan.
- **Was:** (2026-02-28) A byte-offset-level plan to shrink `NodeData` from 320 B to ~176-184 B in five steps: (1) fold `run_destructor: bool` into the C-ABI `Destructor` enum as an `AlreadyDestroyed` variant, shrinking every `AzXxxVec` from 48 B to 40 B; (2) move the `IFrame` payload out of `NodeType` into `NodeDataExt`; (3) delete `IdOrClassVec`, folding ids/classes into `AttributeTypeVec`; (4) move `dataset` to `NodeDataExt`; (5) pack `OptionTabIndex` + `contenteditable` into a `NodeFlags(u32)`. Explicitly weighed the "move `attributes` to ext too" variant and *recommended against it* (most nodes have a class → many small ext allocations).
- **Landed:** All five, and the rejected alternative won anyway.
  - Step 1: `css/src/macros.rs:110` `AlreadyDestroyed` variant; `run_destructor` is gone from the core Vec macro (surviving hits are unrelated FFI shims in `dll/src/unified/*` and `dll/src/desktop/extra/*`). `doc/src/codegen/v2/lang_c.rs:1327` even carries the migration note "run_destructor field has been removed, destructor enum now uses AlreadyDestroyed variant".
  - Step 2 (superseded shape): `NodeType::IFrame` **no longer exists at all** — `rg IFrame core/src/dom.rs` is empty. It became `NodeType::VirtualView` (`core/src/dom.rs:639`) with the payload in `NodeDataExt.virtual_view: Option<VirtualViewNode>` (`:1910`).
  - Step 3: `NodeDataExt.attributes` at `core/src/dom.rs:1908`, doc comment: "IDs and classes are stored as `AttributeType::Id` and `AttributeType::Class` entries."
  - Step 4: `NodeDataExt.dataset: Option<RefAny>` at `:1912`.
  - Step 5: `NodeFlags { inner: u32 }` at `:2120`, with `CONTENTEDITABLE_BIT`/`TAB_INDEX_MASK`/`ANONYMOUS_BIT` at `:2126-2129` — note it absorbed `is_anonymous` too, which the plan hadn't budgeted.
- **Superseded by:** The current `NodeData` (`core/src/dom.rs:1538-1562`) is *smaller* than any variant the plan considered: `node_type`, `callbacks`, `style: azul_css::css::Css`, `flags: NodeFlags`, `accessibility: Option<Box<..>>`, `extra: Option<Box<NodeDataExt>>`. Two changes the plan never anticipated: (a) `attributes` moved into ext after all — the comment at `:1907` says "Moved from `NodeData` to save 48B for the ~95% of nodes with no attributes", i.e. the plan's own "most styled nodes DO have at least one class" assumption was measured wrong; (b) the per-property `css_props: CssPropertyWithConditionsVec` was replaced wholesale by a single `style: Css` carrying conditioned rules (`:1551`), which is a cascade-model change, not a size change.
- **Still open:** No `size_of::<NodeData>()` assertion exists anywhere — the 320→~144 B claim is unverified in CI, so a regression would be silent. Adding a `const _: () = assert!(size_of::<NodeData>() <= N);` would be a cheap, durable guard.
- **Research value:** Two transferable techniques, both now proven in-tree: (1) **encoding a boolean guard as an enum variant** (`run_destructor: bool` → `Destructor::AlreadyDestroyed`) to reclaim a whole alignment slot in a `repr(C)` FFI type without losing the double-free guard; (2) the discipline of writing the offset/size/align table *before* and *after*, which is what exposed that the padding after `contenteditable` was free real estate. The `assert` gap above is the counter-lesson.

---

#### scripts/COMPACT_CACHE_PLAN.md

- **Verdict:** RESEARCH — the durable data-oriented-design argument behind a shipped subsystem.
- **Was:** (2026-02-17, commit `611c1251f`) The design doc for replacing BTreeMap CSS property lookups with a three-tier compact cache. Contains a real flamegraph baseline (64,429 samples; `BTreeMap::get` = 40.8%, cascade walk 6.2%, getter wrappers 10.1%, `FontFamilyVec` clone/drop 2.6%, BTreeMap allocs 7.8% → ~67.5% "eliminatable"), a per-property access-frequency histogram from grepping `layout/src/`, an L1/L2/L3 byte budget per DOM size, the Tier-1 u64 bit-layout, MSB-sentinel encodings for u16/i16/u32, and the superlinear-scaling argument (BTreeMap nodes scatter across pages as the DOM grows, so cost/node rises; a linear `Vec` sweep is prefetchable so it doesn't).
- **Landed:** Yes, as `css/src/compact_cache.rs` (4180 lines) + builder `core/src/compact.rs` (`build_compact_cache` at `:45`, `build_compact_cache_with_inheritance` at `:447`).
  - `CompactLayoutCache` at `css/src/compact_cache.rs:1435` with `tier1_enums: Vec<u64>`, `tier2_dims: Vec<CompactNodeProps>`, `tier2_cold: Vec<CompactNodePropsCold>`, `tier2b_text: Vec<CompactTextProps>`.
  - `CompactNodeProps` (`:1162`) matches the plan's field list essentially 1:1 (8× u32 unit-carrying dims, 16× i16 resolved-px, 2× u16 flex) — plus `row_gap`/`column_gap` which the plan lacked.
  - Encoders/decoders exactly as specified: `encode_pixel_value_u32` (`:1041`), `encode_resolved_px_i16`/`decode_resolved_px_i16` (`:1081`/`:1091`), `encode_flex_u16`/`decode_flex_u16` (`:1101`/`:1111`).
  - Owned by `CssPropertyCache.compact_cache: Option<CompactLayoutCache>` (`core/src/prop_cache.rs:738`), with a size-accounting field `compact_cache_bytes` at `:776`.
  - Consumed by `layout/src/solver3/getters.rs` (188 `compact` references, 21 `compact = ` macro variants) and by `taffy_bridge.rs` (57 `compact` references).
- **Superseded by:** Partially, by the code itself.
  - **Tier 3 was deleted, not implemented.** `rg tier3` over the tree returns **nothing**. Out-of-range values simply hit the sentinel and fall through to `get_property_slow()`. The plan's `Vec<Option<Box<FxHashMap<..>>>>` overflow tier is dead — this was already flagged as "0% utilized" in STATUS_V2 §6.4 and the fix chosen was removal.
  - **A hot/cold split of Tier 2 replaced the plan's single struct**: `CompactNodePropsCold` (`:1204`) holds border colors/radii/styles, z-index, grid line numbers, border-spacing, tab-size — paint-only fields the layout loop never touches.
  - **Negative caching was invented after the plan** and is the more interesting idea: `hot_flags: u8` + `extra_flags: u8` per node (`has_transform`, `has_box_shadow`, `has_background`, `has_any_scrollbar_css`, `has_counter`, …) let a getter *prove the default applies* and skip the cascade walk entirely, and `dom_declared_flags: u32` on the cache does the same at whole-DOM granularity ("if no node anywhere declares `letter-spacing`, never walk for it"). That converts "slow path is rare" into "slow path is provably unnecessary" — the plan never contemplated it.
  - **Per-node font dirty tracking** also postdates the plan: `font_dirty_nodes` + `prev_font_hashes` + `font_hash_to_families` replaced a "collision-prone global XOR `font_stacks_hash`".
- **Still open:** The plan's headline predictions (3-5× overall, 6-10× on large DOMs, deserialize.rs 60s→6-10s) were **never measured** — STATUS_V2 §9 item 11 was "Benchmark" and no benchmark result exists in the tree. Also `set_css_property()` still does not patch the compact cache in O(1) (STATUS_V2 §9 item 10); the whole cache is rebuilt via `recompute_inheritance_and_compact_cache()` (`core/src/styled_dom.rs:437`), and there is a live footgun documented at `styled_dom.rs:397-399`: a recompute path that silently drops to the getters-only `build_compact_cache` produces wrong results from frame ≥2.
- **Research value:** **High — this is the keeper of the cluster.** It is a complete, self-contained data-oriented-design case study for a retained-mode UI toolkit: profile → property-frequency histogram → tier assignment by access phase → cache-size budget per DOM size → bit-packing and MSB-sentinel encoding → branch-predictability argument for the sentinel check. Explicitly contrasts against BTreeMap pointer-chasing and against a flat SoA `ComputedPropertyStore`. Belongs in `scripts/research/` with a short header noting that Tier 3 was dropped and that hot/cold + negative-caching flags were added later.

---

#### scripts/COMPACT_CACHE_STATUS.md

- **Verdict:** DELETE — a pre-implementation snapshot whose entire "What Does NOT Exist Yet" list now exists.
- **Was:** (2026-02-17, commit `54a4e1342`) Generation 2 of the compact-cache trio: a reality check taken right after the `getter-migration` branch merged. Reports that all CSS access had been centralized into `layout/src/solver3/getters.rs` (3 macro families + ~37 handwritten getters, 84 public fns, 3473 lines) but that **no** compact cache existed yet and every getter still went through `get_property_slow()`. Audits which properties belong in which tier, lists 3 remaining out-of-getters cache accesses, and recommends a 4-phase build order (Tier 1 → Tier 2 → Tier 2b → Tier 3 + taffy bridge).
- **Landed:** Its §1 "What Does NOT Exist Yet" is now false in every line — see the COMPACT_CACHE_PLAN entry. Its §6 build order was followed except that Phase 4's Tier 3 was dropped. Its §7 "Keep CssPropertyCache" recommendation held: `core/src/prop_cache.rs:708` `pub struct CssPropertyCache` is still the cascade engine, with the compact cache hanging off it as an `Option` field at `:738` — exactly the "read-optimized projection" split it argued for.
- **Superseded by:** `scripts/COMPACT_CACHE_STATUS_V2.md` (same day, later commit `8e812e3b1`), which re-audits the same ground post-implementation. Nothing in V1 survives that V2 doesn't restate more accurately.
- **Still open:** none of its own. One of its three flagged out-of-getters accesses is still live and is the more interesting one: `layout/src/solver3/layout_tree.rs` uses `dependency_chains` for font-size resolution rather than a getter — the doc correctly called this "⚠️ Special", and it remains special.
- **Research value:** none beyond the §7 argument ("cascade engine vs read-optimized projection"), which COMPACT_CACHE_PLAN and the code comments both carry.

---

#### scripts/COMPACT_CACHE_STATUS_V2.md

- **Verdict:** DELETE — generation 3's roadmap; every prioritized item has since been done or deliberately dropped.
- **Was:** (2026-02-17, commit `8e812e3b1`) The post-implementation audit. Reports 43/98 getters (43%) on a compact fast path, ~55 properties stored (T1: 21, T2: 28, T2b: 6), **16 properties stored but never read via a fast path** (a table naming each), Tier 3 at 0% utilization, and 18 `get_css_property_value!` taffy-bridge getters bypassing the cache entirely. Then a 5-phase prioritized path forward with hour estimates, plus a §7 edge-case analysis (inheritance handled because the builder runs after `compute_inherited_values()`; pseudo-states handled because every fast path checks `node_state.is_normal()` first) and a §10 "should CssPropertyCache be kept? Yes, for now".
- **Landed:** Effectively the whole roadmap.
  - Phase 1 (border-collapse / z-index / border-spacing fast paths) — landed in the very commit that added this file, `8e812e3b1` "feat: add compact fast-paths to border-collapse, z-index, border-spacing getters"; `CompactLayoutCache::get_border_collapse` at `css/src/compact_cache.rs:1613`.
  - Phases 2-3 — `getters.rs` now has 188 `compact` references and 21 `compact = ` macro variants; `taffy_bridge.rs` has 57 (it had 0), so the "taffy bridge bypasses the cache entirely" finding is resolved.
  - Phase 4 item 9 ("remove Tier 3 allocation if not used") — done by deletion; `rg tier3` finds nothing.
  - §7's edge-case claims still hold structurally: the builder is `build_compact_cache_with_inheritance` (`core/src/compact.rs:447`) and runs post-inheritance.
- **Superseded by:** The code, plus the later hot/cold + `hot_flags`/`extra_flags`/`dom_declared_flags` design, which answers V2's §6 "16 properties stored but not read" from the other direction: several of those (`transform`, `box-shadow`, `text-decoration`, `background`, scrollbar props) are now handled by a **negative** fast path — a flag proving the property is unset — rather than by storing and reading a value.
- **Still open:** Two items, both real: (a) **Phase 4 item 11, "Benchmark"** — the entire 3-10× claim remains unmeasured; (b) **Phase 4 item 10, `set_css_property()` → O(1) compact update** — still a full rebuild, with the frame-≥2 correctness footgun documented at `core/src/styled_dom.rs:397-453`. Phase 5 item 12 ("deprecate raw `CssPropertyCache` access") was answered "no, keep it" by §10 and by the code, so it is closed rather than open.
- **Research value:** Low on its own. The one genuinely reusable artifact is §7's edge-case checklist for *any* precomputed-projection cache: inheritance ordering, pseudo-state invalidation, runtime overrides, non-px units, font resolution, `calc()`, memory. Worth folding as an appendix into the COMPACT_CACHE_PLAN research keeper rather than preserving separately.

---

#### scripts/gemini_compact_cache_response.md

- **Verdict:** DELETE — an LLM design review whose substantive corrections are already folded into the plan and the code.
- **Was:** (2026-02-17, same commit `611c1251f` as the plan) A 26 KB external review endorsing the three-tier split. Validates tier-by-phase alignment, endorses the u64 Tier-1 bitfield and the sentinel pattern (arguing the `!= SENTINEL` branch is >99% predictable and therefore nearly free), confirms the L2/L3 residency claims with latency figures (~4 ns L2, ~40 ns L3, ~100 ns DRAM), and analyses the three-pass cascade for interference (conclusion: the only real intra-tier dependency is `font-size` before other `em`-based values → do a two-pass Tier 2). Its one hard push-back: a **signed 24-bit fixed-point value at ×1000 precision only spans ±8388.6 px**, so `width: 10000px` would overflow.
- **Landed:** The 24-bit push-back was accepted before the plan was even committed — COMPACT_CACHE_PLAN's Tier-2 section explicitly says "The u32 encoding with MSB sentinels is strictly better than the previous flags-in-top-4-bits approach: 28 bits instead of 24 bits... range jumps from ±8,388 to ±134,217 px", and that is what shipped (`encode_pixel_value_u32` at `css/src/compact_cache.rs:1041`). The `font_family` → u64 hash suggestion it called "brilliant" also shipped and then grew a reverse map (`font_hash_to_families`) plus per-node dirty tracking. Its "pre-calculate hashes during parsing" suggestion is *not* implemented — hashes are computed during `build_compact_cache()`.
- **Superseded by:** `scripts/COMPACT_CACHE_PLAN.md`, which is the same design with the review's corrections already applied. Keeping both means keeping the pre-correction numbers (24-bit, ±8388 px) alive next to the post-correction ones, which is actively confusing.
- **Still open:** One claim was never validated: the review asserts the sentinel branch is "nearly free" and that Tier 1 stays L2-resident. Nobody measured it (see the benchmark gap above). Its `inherit`-keyword warning ("a `width: inherit` fast path must read the parent's computed value, and if the parent's was a percentage it must be re-resolved") is worth re-checking against `core/src/compact.rs` before trusting the inherit encoding.
- **Research value:** Low-moderate, and fully dominated by the plan. Only two bits are unique: the cache-latency table (L2 ~4 ns / L3 ~40 ns / DRAM ~100 ns) used to justify the tier budget, and the maintenance-cost caveat ("adding a new property now requires updating tier assignment, struct definition, bitfield packing, and encoder/decoder — a conscious trade-off that must be documented"). That caveat has proven true and could be pulled forward into the plan's header as a one-line warning.

**Which generation won:** generation 1's *design* (COMPACT_CACHE_PLAN, endorsed by the Gemini review) shipped essentially intact minus Tier 3; generation 3's *roadmap* (STATUS_V2) is what actually got executed and is now complete. Generation 2 (STATUS) was a transient pre-implementation snapshot and is pure noise today. Keep exactly one document — the plan — as research.

---

#### scripts/VEC_ITERATOR_PLAN_2026_05_15.md

- **Verdict:** ACTIVE — real open work, but the checkbox state badly understates progress.
- **Was:** (2026-05-16, commit `84cfca5cb`) An autonomous-loop task file for making every codegen-emitted `AzVec<T>` wrapper yield elements that survive the Vec being closed. Today several host bindings overlay element wrappers on the Vec's internal buffer, so `close()`ing the Vec dangles them. Classifies every element type into Primitive (bulk-copy to a host typed array) / Wrapper-with-`_clone` (per-element `AzT_clone`) / POD-without-`_clone` (byte copy), proposes shared IR predicates in `managed_lang_helpers.rs`, then walks 11 phases across Java, Kotlin/Scala, C#, Ruby, Node, Lua, OCaml, Haskell, Python, plus tests. Uses `[ ]`/`[x]`/`[⊘]`/`[—]` markers.
- **Landed:** 7 boxes are checked but **at least six more items are done and never checked off** — verify against code, not the markers.
  - V1.2 / V3.2 (JVM & CLR primitive bulk arrays) — **done**: `doc/src/codegen/v2/lang_java/wrappers.rs:690` maps `"u8" | "i8" | "bool" => ("byte[]", "getByteArray", "toByteArray")`; `lang_csharp/wrappers.rs:927` the `ToByteArray` equivalent.
  - V4.1 (Ruby clone-via) — **done**: `lang_ruby/wrappers.rs` struct-element branch now emits `yield Native.az_<elem>_clone(buf + i * elem_size)` when a `_clone` export exists, with an explicit fallback warning comment otherwise. The in-code comment cites commit `4edb65d7c`.
  - V5.1 (Node) — **done**: `lang_node/wrappers.rs:717` computes `has_clone` inside the iterator emitter.
  - V8.1 (Haskell) — **done**: `lang_haskell/types.rs:540-575` emits a per-element `Az<X>_clone_via` call, with a module-scoped `foreign_imports` registry to avoid GHC-29916 duplicate-declaration errors when two Vec types share an element type (`StringVec` + `IcuStringVec` over `String`).
  - V7.1 / V7.2 (OCaml) — checked, and confirmed present: `lang_ocaml/wrappers.rs:574` `detect_vec_to_list_shape`, `:486`/`:556` `emit_ocaml_vec_to_array_if_primitive`.
- **Superseded by:** n/a — no later document covers this.
- **Still open:** Genuinely, three groups.
  1. **V0.2 shared predicates never happened.** `detect_vec_elem_type` is still copy-pasted five times under different names: `lang_ruby/wrappers.rs:415`, `lang_kotlin/wrappers.rs:397` (`_kt`), `lang_java/wrappers.rs:528` (`_jvm`), `lang_csharp/wrappers.rs:462` (`_cs`), `lang_haskell/types.rs:495`. No `classify_vec_elem` / `VecElemShape` / `has_clone_export` / `clone_export_name` exists anywhere. Every per-language fix so far re-derived the same classification locally.
  2. **V6.1 (Lua numeric `__index`) not done.** `lang_lua/wrappers.rs:393-412` emits only the `__len` clause for Vec-shaped structs (`ptr`/`len`/`cap` detection); `__index` is still bound to the methods table, so `vec[i]` element access does not exist and there is nothing to make safe yet.
  3. **All smoke tests (V1.4, V2.3, V3.3, V4.4, V5.3, V6.3, V7.3, V8.3, V11.1-V11.3) are open.** There is no `scripts/test_vec_iter_safety_all.sh`. The whole class of bug — iterate, close, use element — is unverified end-to-end in every language, including the ones marked FIXED. Given the memory-index history on double-free bugs in this repo, that is the highest-value remaining item. V9.2 (Python `__len__`/`__getitem__` on non-primitive Vec wrappers) is explicitly marked `[—]` won't-fix-this-session and is still not done; V9.1 also recorded a live codegen bug — the emitted `len(&self, dom_vec)` takes both `&self` and a redundant argument.
- **Research value:** The three-shape element taxonomy (Primitive / Wrapper-with-clone / POD-without-clone) driving three emit paths is a clean, reusable model for any C-ABI-to-managed-language binding generator, as is the invariant it enforces ("a yielded element must outlive its container"). Moderate value, but it is operational-plan-shaped, so it belongs where it is until finished.

---

#### scripts/MEMORY_AUDIT_DYNAMIC_2026_05_15.md

- **Verdict:** ARCHIVE — a read-only point-in-time audit; most findings fixed, but the leak class it names is still partly live.
- **Was:** (2026-05-15, commit `4edb65d7c`) A no-code-changes audit of the four dynamic-binding codegens (Ruby, Node, Lua, OCaml) for the two bug classes fixed for JVM/CLR in `62094b885` (consume-after-by-value → double-free) and `75a1fbcd2` (Option/Result payload extraction → heap leak / dangling payload). Verdict grid: Ruby and Node OK on consume; **Lua and OCaml emit no consume call at all** despite registering `__gc` / `Gc.finalise` on every wrapper; all four leak the outer Option/Result; none clone Vec elements. Ends with an 11-item cross-language fix order and a caveat that OCaml's `Obj.set_field`-based `azul_consume` assumes `disposed` sits at record field index 1.
- **Landed:** The top of the fix order is done.
  - Lua consume (#1) — `lang_lua/wrappers.rs:634-655` now computes `consumed_self` / `consumed_arg_indices` from `ArgRefKind` and threads `azul._consume` through; comment at `:637` cites `62094b885`.
  - OCaml consume (#2) — `lang_ocaml/managed.rs:158` defines `azul_consume`, exported at `:563`, and it is now **actually called**: `wrappers.rs:247` (`azul_consume d;`), `:1199` (`let _ret = make_X (...) in azul_consume self; _ret`), `:1209`.
  - Ruby Option/Result outer-free (#4) — done: `lang_ruby/types.rs:601-606` "AzOption<T>.to_opt — Ruby nullable mirror with delete+clone", `emit_ruby_to_opt_body` at `:923` with `ruby_has_delete` guard at `:883` and the `Native.az_<x>_delete(self.to_ptr)` call at `:931`.
  - Lua Option/Result (#6) — done: `emit_lua_to_opt_body` at `lang_lua/wrappers.rs:934-950` emits `C.<X>_delete(self)` followed by `azul._consume(self)` to disarm `__gc`.
  - Ruby Vec-element clone (#8) — done, same evidence as VEC_ITERATOR_PLAN V4.1.
- **Superseded by:** `scripts/VEC_ITERATOR_PLAN_2026_05_15.md` for items 8-11 (the Vec-iterator half), which tracks them per-language with checkboxes.
- **Still open:** Item **#5, Node Option/Result outer-struct delete + wrapper-payload clone** — I found no `to_opt`/`isSome`/option-delete emitter in `doc/src/codegen/v2/lang_node/` (only the generic per-type `_delete`/`has_delete_for` registry at `wrappers.rs:224-265`), so the per-call payload leak the audit describes appears live. Item **#3** (Node static factories consuming wrapper args) and item **#7** (OCaml Option/Result extractor) are likewise unconfirmed. The audit's own file paths are `/Users/fschutt/...` — a different machine — so line numbers in it are advisory only.
- **Research value:** Low as a document, but the **two bug shapes it names are the durable artifact** and they recur across every binding this repo generates: (1) *consume-after-by-value* — when a C ABI takes a struct by value, the host wrapper's finalizer must be disarmed (`undefine_finalizer` / `SetFinalizer(nil)` / `FOwned := False` / `$$self = undef` / `consumed = true` / IORef tombstone), otherwise every call is a double-free; (2) *outer-container free* — extracting a payload from an `Option`/`Result` must clone the payload before deleting the outer struct, or the payload borrows freed memory. Worth a one-page distillation for `scripts/research/` covering both audits (see next entry) rather than preserving two long per-language grids.

---

#### scripts/MEMORY_AUDIT_NICHE_2026_05_15.md

- **Verdict:** ARCHIVE — same audit for nine more languages; the headline finding has since been fixed everywhere.
- **Was:** (2026-05-15, commit `2bbd35f89`) The companion audit covering Haskell, Perl, Pascal, Go, Zig, Fortran, Smalltalk, COBOL, PHP. Headline: eight of nine emit a deferred finalizer and **every one of them was missing the consume/disarm step**, so any C-ABI call taking a wrapper by value double-frees. Extra per-language findings: Pascal's `FRaw`-by-value embedded record creates a transitive double-free through record-copy semantics; Fortran has a factory-return double-free independent of the consume bug; Zig and PHP emit uncompilable / wrong-pointer code for self-by-value calls. Closes with a per-language table naming the exact consume analogue each needs and where in the codegen it goes.
- **Landed:** The consume fix was ported to **all eight** finalizing languages — this audit's central recommendation is complete.
  - Haskell: `lang_haskell/wrappers.rs:248` — the wrapper record became `data X = X { unX :: !(Ptr (..)), XConsumed :: !(IORef Bool) }`, with `consume<X>` at `:273` and auto-consume of owned wrapper args noted at `:104-105`. This is the IORef-tombstone design the audit prescribed.
  - Pascal: `lang_pascal/wrappers.rs:376`, `:472`, `:581` — `FOwned := False;` at consume sites, `if FOwned then` guard at `:360`.
  - Go: `lang_go/managed.rs:584`, `:624`, `:630` — `runtime.SetFinalizer(ret, nil)` / `(config, nil)` / `(ref, nil)`.
  - Zig: `lang_zig/wrappers.rs:188` `consumed: bool = false,` field, `:278` `if (self.consumed) return;` in deinit, `:423` `self.consumed = true;`.
  - Fortran: `lang_fortran/wrappers.rs:223` and `:463` `self%owned = .false.`, `:335` `r%owned = .true.` for the factory-return path.
  - Smalltalk: `lang_smalltalk/wrappers.rs:187` and `:340` `handle := nil`, with `finalizationRegistry` enrolment at `:152`.
  - PHP: `lang_php/wrappers.rs:487`, `:491` `$this->ptr = null;`.
  - Perl: `lang_perl/wrappers.rs:214-224` — `$$self = undef;` gated on `args[0].ref_kind == Owned`, with a comment explicitly naming "the Pascal/Fortran/JVM `__consume` / FOwned/owned-flag pattern".
- **Superseded by:** n/a. The Vec-iterator sub-finding for Haskell was picked up by `VEC_ITERATOR_PLAN_2026_05_15.md` phase V8 and is done.
- **Still open:** The audit's second class — **Option/Result outer-free is absent in all nine** — is largely still open. I found no `ToMaybe`/`ToEither`/option-delete emitter in `lang_haskell/types.rs`, none in `lang_go/`, none in `lang_php/wrappers.rs`. Only Ruby and Lua (from the *other* audit) gained one. Every `Option<T>`/`Result<T,E>`-returning method whose payload owns inner heap (`AzString.vec.ptr`, `AzDom`'s styled-node vec, …) still leaks in those bindings. Also unresolved: the audit's caveat that Pascal's `Wrap(ARaw)` by-value record copy needs a pointer-taking variant or a refcount discipline — the `FOwned := False` fix addresses the direct consume path but not the transitive copy path.
- **Research value:** Same as the DYNAMIC audit — the value is the **cross-language table of "what does 'disarm the finalizer' mean in language X"** (IORef tombstone / `$$self = undef` / `FOwned := False` / `SetFinalizer(nil)` / `consumed: bool` / `owned = .false.` / `handle := nil` / `$this->ptr = null`). That single table is the transferable artifact and is now empirically validated by eight landed implementations. Distil both audits into one `scripts/research/` page on FFI ownership-transfer across finalizing runtimes; archive the per-language walkthroughs to git.

---

#### scripts/BTREEMAP_TO_VEC_PLAN.md

- **Verdict:** DELETE — fully implemented, including the helper names; and its risk section was wrong in an instructive way.
- **Was:** (2026-02-16, commit `3048f34fb`) A 10-phase plan to replace `BTreeMap<usize, LogicalPosition>` with `Vec<LogicalPosition>` for `calculated_positions`, motivated by O(log N) lookups, poor locality, and BTreeMap node allocation churn on 12k-node DOMs. Prescribed a `PositionVec` type alias, `pos_get`/`pos_set`/`pos_contains` helpers, and an in-band sentinel `LogicalPosition { x: f32::MIN, y: f32::MIN }` instead of `Option` to keep the Vec tightly packed. Enumerated 21 type signatures and ~114 references across `cache.rs`, `fc.rs`, `display_list.rs`, `positioning.rs`, `mod.rs`, `paged_layout.rs`, `window.rs` plus 6 read-only dll sites. Risk assessment: "Low risk… **Sentinel value: f32::MIN is safe — no real position is ever that value**… Easy rollback."
- **Landed:** Exactly as specified, names and all.
  - `layout/src/solver3/mod.rs:133` `pub(crate) const POSITION_UNSET: LogicalPosition = LogicalPosition { x: f32::MIN, y: f32::MIN };`
  - `:142` `pub type PositionVec = Vec<LogicalPosition>;`
  - `:150` `pos_get`, `:156` `pos_set` (resize-with-sentinel on out-of-range write), `:165` `pos_contains`.
  - In use across the whole prescribed surface: `layout/src/window.rs:595` `pub calculated_positions: solver3::PositionVec` and `:2801`; `solver3/display_list.rs:1899`; `solver3/mod.rs:893`, `:928`, `:1090-1095`; `layout/src/headless.rs:29`, `:1092`, `:1180`, `:1316`. The dll read sites are `wr_translate2.rs:795/885/1069`, `shell2/common/layout.rs:66`.
  - The helpers are unit-tested at `solver3/mod.rs:1277-1321`, including a test asserting both components of the sentinel because `pos_get`/`pos_contains` only test `x`.
- **Superseded by:** n/a for the data structure. But the sentinel decision was **partially walked back by bug fixes**, which is the durable content.
- **Still open:** The doc's "no semantic change" claim did not hold. Replacing `Option`-by-absence with an in-band `f32::MIN` value moved a *type-level* guarantee to a *convention*, and the convention leaked twice: commit `7ac52d301` "fix(layout): unassigned-position sentinel escaped into the display list", and commit `584f5797f` "fix(solver3): collapsed-through empty blocks receive a position" — whose message reads "the node and its whole subtree stayed at the POSITION_UNSET sentinel and every DL item they emitted (Border, HitTestArea) landed at (f32::MIN, f32::MIN)… Visible in azul-self-test as the repeating 624x0 drop warnings." That was open for ~5 months after the swap. Anything downstream that reads `calculated_positions` **must** go through `pos_get`, and there is no compile-time enforcement of that; a newtype wrapper with a private field, or a debug assertion in the display-list emitter, would close it. The plan's "run git2pdf benchmark to measure improvement" (Phase 10) also has no recorded result — the claimed ~50-100 ms/pass was never confirmed.
- **Research value:** Moderate and specific: **the cost of trading `Option` for an in-band sentinel in a hot array**. The upside was real (8 B/entry, memcpy clone, O(1) index, perfect locality) and the code kept it. The downside was equally real and took two production bugs to surface — the sentinel is a *value*, so "I forgot to write this entry" silently becomes "this thing is at (-3.4e38, -3.4e38)" and propagates all the way to the display list, whereas `Option` would have been a compile-time or `unwrap`-time failure at the source. The honest write-up is: keep the packed Vec, but make the sentinel unforgeable (private field + accessor) rather than merely conventional. Pair this with the `POSITION_UNSET` unit tests as the worked example.


## Part 05 — Scrolling, Scrollbars, Hit-Testing, Tag IDs, Cursor (12 files)

Verified against master @ f1c43ba60 (2026-08-01). Note two repo-wide renames that
invalidate almost every path in these docs:

- `IFrame*` → `VirtualView*` (`layout/src/managers/iframe.rs` → `virtual_view.rs`,
  `IFrameManager` → `virtual_view_manager`, `IFrameCallbackReturn` → VirtualView return).
- `*_v2.rs` → `*.rs` (`dll/src/desktop/shell2/common/event_v2.rs` → `event.rs`,
  `layout_v2.rs` → `layout.rs`). `core/src/hit_test_tag.rs` was merged into
  `core/src/hit_test.rs`.

---

#### scripts/scroll3.md

- **Verdict:** DELETE — proposed timer-based scroll architecture; shipped verbatim.
- **Was:** A German post-mortem grading an earlier scroll implementation (Phases A–F) as
  mostly unwired scaffolding, then proposing the replacement: make `ScrollManager` a pure
  *recorder* (`ScrollInput` + `ScrollInputSource{WheelDiscrete,TrackpadContinuous,Programmatic}`),
  move physics into a reserved-ID timer callback that pushes `CallbackChange::ScrollTo`, and
  delete the macOS `physics_tick(1.0/60.0)` hack in `render_and_present`. Also contains an
  IFrame side-analysis (`scan_for_iframes` race: called before `layout_results` insertion).
- **Landed:** every named symbol exists.
  `layout/src/managers/scroll_state.rs:79` `ScrollInputSource`, `:102` `ScrollInput`,
  `:122` `ScrollInputQueue` (Arc<Mutex>, `take_all`/`take_recent`), `:546` `record_scroll_input`,
  `:561` `record_scroll_from_hit_test`, `:759` `find_scroll_parent`.
  Physics moved out to `layout/src/scroll_timer.rs:68` `ScrollPhysicsState`, `:86` `NodeScrollPhysics`.
  Reserved timer IDs are real constants: `core/src/task.rs:70` `SCROLL_MOMENTUM_TIMER_ID = 0x0002`,
  `:72` `DRAG_AUTOSCROLL_TIMER_ID = 0x0003` (the doc's `0xABCD_2000` magic value never shipped).
  Zero hits for `physics_tick` / `add_scroll_impulse` / `needs_animation_frame` anywhere outside
  vendored webrender — the Phase B–E scaffolding the doc wanted removed is gone.
- **Superseded by:** scroll5.md (status ledger) → scroll6_report.md (final verification).
- **Still open:** none.
- **Research value:** low on its own — the "manager records, reserved timer integrates, changes
  land as a transactional `CallbackChange`" pattern is worth one line elsewhere, but the doc is
  90% obsolete file/line references.

---

#### scripts/scroll4.md

- **Verdict:** DELETE — external design review; every recommendation implemented or improved on.
- **Was:** An advisory review (sections A–F) of `SCROLL_ARCHITECTURE.md`: confirms the
  `apply_content_based_height` sizing bug and adds the safeguard ("skip expansion only when the
  containing block is finite"), sketches exponential-decay momentum + spring overscroll, splits
  *logical* (clamped) from *visual* (unclamped) scroll for rubber-banding, and proposes a
  per-frame drag-select auto-scroll in `window.rs::update()`.
- **Landed:**
  - Sizing fix with the exact safeguard: `layout/src/solver3/cache.rs:2199-2212`
    (`is_scroll_container` + `skip_expansion = … && containing_block_size.height.is_finite() && > 0.0`).
  - Physics: `layout/src/scroll_timer.rs` (decay, `is_rubber_banding` at `:90`), constants sourced
    from a `ScrollPhysics` config rather than the doc's hardcoded 0.95/0.92.
  - Logical/visual split: `scroll_state.rs:797` `set_scroll_position` vs `:823`
    `set_scroll_position_unclamped`.
  - Auto-scroll landed as a *separate timer*, not a `window.rs::update()` poll —
    `dll/src/desktop/shell2/common/event.rs:331` `auto_scroll_timer_callback` (fully implemented,
    ~130 lines: edge-threshold delta from `find_scroll_parent` + `get_scroll_node_info`,
    `timer_info.scroll_to(...)`).
- **Superseded by:** scroll3.md's timer architecture won over this doc's render-loop `physics_tick(dt)`
  and `window.rs::update()` polling. Same physics, different host.
- **Still open:** none.
- **Research value:** none unique (the friction/spring formulas now live in code with better
  provenance in `scroll_timer.rs` module docs).

---

#### scripts/scroll5.md

- **Verdict:** DELETE — a transient status ledger; all six "not finished" items are finished.
- **Was:** A German status report on scroll3/scroll4 execution: table A (done: `scan_for_iframes`
  fix c909daa5, ScrollInput types f37b012a, `scroll_physics_timer_callback` 96617f31, macOS
  `physics_tick` hack removal 7e829b97) and table B (six open items), plus the key architectural
  ruling: IFrame re-invocation belongs in the **ScrollTo processing path**, not in the timer —
  "the timer knows nothing about IFrames", so the virtual-view swap is transparent to physics.
- **Landed:** all six B-items verified closed —
  1. `auto_scroll_timer_callback` is no longer a stub (`event.rs:331`).
  2. Magic `AUTO_SCROLL_TIMER_ID = 0xABCD_1234` gone; `DRAG_AUTOSCROLL_TIMER_ID` used (`core/src/task.rs:72`).
  3./6. `begin_frame`/`end_frame`/`record_sample`/`FrameScrollInfo`/`previous_offset`/
     `had_scroll_activity` flags on ScrollManager: **zero hits** outside webrender and two stale
     doc comments in `layout/src/event_determination.rs:40,54` (the only leftover: comments
     naming a deleted `record_sample()`).
  4. VirtualView re-invocation wired at `layout/src/window.rs:3443` and `:9186` (`check_reinvoke`).
  5. All four platforms now use `record_scroll_from_hit_test` + timer start:
     `macos/events.rs:452-482`, `windows/mod.rs:3805-3824`, `x11/events.rs:744-763`,
     `wayland/mod.rs:3154-3173` (plus `wayland/mod.rs:3184-3214` axis_stop → TrackpadEnd).
  - `virtual_scroll_size`/`virtual_scroll_offset` landed on `AnimatedScrollState`
    (`scroll_state.rs:348,350`) fed by `update_virtual_scroll_bounds` (`:926`) from
    `window.rs:3526,3553`.
- **Superseded by:** scroll6_report.md.
- **Still open:** cosmetic only — two `record_sample()` references in
  `layout/src/event_determination.rs:40,54` describe a function that no longer exists.
- **Research value:** one durable ruling worth carrying forward: *virtual-view re-invocation is
  handled in the ScrollTo consumer, so scroll physics stays ignorant of DOM replacement.*

---

#### scripts/scroll6_report.md

- **Verdict:** DELETE — final verification pass; its two named bugs and one "known gap" are all fixed.
- **Was:** A component-by-component audit of the shipped scroll stack against
  `SCROLL_ARCHITECTURE.md` (commit d3b6372f), with a 17-row status table. Declared everything ✅
  except two bugs (`calculate_scrollbar_states()` and `is_node_scrollable()` both ignoring
  `virtual_scroll_size`, so a VirtualView with 2 M virtual px but 1 000 px rendered gets a
  wrong/absent scrollbar) and one gap (CPU renderer ignores scroll offsets).
- **Landed:** all three closed.
  - `scroll_state.rs:1091-1112` — `calculate_scrollbar_states()` filter now uses
    `s.virtual_scroll_size.map_or(s.content_rect.size.height, |vs| vs.height)` per axis.
  - `scroll_state.rs:1139` — thumb geometry uses
    `virtual_scroll_size.map_or(content_rect.size, |vs| vs)`.
  - `scroll_state.rs:782-787` — `is_node_scrollable()` computes `effective_width/height` from
    `virtual_scroll_size`. Regression test at `:1888`
    `clamp_prefers_virtual_scroll_size_over_content_rect`.
  - CPU renderer: `layout/src/cpurender/raster.rs:767` `ScrollOffsetMap` + `:944`
    `scroll_offset_stack` (accumulated per `PushScrollFrame`), fed by
    `ScrollManager::build_scroll_offset_map` (`scroll_state.rs:510`).
  - Geometry unification the doc assumed: single source at
    `layout/src/solver3/scrollbar.rs:115` `compute_scrollbar_geometry` (+ `:138`
    `_with_button_size`), consumed by both `scroll_state.rs:1155` and `gpu_state.rs:171,197`.
- **Superseded by:** n/a (this *is* the terminal doc of the scroll3→6 chain).
- **Still open:** none.
- **Research value:** the 17-row goal/status table is a good *shape* for architecture-conformance
  audits, but its content is fully realized in code.

---

#### scripts/SCROLL_ARCHITECTURE.md

- **Verdict:** RESEARCH — the durable three-sizes + no-viewport-scroll model; strip the stale bug sections.
- **Was:** The reference doc the whole scroll3→6 chain audits against. Defines the **three sizes**
  (scroll clip size = container inner box; content size = children extent; virtual scroll size =
  logical total for lazy scroll) and the thumb ratio formulas; explains `PushScrollFrame` →
  WebRender `define_scroll_frame` (spatial transform only — a *separate* clip is required to
  actually hide overflow); argues from first principles why Azul has **no viewport-level
  scrolling** (`<html>` is the window, a CSD titlebar is its first child, so scrolling must happen
  on `<body>` or below); then diagnoses the body-expands-to-content bug and specifies the fix.
- **Landed:** the model is the code.
  `layout/src/solver3/scrollbar.rs` is the single geometry authority; `PushScrollFrame` →
  `dll/src/desktop/compositor2.rs:883-1035` (push_clip + define_scroll_frame + clip_chain, exactly
  as described); three sizes are the three fields on `AnimatedScrollState`
  (`container_rect`, `content_rect`, `virtual_scroll_size` — `scroll_state.rs:345-350`).
  §3.4 body-scroll fix landed at `layout/src/solver3/cache.rs:2199-2212`.
  §5's "CPU renderer: no scroll offset (TODO)" is now false (`cpurender/raster.rs:944`).
- **Superseded by:** partially, by scroll6_report.md for status; the *model* is not superseded.
- **Still open:** **one real, load-bearing regression.** §2.1/§6 rest on
  `<html> { height: 100% }`. That UA rule is **commented out today**:
  `core/src/ua_css.rs:834` `// (NT::Html, PT::Height) => Some(&HEIGHT_100_PERCENT),` with a
  2026-06-02 DIAG note ("the lifted `get_ua_property` jump table mis-dispatches
  (Text/Button, Height) → THIS (Html, Height) arm → children wrongly get height:100%"), guarded by
  the test `core/src/ua_css.rs:1420 html_has_no_default_height`. The real fix named in the comment —
  the node_type jump-table dispatch / table-mirror in the lift — is **not done**. Until it is,
  `<html>` has `height: auto` and the very failure mode §3.1 describes (container ≈ content →
  useless 100 % scrollbar) can return by a different route. Same for
  `ua_css.rs:556 // (NT::Body, PT::Height)`.
- **Research value:** high — the three-sizes vocabulary, the "scroll frame is a transform, not a
  clip" WebRender distinction, and the explicit *why-we-differ-from-browsers* argument about
  viewport scrolling are the transferable pieces. Good `scripts/research/` candidate as
  "scroll container coordinate & sizing model", with §3/§6 (fixed bug) cut down to a one-line
  historical note plus the live `ua_css` caveat above.

---

#### scripts/SCROLLBAR_BUGS.md

- **Verdict:** DELETE — 13-item bug tracker; all 13 resolved, including the four marked `[ ]`.
- **Was:** A tracker for `scrolling.c` (S1–S11) and `infinity.c` (I1–I2), plus a genuinely good
  "holistic" section that collapses 13 symptoms into 6 root causes — the headline being **Root
  Cause 1/2: scrollbar thumb geometry computed in three independent places with different
  formulas** (`paint_scrollbars()` in display_list.rs, `compute_vertical_thumb_transform()` in
  gpu_state.rs, `calculate_scrollbar_states()` in scroll_state.rs), and the thumb position being
  *baked into the display list* so GPU-only scroll froze it. Ends with an 8-phase plan.
- **Landed:** all six root causes fixed; the doc's own `[ ]` items are stale.
  - RC1/RC2 (S1,S2,S3,S8): unified geometry at `layout/src/solver3/scrollbar.rs:115`, consumed by
    `scroll_state.rs:1155` **and** `gpu_state.rs:171,197`. Thumb is GPU-animated —
    `gpu_state.rs:183,208` build `ComputedTransform3D::new_translation(...)` from
    `geom.thumb_offset` and push via `update_scrollbar_transform_key` (`gpu_state.rs:216`).
    Corner + button-size handling is in `compute_scrollbar_geometry_with_button_size`.
  - **S7 (`[ ]`, macOS overlay fade)** — DONE. Fade config on `GpuStateManager`
    (`gpu_state.rs:52-59` `fade_delay`/`fade_duration`/`scrollbar_fade_active`), per-frame
    interpolation in `LayoutWindow::synchronize_scrollbar_opacity`, called from
    `dll/src/desktop/shell2/common/layout.rs:713` and `wr_translate2.rs:2634,2765,2860`.
    CSS surface shipped too: `-azul-scrollbar-fade-delay` / `-azul-scrollbar-fade-duration`
    (`css/src/props/style/scrollbar.rs:244-280`), `ScrollbarVisibilityMode::WhenScrolling` (`:228`).
    `gpu_state.rs:94-99` even records that a *duplicate* fade implementation was deleted in favor
    of the `window.rs` one — a negative-control the doc never had.
  - **S4 (trackpad rubber-band)** — DONE via the exact mechanism the doc proposed:
    `ScrollInputSource::TrackpadEnd` (`scroll_state.rs:87`), emitted by macOS phase-Ended
    (`macos/events.rs:452-482`) and Wayland `axis_stop` (`wayland/mod.rs:3184-3214`), consumed by
    `scroll_timer.rs:90 is_rubber_banding`.
  - **S10 (`[ ]`, selection drag auto-scroll)** — DONE: `event.rs:331 auto_scroll_timer_callback`,
    which explicitly handles `CursorPosition::OutOfWindow` (`event.rs:355-364`, "MWA-B8") — i.e.
    it also closes **S11**'s outside-window requirement; Windows side has
    `SetCapture` at `windows/mod.rs:3350,3408`, drag state at `window.rs:854 currently_dragging_thumb`
    (remapped on DOM rebuild, `window.rs:9442`).
  - **I1 (`[ ]`, VirtualView scrollbar)** — DONE (see scroll6 entry: `scroll_state.rs:1103,1107,1139,784`).
- **Superseded by:** scroll6_report.md for the scroll-physics half; the scrollbar-geometry half is
  superseded by the module docs of `layout/src/solver3/scrollbar.rs` itself.
- **Still open:** none. (The `[ ]` checkboxes are the doc lying — exactly the "never trust a doc's
  status marker" pattern.)
- **Research value:** the *method* (collapse N symptoms into 6 root causes, then fix root causes)
  is good but generic; the specific "one geometry function, three consumers" invariant is better
  stated in `solver3/scrollbar.rs`'s own header. Nothing to keep.

---

#### scripts/SCROLL_COORDINATE_ARCHITECTURE.md

- **Verdict:** RESEARCH — coordinate-space typing rationale; the newtype it argues for shipped and holds.
- **Was:** Post-mortem on 5 scroll-rendering bugs whose shared root cause is that the display list
  stores **absolute window-space** coordinates while WebRender wants **scroll-frame-relative**
  ones, with the conversion done ad-hoc per `DisplayListItem` arm — so every new variant can
  silently forget `apply_offset()`, and the bug is invisible until a scroll container exists.
  Includes a full per-item audit table, then weighs three preventions: (1) type-level newtypes,
  (2) centralized `resolve_rect()`, (3) tests. Concludes with "Implemented: Approach 1".
- **Landed:** Approach 1 is real and pervasive. `WindowLogicalRect` is used 60× in
  `layout/src/solver3/display_list.rs`, 6× `cpurender/raster.rs`, 5× `compositor2.rs`, 4×
  `cpurender/compositor.rs`, 3× `headless.rs`, 2× each in `widgets/menubar.rs`,
  `widgets/drop_down.rs`, `layout/tests/demo_layout_regressions.rs`.
  `resolve_rect` at `dll/src/desktop/compositor2.rs:80`, with `offset_stack` at `:250` and the
  documented "do NOT push a new offset here" exception at `:961`.
- **Superseded by:** n/a — the only doc in this cluster arguing coordinate spaces.
- **Still open:** its own "Future work", partially:
  - Migration is ~85% done, not complete: `compositor2.rs` has 20 `resolve_rect` uses but still
    4 raw `scale_bounds_to_layout_rect` and 2 raw `apply_offset` call sites.
  - Approach 3 (an integration test pushing **every** item type inside a scroll frame and
    asserting frame-relative output) does **not** exist — the exact regression guard the doc says
    is needed to stop the bug recurring for new variants.
  - The Taffy tension in §"Taffy Integration Problem" is unresolved: the doc asks for
    `compute_child_layout()`'s scrollbar check to be unified with `compute_scrollbar_info()`;
    `layout/src/solver3/cache.rs:1580` and `:2199` still each derive `is_scroll_container`
    independently.
- **Research value:** high, and the most portable item in this cluster — "the display list is in
  window space, the renderer wants frame-relative space, and nothing but a type can enforce the
  conversion" is a general rendering-architecture lesson, with a worked cost/benefit of
  compile-time vs runtime vs test enforcement and a real outcome to point at.

---

#### scripts/SCROLL_CURSOR_TEXT_INPUT_ARCHITECTURE.md

- **Verdict:** ACTIVE — 44 KB, 8-phase plan; phases 1–7 shipped, phase 8 (code-editor widget) not started.
- **Was:** The largest doc here. Defines a W3C-CSSOM-shaped scroll-into-view stack
  (`scroll_rect_into_view` as the *only* primitive, `ScrollIntoViewOptions{block, inline, behavior}`
  with `ScrollLogicalPosition{Start,Center,End,Nearest}`, everything else a wrapper), then
  layers on: focus/cursor/selection → auto-scroll integration; contenteditable; a
  `Vec<Dom>`-per-line code-editor model; a native multi-cursor system; Ctrl+A selection scoping;
  and a code-editor widget. 8 implementation phases with priorities.
- **Landed:** phases 1–7.
  - Phase 1: `layout/src/managers/scroll_into_view.rs` (`ScrollAdjustment:47`,
    `scroll_node_into_view:177`, `scroll_cursor_into_view:209`, `calculate_axis_delta:387`),
    `ScrollLogicalPosition` exercised in `layout/tests/managers/scroll_into_view.rs`.
  - Phases 2–3: `layout/src/window.rs:3841,3860,5550,5695`, driven from
    `window.rs:6709` / `:8276 scroll_cursor_into_view_if_needed` and
    `dll/src/desktop/shell2/common/event.rs:2303,2758,3805`.
  - Phase 6 (multi-cursor): `core/src/selection.rs:257 MultiCursorState` with the doc's
    merge-overlapping semantics plus a *stable* `primary_id` the doc didn't anticipate
    (`:256-268`); `add_cursor:288`; live at `text_edit_manager.multi_cursor`
    (`layout/tests/contenteditable_e2e.rs:481,949,1090`).
  - Phase 7 (Ctrl+A scoping / Ctrl+D): `layout/src/window.rs:5227 select_next_occurrence`,
    `callbacks.rs:110 SelectAllResult`, `:456 SetSelectAllRange`, `:4229
    inspect_select_all_changeset`; dispatch at `event.rs:2564,3293,5706`,
    macOS `EditCommand::SelectAll` (`macos/mod.rs:326,347`).
- **Superseded by:** n/a. (`ScrollBehavior` moved from a bespoke enum to the CSS crate:
  `css/src/props/style/scrollbar.rs:26`, with per-platform defaults `:607-819` — a better home
  than the doc's `#[repr(C)]` sketch.)
- **Still open:**
  - **Phase 8, the code-editor widget: not started.** `rg code_editor|CodeEditor` → zero hits;
    `layout/src/widgets/` has `text_area.rs`/`text_input.rs` but no editor. §5's `Vec<Dom>`
    per-line model and §8.4's cross-line cursor movement are unimplemented design.
  - §6.3's `select_next_occurrence` TODO ("find next occurrence of selection and add cursor") is
    satisfied at `window.rs:5227`, but the doc's §6.5 multi-cursor keyboard shortcut table and
    §6.4 CSS-styled per-cursor rendering were not verified as complete.
  - Also note `layout/tests/contenteditable_e2e.rs` is dirty in the working tree — this area is
    under active edit right now.
- **Research value:** medium-high, and it should be **split**. §1–§2 ("one rect-based primitive,
  every higher-level scroll-into-view is a wrapper; W3C `ScrollLogicalPosition` semantics incl.
  `Nearest` = minimum scroll distance") is a clean, transferable API-design argument. §5/§8
  (code-editor-as-`Vec<Dom>`-of-lines) is unbuilt design that belongs in a plan doc, not research.

---

#### scripts/CURSOR_AND_TEXT_HIT_TEST_ANALYSIS.md

- **Verdict:** DELETE — proposed NodeId-through-text-pipeline plan; implemented end to end.
- **Was:** Diagnoses that text nodes produce **no hit-test areas**, so hovering text returns the
  *container*, and verifies four claims against source: `StyledRun` lacks `source_node_id`,
  `collect_inline_content()` drops the NodeId, `SimpleGlyphRun` lacks NodeId, and the display list
  emits no hit area for text. Proposes threading `source_node_id: Option<NodeId>` through the
  whole text pipeline (Option because list markers / `::before` have no DOM node), emitting a
  text hit area, then deleting two hacks: the text-child walk in `hit_test.rs` and the
  "container with selectable text children gets the tag" rule in `prop_cache.rs`.
- **Landed:** the NodeId propagation shipped exactly as specified.
  `layout/src/text3/cache.rs:1961-1969` — `StyledRun.source_node_id: Option<NodeId>` with the
  doc's own rationale in the doc-comment ("None for generated content (e.g., list markers,
  `::before/::after`)"). `layout/src/text3/glyphs.rs:49-50` — `SimpleGlyphRun.source_node_id`,
  threaded through the `process_glyphs` closure (`glyphs.rs:70`). Widely exercised —
  `source_node_id` appears across ~10 text3 test files plus `layout/tests/ifc_caching.rs:241`.
  The hit area shipped **not** as the proposed `DisplayListItem::TextHitArea` but as a
  `TAG_TYPE_CURSOR` (0x0400) tag carrying the cursor type in the tag itself, decoded into
  `HitTest.cursor_hit_test_nodes` (`core/src/hit_test.rs:30`) — no CSS lookup needed at cursor
  resolution time. The text-child hack is gone from `layout/src/hit_test.rs` (see next entry).
- **Superseded by:** CURSOR_HIT_TEST_ARCHITECTURE_REPORT.md, which found the *actual* dominant bug
  (inverted depth comparison) that this doc's §3.1 only saw the downstream symptom of. Winning
  conclusion: fix depth ordering first, and the text-child hack becomes deletable rather than
  needing a new display-list item type.
- **Still open:** none. (§5.6's "tag Text nodes directly instead of their container" was **not**
  adopted — the cursor-tag namespace made it unnecessary; `prop_cache`'s selectable-text rule
  survives for *selection*, which is the right split.)
- **Research value:** none beyond what's now in `layout/src/hit_test.rs`'s module header.

---

#### scripts/CURSOR_HIT_TEST_ARCHITECTURE_REPORT.md

- **Verdict:** DELETE — the "inverted depth logic" diagnosis; fixed, and the fix is documented in code.
- **Was:** Identifies three problems, headlined by a **critical inverted depth comparison** in
  `CursorTypeHitTest::new()`: WebRender returns hits front-to-back (depth 0 = frontmost) but the
  code initialized `best_depth = 0` and preferred `node_depth >= best_depth`, i.e. picked the
  *backmost* node. Combined with the text-child hack (which added `+1` to depth), `<body>`'s text
  child beat a `<button>`'s `cursor:pointer`, giving an I-beam over the whole body. Prescribes
  `best_depth = u32::MAX` + `<`, then deleting the hack.
- **Landed:** verbatim. `layout/src/hit_test.rs:60` `let mut best_depth: u32 = u32::MAX;` with the
  comment "Start with MAX so any node with a cursor property will be selected"; the guards at
  `:79` and `:100` are `if node_depth >= best_depth { continue; }` (equivalent to the prescribed
  `<`). The text-child walk is **gone** — replaced by a first loop over
  `hit_nodes.cursor_hit_test_nodes` (`:74-92`) that reads the cursor type straight off the tag.
  The doc's "Design Principles" are now the module header at `layout/src/hit_test.rs:6-27`.
  Two behaviors were added beyond the doc: a checked-access guard for stale VirtualView NodeIds
  (`:105-114`, a real panic fix: "len is 25 but the index is 27") and a browser-parity I-beam
  default for `contenteditable`/`TextArea` without explicit `cursor:` (`:132-147`).
- **Superseded by:** n/a — this is the winning conclusion of the cursor thread; the sibling
  CURSOR_AND_TEXT doc's heavier `TextHitArea` proposal lost to it.
- **Still open:** the doc's §7 third row, "intermittently 0 DOMs in hit-test — unclear, needs
  further analysis", was never resolved in writing. It may be the same stale-NodeId class the
  `:105` guard now absorbs, but that is unconfirmed.
- **Research value:** low as a document — one durable sentence ("WebRender hit results are
  front-to-back; frontmost = lowest depth wins") already lives at `layout/src/hit_test.rs:6-27`.

---

#### scripts/HIT_TEST_TAG_ANALYSIS.md

- **Verdict:** DELETE — tag-namespace migration plan; migration complete, extension variants shipped.
- **Was:** Root-causes "every click reads as a scrollbar hit" to `translate_item_tag_to_scrollbar_hit_id()`
  decoding `(tag >> 62) & 0x3`, which is `0` for all small sequential DOM TagIds → `0 =>
  VerticalTrack`. Rejects the in-flight bit-61 `SCROLLBAR_MARKER` patch in favor of an existing
  but unexported type-safe system: use WebRender ItemTag's **`u16` half as a namespace**
  (0x0100 DOM / 0x0200 Scrollbar / 0x0300 Selection / 0x0400 Cursor / 0x0500 reserved), leaving all
  64 payload bits free. Includes a proposed extended enum (SelectionHandle, ResizeHandle).
- **Landed:** fully, in `core/src/hit_test.rs` (the module was merged there, not kept as
  `hit_test_tag.rs`). Constants `:389-414` (`TAG_TYPE_DOM_NODE` … `TAG_TYPE_SCROLL_CONTAINER`,
  with 0x0500 used for scroll containers rather than "reserved"); `HitTestTag` enum at `:484` with
  the doc's `DomNode`/`Scrollbar` plus `Cursor{dom_id,node_id,cursor_type}` and a selection
  variant; encode/decode at `:605-690`. Consumers use the namespaces:
  `dll/src/desktop/wr_translate2.rs:849,931,966` (three passes: scroll-container, cursor, DOM node)
  and `compositor2.rs:1017`. `SCROLLBAR_MARKER` (bit 61): **zero hits** — the stopgap the doc
  argued against was correctly dropped.
- **Superseded by:** TAG_ID_SYSTEM_BUGS.md, which audits the shipped system and finds its
  remaining defects (notably that the `HitTestTag` enum is bypassed by raw bit-fiddling in the hot
  paths — its BUG-7).
- **Still open:** the doc's §9 ("why does `perform_scrollbar_hit_test()` find a scrollbar when
  none is visible?") was never written up as resolved, though the body-sizing chain it suspected
  is fixed at `cache.rs:2199`. Phase-3 `ResizeHandle` was never added (no `ResizePosition` type).
- **Research value:** low-medium — the transferable idea is "carve a namespace out of the
  renderer's opaque tag's *second* field rather than bit-stealing from the payload", which is a
  one-paragraph note; the surrounding migration plan is spent.

---

#### scripts/TAG_ID_SYSTEM_BUGS.md

- **Verdict:** RESEARCH — tag-id/hit-test architecture + the manager-remap invariant; 7/7 bugs fixed, design critique still live.
- **Was:** A systematic audit (2026-02-12) of TagId generation and the hit-test pipeline: the 11
  criteria under which `CssPropertyCache::restyle()` grants a tag, the double storage
  (`StyledNode.tag_id` *and* `StyledDom.tag_ids_to_node_ids`), and the **three inconsistent
  encoding strategies** across namespaces (TagId indirection for 0x0100; direct
  `(DomId<<32)|NodeId` for 0x0200/0x0400; hash indirection for 0x0500). Lists 7 bugs (BUG-1
  HoverManager not remapped after DOM regeneration → wrong nodes get `:hover`/`:active`; BUG-2
  GestureAndDragManager not remapped; BUG-3 PendingContentEditableFocus not remapped; BUG-4 O(n)
  linear tag lookup per hit item; BUG-5 negative/blacklist tag-type filter; BUG-6 0x0300 dead
  code; BUG-7 the `HitTestTag` enum bypassed by raw bit manipulation) plus §6, a proposal to
  delete `StyledNode.tag_id` and go all-direct-encoding.
- **Landed:** all 7 fixed, and BUG-1/2/3 got a *structural* fix better than the doc's ask.
  - BUG-1/2/3: `LayoutWindow::remap_node_ids` (`layout/src/window.rs:9422`) destructures `Self`
    **exhaustively** into three commented buckets — "NODE-KEYED managers implementing
    `NodeIdRemap`" (includes `hover_manager:9431` and `gesture_drag_manager:9428`),
    "NODE-KEYED plain caches", and "EXEMPT" with a per-field justification. Adding a field to
    `LayoutWindow` now fails to compile until it is classified, so the whole *class* of bug is
    closed, not the three instances. `pending_contenteditable_focus`
    (`layout/src/managers/focus_cursor.rs:64`) is remapped at `:200-211`.
    Entry point `update_managers_with_node_moves` at
    `dll/src/desktop/shell2/common/layout.rs:1076`.
  - BUG-4 + BUG-5: both fixed at `dll/src/desktop/wr_translate2.rs:966-988`, with the bug IDs
    cited in the comments ("BUG-4 fix: Build a HashMap for O(1) tag→node lookup"; "BUG-5 fix: Use
    positive filter (== TAG_TYPE_DOM_NODE) instead of negative blacklist").
  - BUG-6: `TAG_TYPE_SELECTION` (0x0300) still defined at `core/src/hit_test.rs:402` but now
    *documented* as unused (`:399` "NOTE: Text selection hit-testing currently uses
    `TAG_TYPE_CURSOR` (0x0400)") — resolved as "keep + document" rather than deleted.
- **Superseded by:** n/a — it supersedes HIT_TEST_TAG_ANALYSIS.md.
- **Still open:**
  - **BUG-7 (design):** unchanged. `HitTestTag`'s encode/decode (`core/src/hit_test.rs:605-690`)
    exists, but `wr_translate2.rs:849-988` and `compositor2.rs:1017` still do raw
    `i.tag.1 & 0xFF00` comparisons and hand-built `(u64, u16)` tuples — encode and decode remain
    two places that must be kept in sync by hand, exactly the fragility the doc named.
  - **§5.1/§6 (double storage):** unchanged. `StyledNode.tag_id` (`core/src/styled_dom.rs:779`)
    and `StyledDom.tag_ids_to_node_ids` (`:838`) both still exist, with the shifting logic at
    `:1303-1308` (`append_child`) and `:1519` (`restyle`) that the doc calls out as error-prone;
    `layout/src/window.rs:3611-3613` and `display_list.rs:4855` still do `.iter().find(...)`
    linear scans (only the WebRender hit path got the HashMap).
  - **§5.2 (three encoding strategies):** unchanged — still TagId-indirect / direct / hash-indirect.
- **Research value:** high, on two counts. (a) The tag-id/hit-test architecture write-up — the 11
  tag criteria, the namespace table with each namespace's encoding *and* indirection strategy, and
  the end-to-end mouse-event → callback pipeline — is the only place this is written down and it
  is still accurate modulo `hit_test_tag.rs` → `hit_test.rs`. (b) The
  **manager-remap-after-DOM-regeneration invariant** and its exhaustive-destructuring enforcement
  is a genuinely transferable technique (same family as the coordinate-newtype in
  SCROLL_COORDINATE_ARCHITECTURE: make the compiler ask the question you keep forgetting).
  Best `scripts/research/` candidate in this cluster alongside SCROLL_COORDINATE_ARCHITECTURE.md.

---

### Cluster summary

| Verdict | Files |
|---|---|
| DELETE (7) | scroll3.md, scroll4.md, scroll5.md, scroll6_report.md, SCROLLBAR_BUGS.md, CURSOR_AND_TEXT_HIT_TEST_ANALYSIS.md, CURSOR_HIT_TEST_ARCHITECTURE_REPORT.md, HIT_TEST_TAG_ANALYSIS.md → **8** |
| RESEARCH (3) | SCROLL_ARCHITECTURE.md, SCROLL_COORDINATE_ARCHITECTURE.md, TAG_ID_SYSTEM_BUGS.md |
| ACTIVE (1) | SCROLL_CURSOR_TEXT_INPUT_ARCHITECTURE.md |
| ARCHIVE (0) | — |

(8 DELETE + 3 RESEARCH + 1 ACTIVE = 12.)

**Which conclusion won the scroll3→4→5→6 chain:** scroll3's *timer-based* architecture —
`ScrollManager` as a pure input recorder feeding a single per-window reserved-ID timer that
computes physics and emits `CallbackChange::ScrollTo`. scroll4's competing render-loop
`physics_tick(dt)` and `window.rs::update()` polling lost; only its formulas and its
finite-containing-block safeguard survived. scroll5 added the ruling that VirtualView
re-invocation happens in the ScrollTo *consumer* so physics stays DOM-agnostic. scroll6 verified
the result and is the terminal doc.

**Overlap with the 2026-07-31 seam audit:** yes, and it closed the leftovers. Scroll chaining —
absent from all six scroll docs — is now real (`layout/src/scroll_timer.rs:381` "MWA-C-scroll:
transfer residual momentum up the scroll chain"). The auto-scroll timer's `OutOfWindow` handling
(`event.rs:355-364`, "MWA-B8") closes SCROLLBAR_BUGS' S10+S11 in one stroke. Neither is described
in any doc here, so the docs understate how much is done.

**Concrete leftovers, ranked:**
1. `core/src/ua_css.rs:834` — `(Html, Height) => HEIGHT_100_PERCENT` disabled since 2026-06-02
   pending a `get_ua_property` jump-table dispatch fix. This is the foundation of
   SCROLL_ARCHITECTURE §2.1 and is currently absent.
2. No integration test asserting frame-relative coordinates for every `DisplayListItem` inside a
   scroll frame (SCROLL_COORDINATE_ARCHITECTURE Approach 3) — the guard that keeps the fixed bug
   from returning on the next new variant.
3. TAG_ID BUG-7 / §5.1: `HitTestTag` bypassed by raw bit manipulation in `wr_translate2.rs` /
   `compositor2.rs`; `StyledNode.tag_id` still duplicated against `tag_ids_to_node_ids`, with
   linear scans remaining at `window.rs:3613` and `display_list.rs:4855`.
4. Code-editor widget (SCROLL_CURSOR §5/§8) not started.
5. Cosmetic: `layout/src/event_determination.rs:40,54` reference the deleted `record_sample()`.


## Part 06 — text layout, text input, selection, fonts, hinting

Audit date 2026-08-01, branch `master`. Every status line in the docs was re-checked
against the tree; nothing below is trusted from the doc itself.

---

#### scripts/TEXT_INPUT_ARCHITECTURE_V4.md

- **Verdict:** DELETE — every prescribed change is present in the macOS shell.
- **Was:** 2026-03-30 analysis of the macOS text-input dual update path. Diagnosed that
  `convert_process_result()` escalated `ShouldUpdateDisplayListCurrentWindow` into a full DOM
  rebuild, so the layout callback re-emitted the pre-edit text and clobbered every keystroke.
  Prescribed a 3-level `EventProcessResult` (`RequestRedraw` / `UpdateDisplayList` /
  `RegenerateDisplayList`) plus a fixed `ProcessEventResult` → level mapping. Also flagged three
  side bugs: missing `NSTextInputClient` conformance, cursor initialised at end instead of click
  point, blink timer possibly lost across DOM reconciliation.
- **Landed:** `dll/src/desktop/shell2/macos/events.rs:74-92` — the enum now has `DoNothing`,
  `RequestRedraw`, `UpdateDisplayList`, `RegenerateLayoutIncremental`, `RegenerateDisplayList`,
  `CloseWindow` (one level MORE than the doc proposed). `convert_process_result` at `:99-115` maps
  `ShouldUpdateDisplayListCurrentWindow → UpdateDisplayList`; handler at `:735-742` splits
  `RegenerateDisplayList` (frame_needs_regeneration) from `UpdateDisplayList` (display_list_dirty).
  `NSTextInputClient` is declared for both views: `macos/mod.rs:1172` (GLView) and `:1867`
  (CPUView). `ProcessTextSelectionClick` exists as a `CallbackChange`
  (`layout/src/callbacks.rs:477`, dispatched at `dll/.../common/event.rs:2629`); cursor-blink timer
  logic lives in `layout/src/managers/text_edit.rs` + `layout/src/window.rs`.
- **Superseded by:** the doc's own `ShouldIncrementalRelayout → UpdateDisplayList` row was
  rejected — the code deliberately routes it to a dedicated `RegenerateLayoutIncremental` variant
  (comment at `events.rs:105-109` explains why collapsing was wrong: it never re-ran layout).
- **Still open:** the residual `TODO(superplan g6)` at `events.rs:65-72` — this macOS-local enum is
  still a lossy projection of `azul_core::events::ProcessEventResult`
  (`UpdateHitTesterAndProcessAgain` and `ShouldRegenerateDomAllWindows` still collapse), kept
  because ~40 match sites in `macos/mod.rs` consume it. That TODO is tracked in-code, not here.
- **Research value:** none — platform-specific bug write-up.

---

#### scripts/TEXT_INPUT_IMPLEMENTATION_PLAN_V3.md

- **Verdict:** DELETE — the entire plan is implemented; the file-location appendix is stale.
- **Was:** 2026-01-28 definitive plan for the text-input system, built on a dual layout path
  (initial layout on `StyledDom`, relayout on `LayoutCache` so quick edits survive). Specified
  `TextConstraintsCache` + `dirty_text_nodes` on `LayoutWindow`, an enhanced `PendingTextEdit`,
  the `record_input → synthetic Input event → user callback (with `prevent_default`) →
  `apply_text_changeset` → `update_text_cache_after_edit` → relayout` flow, cursor hit-testing,
  and a 7-step / 5-day implementation schedule.
- **Landed:** `TextConstraintsCache`, `DirtyTextNode`, `dirty_text_nodes`, `text_constraints_cache`
  all in `layout/src/window.rs`; `TextInputManager` in `layout/src/managers/text_input.rs`;
  `apply_text_changeset` / `update_text_cache_after_edit` in `layout/src/window.rs` and consumed
  by `dll/src/desktop/shell2/common/event.rs`, `macos/events.rs`, `common/layout.rs`, and
  `layout/src/e2e/runner.rs`; `get_text_changeset()` in `layout/src/callbacks.rs` and used by the
  `text_input` / `text_area` / `combobox` widgets; `prevent_default` in `core/src/events.rs`.
  End-to-end coverage exists at `layout/tests/contenteditable_e2e.rs`.
- **Superseded by:** n/a — but the appendix's file paths are wrong now: `event_v2.rs` was renamed
  to `dll/src/desktop/shell2/common/event.rs`, `managers/cursor.rs` is `managers/focus_cursor.rs`,
  and the named helpers `hit_test_text_at_point` / `relayout_dirty_nodes` never existed under
  those names (the functionality lives in `UnifiedLayout::hittest_cursor` and the
  `update_text_cache_after_edit` path).
- **Still open:** the doc's own "Future Work (V4+)" list is largely done (multi-node selection,
  IME, undo/redo, RTL all shipped); inline images pasted from the clipboard into an editable IFC
  are the only item with no code (`layout/src/text3/edit.rs` handles non-text runs on delete but
  there is no clipboard-image insert path).
- **Research value:** none — the "optimistic quick-edit vs committed StyledDom" split is durable,
  but it is already documented in the overlay/changeset memory note and in-code.

---

#### scripts/TEXT_SELECTION_ARCHITECTURE.md

- **Verdict:** RESEARCH — the anchor/focus + DOM-order selection model, incl. the browser
  comparison table, is transferable and outlived the specific structs it proposed.
- **Was:** 2026-01-20 planning doc. Documented the W3C Selection API model
  (anchorNode/anchorOffset/focusNode/focusOffset/isCollapsed), the DOM-order-vs-visual-order rule
  (browsers select in DOM order but highlight in visual order), per-platform behaviour
  (macOS/Windows/X11 primary selection/touch handles), and the logical-selection-rectangle
  algorithm. Proposed `TextSelection` / `SelectionAnchor` / `SelectionFocus` /
  `NodeSelectionRange` / `NodeSelectionType` plus `compute_affected_nodes()`.
- **Landed:** the model landed, the exact type names did not. `core/src/selection.rs` is the
  home of the selection state; `layout/src/managers/selection.rs` exists (the doc predicted it
  might need creating); rendering is `paint_selection_and_cursor` in
  `layout/src/solver3/display_list.rs`; per-line/per-bidi-segment rects come from
  `UnifiedLayout::get_selection_rects` (`layout/src/text3/cache.rs:4892`). The named symbols
  `SelectionAnchor`, `SelectionFocus`, `NodeSelectionRange`, `NodeSelectionType` and
  `compute_affected_nodes` do NOT exist anywhere in the tree — a different decomposition won
  (cursor/affinity pairs over `GraphemeClusterId` inside one IFC, multi-cursor via
  `MultiCursorState::move_all_cursors` at `core/src/selection.rs:489`).
- **Superseded by:** `scripts/report-selection.md`'s IFC-membership design (the actual
  hit-test/lookup mechanism) and the text3 cluster-id selection model in
  `layout/src/text3/selection.rs` + `cache.rs`.
- **Still open:** two edge cases from §5 have no code: `writing-mode: vertical-*` selection axis,
  and "select a non-text inline (image/inline-block) as an atomic unit" — `text3/edit.rs`
  deletes covered non-text runs but there is no atomic-unit selection/copy semantics.
- **Research value:** the anchor/focus vs. start/end distinction, "select in logical order,
  highlight in visual order", and the platform-behaviour matrix are the durable parts —
  reusable for any future caret/selection work.

---

#### scripts/TEXT_UNDERLINE_SKIP_INK_ANALYSIS.md

- **Verdict:** ACTIVE — the analysis is sound and NOT ONE line of skip-ink exists in the tree.
- **Was:** 2026-02-02 analysis of `text-decoration-skip-ink: auto`. Four phases: (1)
  `OwnedGlyph::has_descender()` / `get_underline_intersection()` using the glyph bounding box,
  (2) `calculate_underline_segments()` + `merge_overlapping_ranges()` producing gap-split
  segments, (3) push one `Underline` item per segment, (4) optional
  `TextDecorationSkipInk` CSS enum. Notes the deliberate design choice: bounding-box gaps
  (Chrome's approach) rather than true outline intersection (Firefox's).
- **Landed:** only the pre-existing baseline. `push_underline` at
  `layout/src/solver3/display_list.rs:1856`, emitted from `:4441-4470` as ONE continuous rect per
  glyph run (`let needs_underline = glyph_run.text_decoration.underline || glyph_run.is_ime_preview`).
  The one "easy win" the doc listed DID land: `TEXT_DECORATION_UNDERLINE` is live at
  `core/src/ua_css.rs:433` and wired to `<a>`/`<u>` at `:655`/`:663`.
  `rg 'skip_ink|SkipInk|has_descender|calculate_underline_segments|UnderlineSegment'` over all
  `*.rs` and `api.json` returns ZERO hits.
- **Superseded by:** n/a.
- **Still open:** all four phases. Estimated ~260 LOC by the doc. Note the display-list side
  already supports N `Underline` items per run (and `clip_text_decoration_item` handles them at
  `display_list.rs:7917`), so phase 3 is genuinely cheap once phases 1-2 exist. The CSS property
  `text-decoration-skip-ink` is also unparsed, so api.json/codegen work is implied.
- **Research value:** the Chrome-bbox vs Firefox-outline trade-off and the descender-character
  table are worth keeping with the plan; the plan itself is still executable, so keep it as
  ACTIVE rather than moving it to research.

---

#### scripts/TEXT3_HINTING_REVIEW_2026_07_06.md

- **Verdict:** RESEARCH — the divergence-class → suspect-code map and the CoreText-parity
  methodology are the most reusable text artefact in this cluster; the bug list itself is closed.
- **Was:** 2026-07-06/07 review of text3 + the allsorts hinting fork. Built a deterministic fake
  TTF (`layout/tests/common/fakefont.rs`), three "brutal" spec-first batteries, and a CoreText
  autoregression harness. Recorded 9 candidate defects (C1–C9) from 20 spec-first failures, then a
  multi-agent scan (10 finders / 49 verifiers, 2.4M tokens) that produced **46 CONFIRMED + 3
  refuted** findings. Ends with a "FINAL OUTCOME — fix wave complete" table claiming 44 fixed / 2
  deferred, and 3 remaining failures attributed to unimplemented UBA rule L2.
- **Landed:** verified, and MORE than the doc claims.
  - C5 spacing quantization: `Spacing::PxF(f32)` added at `layout/src/text3/cache.rs:3859-3866`;
    `solver3/getters.rs:2938/2950/2963/2975` now emit `PxF`, the `.round() as i32` casts are gone.
  - C4 NBSP + hyphen breaking: `layout/src/text3/knuth_plass.rs:153-186` emits zero-width
    penalties after U+002D/U+2010 mid-word, with tests at `:1116-1118`.
  - min-content: `knuth_plass.rs:326-338` short-circuits `AvailableSpace::MinContent` to break at
    every legal opportunity.
  - The 11 TrueType-interpreter bugs are fixed in the PUBLISHED `allsorts-azul 0.17.1`
    (`~/.cargo/registry/.../allsorts-azul-0.17.1/src/hinting/`): per-glyph
    `round_state = RoundState::Grid` reset (`interpreter.rs:490`), CVT bound
    `if i > 10_000 → InvalidCvtIndex` (`:999`), DELTAP point-index-first pop (`:2612-2613`),
    SROUND phase from `period/4` (`graphics_state.rs:198-205`), `F_dot_P` clamp to `0x4000`
    (`interpreter.rs:1644-1646`), stack headroom `max_stack_elements + 32` (`:292-294`),
    MIRP cut-in gated on `zp0 == zp1` (`:1989`), ISECT near-parallel rejection (`:2533`),
    SHZ untouched shift (`:2194`), phantom `pp1.x = xMin - lsb` via `hint_glyph_with_flags_pp1`
    (`hinting.rs:380-418`). 17 golden tests in `hinting/tests.rs`.
  - The three tests the doc left failing are now covered: `apply_l2_visual_reversal` exists at
    `layout/src/text3/cache.rs:8642` and is called per line at `:9087`; `get_selection_rects`
    now emits one rect per bidi/visual segment (`cache.rs:4892` + segment loop ~`:4980`). No
    `#[ignore]` on `hebrew_run_is_rtl_reversed_and_33px_wide`,
    `bidi_mixed_run_is_80px_and_reverses_hebrew`, or
    `bidi_selection_over_rtl_run_splits_into_multiple_rects`.
- **Superseded by:** n/a — this IS the superseding document for the per-area fix briefs.
- **Still open:** (a) the stale `TODO(text3-review)` comment at `cache.rs:7155` claims L2 is not
  applied — it now is, at the line level; the comment should be reworded or removed.
  (b) `layout/tests/test_coretext_compare.rs` still exists; the doc says it is superseded by
  `coretext_autoregression.rs` and should be deleted or fixed. It is `#![cfg(all(target_os =
  "macos", feature = "coretext_tests"))]` so it never builds in CI — dead weight, not a bug.
  (c) The `cpurender/raster.rs` text-gamma gap (CoreText applies a text gamma even with smoothing
  off; we use linear agg coverage) was a fidelity flag, not one of the 46, and I found no gamma
  LUT in the CPU glyph fill path.
- **Research value:** HIGH and specific — the **divergence-class → suspect-code map**
  ("over-ink everywhere → gamma/coverage"; "1px vertical shift, rms_aligned ≪ rms_raw →
  phantom-point/baseline rounding"; "stems 1px too far at small ppem → CVT cut-in/round state";
  "identical to unhinted → hinting not running"). That maps a *visual symptom* to a *code
  location* across shaping/hinting/rasterization and is reusable by anyone chasing text parity
  against CoreText/DirectWrite/FreeType. The fake-font-with-pinned-metrics methodology
  (upem 1000, a-z = 600u, kern(A,V) = −100u, so every expected value is arithmetic) is the other
  keeper.

---

#### scripts/text3_review/fix_HINT.md

- **Verdict:** DELETE — all 11 fixes verified present in the published allsorts-azul 0.17.1.
- **Was:** Per-cluster fix brief (11 confirmed bugs) for the TrueType bytecode interpreter in the
  then-vendored `third_party/allsorts`: per-glyph `round_state` leak, WCVTP/WCVTF unbounded CVT
  resize (OOM), DELTAP swapped pops, SROUND phase hardcoded 16/32/48, FLIPRGON/FLIPRGOFF
  unchecked range, stack with no headroom over `maxStackElements`, `move_point` dropping the move
  when F·P == 0, MIRP cut-in gating, SHZ poisoning IUP by marking points touched, ISECT
  near-parallel, phantom `pp1.x` hardcoded to 0. Each entry carries a FreeType-referenced repro.
- **Landed:** see the evidence list in the HINTING_REVIEW section above — every item confirmed at
  a specific line in `~/.cargo/registry/src/index.crates.io-*/allsorts-azul-0.17.1/src/hinting/`.
  `third_party/allsorts` no longer exists in the repo; root `Cargo.toml:69-75` documents that the
  vendored `[patch]` was exactly how azul-layout 0.0.9 shipped broken (the tree grew
  `hint_glyph_with_flags_pp1` while published 0.17.0 kept the same version) and that the fork is
  now consumed from the registry as 0.17.1.
- **Superseded by:** `scripts/TEXT3_HINTING_REVIEW_2026_07_06.md` (final outcome table) and the
  upstreamed crate.
- **Still open:** none. The fixes now live in a published crate, so the briefs are not even a
  local reference any more.
- **Research value:** none beyond the review doc — the FreeType-divergence rationale worth keeping
  is already summarised there.

---

#### scripts/text3_review/fix_KP.md

- **Verdict:** DELETE — all 4 Knuth-Plass fixes landed with regression tests.
- **Was:** 4 confirmed bugs in `layout/src/text3/knuth_plass.rs`: (1) CRITICAL — no terminal
  forced break, so a paragraph ending in a word collapses to one line; (2) hyphen break
  opportunity only fired when the hyphen was the first item of a run; (3) trailing space included
  in line width and justification gap count; (4) MinContent mapped to `f32::MAX/2`, making
  min-content == max-content for `text-wrap: balance`.
- **Landed:** (1) terminal `Glue + Penalty(-INFINITY_BADNESS)` appended — see `knuth_plass.rs:290`,
  `:308-314`, and the guard tests `bug1_terminal_break_wraps_word_ending_paragraph` (`:742`),
  `convert_appends_exactly_one_terminal_forced_break` (`:1049`),
  `convert_does_not_duplicate_terminal_break_after_an_explicit_break` (`:1060`).
  (2) mid-word hyphen handled in the general cluster loop at `:179-186` with a `+spec:` citation
  and tests at `:1116`. (3) trailing word-separator handling in `position_lines_from_breaks`
  (`:571-602`). (4) MinContent early-return breaking at every Penalty at `:326-338`, with a
  defensive fallback comment at `:354-356`.
- **Superseded by:** n/a.
- **Still open:** none.
- **Research value:** none — the "Knuth-Plass needs an explicit end-of-paragraph
  `\penalty -inf`" lesson is now encoded in the code comment plus three named tests.

---

#### scripts/text3_review/fix_RENDER.md

- **Verdict:** DELETE — all 3 fixes landed.
- **Was:** 3 confirmed bugs: (1) bare arrow key with an active Range ran `move_fn(&r.end)` and
  stepped one unit past the selection edge instead of collapsing to it; (2) hinted glyphs
  rasterized at the ROUNDED integer ppem while the unhinted fallback used the fractional size
  (mixed-size glyphs in a run, ~3.7% error at 13.5px, and a visible "wobble" when animating
  font-size); (3) `build_hinted_path` fed PRE-hinting on-curve flags into the path builder, so
  FLIPPT/FLIPRGON/FLIPRGOFF changes were dropped and contours kinked.
- **Landed:** (1) `core/src/selection.rs:520-530` — the non-extend Range arm now probes the move
  direction and collapses to max/min boundary with no motion, with an explanatory comment; tests
  at `:1680`, `:1692`. (2) `layout/src/glyph_cache.rs:189-265` introduces `hint_correction =
  effective_px / ppem` and applies a `TransAffine::new_scaling_uniform(hint_correction)` rescale
  when the effective size is fractional, keeping pixel-grid snapping otherwise; the correction is
  part of the cache key at `:233`. (3) `glyph_cache.rs:377-392` calls
  `hint.hint_glyph_with_flags_pp1(...)` and captures POST-hinting on-curve flags — the comment
  explicitly names FLIPPT/FLIPRGON as the reason.
- **Superseded by:** n/a.
- **Still open:** none from this brief. Adjacent and NOT covered: `hint_light_enabled()`
  (`glyph_cache.rs:417-432`) is a later addition implementing CoreText-style light hinting
  (grid-fit Y only, fractional X) — a design decision made after this brief, ON by default.
- **Research value:** thin, but the "hinted-at-integer-ppem vs unhinted-at-fractional-size"
  mismatch class is a real cross-engine trap; it is now documented in-code at
  `glyph_cache.rs:208-216`.

---

#### scripts/text3_review/fix_SHAPE.md

- **Verdict:** DELETE — all 10 fixes landed; spot-verified 5 at line level.
- **Was:** 10 confirmed bugs across `text3/default.rs`, `edit.rs`, `glyphs.rs`, `script.rs`,
  `selection.rs`: byte offset overloaded onto `RawGlyph.liga_component_pos` (breaking allsorts
  mkmk/marklig), glyphs past byte 65535 silently dropped, any font-feature setting wiping the
  default Latin ligature mask, hinted advance at rounded ppem vs fractional offsets, multi-cursor
  edits leaving stale `source_run` indices, delete of a selected non-text item a no-op,
  tate-chu-yoko `CombinedBlock` glyphs stacked at one x, `is_hangul` swallowing Halfwidth/
  Fullwidth Forms and Enclosed CJK, double-click word selection concatenating in visual order
  within one line, ligature `logical_byte_len` covering only its first component.
- **Landed:** `default.rs:769-782` — `liga_component_pos` explicitly left at allsorts' managed
  default 0 with a NOTE; `:778` comment records the >65535 drop and the side-channel replacement;
  `:876` records the `unicodes`-consuming replacement for the removed overload.
  `glyphs.rs:143-154` feeds the WHOLE `CombinedBlock` slice to `process_glyphs` in one call
  (comment names the per-glyph pen reset as the old bug). `script.rs:486-498` — `is_hangul` now
  excludes U+3200..32FF, U+FF00..FF60, U+FF61..FF9F, U+FFE0..FFEF and keeps only
  U+FFA0..FFDC halfwidth Hangul, with the misclassification written into the comment.
  `selection.rs:137-150` — word selection collects the whole `source_run`, sorts by
  `source_cluster_id.start_byte_in_run` (logical order), comment at `:126` names bidi as the
  motivation.
- **Superseded by:** n/a.
- **Still open:** none found.
- **Research value:** none — the transferable bit ("never overload a shaper's internal
  cluster/ligature field to smuggle byte offsets") is captured in the code comment.

---

#### scripts/text3_review/fix_TEXTLAYOUT.md

- **Verdict:** DELETE — all 18 fixes landed; spot-verified 6.
- **Was:** the largest brief — 18 confirmed bugs. CRITICAL: pixel `line-height` decoded to a
  NEGATIVE per-run line height through the compact-cache fast path. HIGH: cursor-move affinity
  swallowing a keypress; min-content 0 for CJK/break-all; leading whitespace stripped for
  `white-space: pre`; `overflow-wrap: normal` shredding a long word one grapheme per line;
  letter/word-spacing excluded from break-width and alignment; NBSP treated as a break
  opportunity. MEDIUM/LOW: IFC cache key omitting container constraints, sub-pixel spacing
  quantization, intrinsic measurement with default constraints, em/rem against 16px,
  `layout_hash` rounding font-size, single-rect bidi selection, no caret for empty text, trailing
  spaces stripped for `pre`, nondeterministic HashMap font fallback, no `.notdef` tofu.
- **Landed:** spacing → `Spacing::PxF` (`cache.rs:3859`, `getters.rs:2938+`);
  IFC cache key now hashes container constraints (`solver3/fc.rs:2703-2704` hashes
  `text_align`/`text_align_last`, part of the `#11 fix` block at `:2683-2690`); bidi selection
  now segments per direction (`cache.rs:4892` + segment vec `Vec<(f32,f32,BidiDirection)>` with a
  comment naming the endpoint-to-endpoint over-cover bug); `get_cursor_rect` at `:5106` resolves
  caret edges from the cluster's own bidi direction; strut fields
  (`strut_ascent`/`strut_descent`/`strut_x_height`, `cache.rs:1727-1789`) back the empty-line
  fallback; NBSP/min-content/overflow-wrap fixes are covered by the KP + brutal-battery evidence
  above. The compact-cache line-height fast path was rewritten (`solver3/getters.rs:~2690`).
- **Superseded by:** n/a.
- **Still open:** none from the brief itself. Related open item: `solver3/fc.rs` Phase **2c**
  (ContentIndex-based refinement of the incremental-IFC decision) is still marked "will refine"
  at `layout/src/solver3/layout_tree.rs:311` and `:319`; Phase 2d IS implemented
  (`fc.rs:2767 === Phase 2d: IFC incremental relayout decision tree ===`).
- **Research value:** none as a document; the individual fixes carry `+spec:` citations in-code.

---

#### scripts/text3_review/confirmed_findings.json

- **Verdict:** DELETE — machine-readable duplicate of the five fix briefs; every row resolved.
- **Was:** 20KB JSON array, one object per confirmed finding with `sev`, `file`, `line`, `area`,
  `title`, `fix`. It is the *source* the five `fix_*.md` briefs and the review's "Finder findings"
  section were rendered from — same 46 rows, same wording, plus the `area` cluster tag
  (`solver3-integration`, `line-breaking`, `shaping-cache`, `edit-selection`, `hint-arith`,
  `hint-move`, `hint-glue`, `glyphs-script`, `layout-glue-raster`).
- **Landed:** the file has NO status field — nothing in it records resolution, so it cannot be
  trusted as a tracker. Resolution was verified independently per brief above; the review's
  outcome table (44 root-caused, 2 low-risk deferrals) plus the L2/selection-split work that
  landed afterwards closes the set. The two named low-risk deferrals were the RTL-L2-dependent
  items, which are now implemented (`cache.rs:8642`, `:9087`).
- **Superseded by:** the five `fix_*.md` briefs (human-readable) and the review's FINAL OUTCOME
  table.
- **Still open:** none. Line numbers in the JSON are already stale (the tree has moved by
  hundreds of lines in `cache.rs`), so it has negative value as a lookup table.
- **Research value:** none — but the *format* (sev/file/line/area/title/fix, one row per
  adversarially-verified finding, refuted rows kept separately) is a decent template for future
  multi-agent audits.

---

#### scripts/TRUETYPE_HINTING_PLAN.md

- **Verdict:** DELETE — fully executed; the resulting interpreter shipped in allsorts-azul 0.17.1.
- **Was:** 2026-05-02 plan to implement TrueType bytecode hinting, motivated by azul text reading
  thinner/wider/less crisp than Chrome. Explains what hinting is (a pre-rasterization outline
  transform in F26Dot6), documents the four tables (`cvt`, `fpgm`, `prep`, `gasp`) and their
  then-status in allsorts (parsed / tag-only / dropped-on-subset), specifies the stack VM
  (~200 opcodes, ~30 graphics-state variables, twilight zone, FDEF/IDEF), a 4-phase plan, and a
  scope estimate (~500-800 lines for parsing, ~3000-5000 for the interpreter vs FreeType's ~10K
  lines of C). Explicitly says "implement in allsorts, on the `pixelsnap` branch".
- **Landed:** in full. The interpreter is `allsorts-azul 0.17.1`
  (`src/hinting.rs` + `src/hinting/{f26dot6,graphics_state,interpreter,tests}.rs`), consumed by
  azul at `layout/src/glyph_cache.rs:295-415` (`build_hinted_path`): `parsed_font.hint_instance`
  (`:341`), `allsorts::hinting::f26dot6::compute_scale(ppem, upem)` (`:355`),
  `hint.hint_glyph_with_flags_pp1(...)` (`:382`). The Phase-3 "wr_glyph_rasterizer change" was
  NOT where it ended up — `webrender/glyph/` has no hinting at all
  (`webrender/glyph/src/lib.rs:31` explicitly disclaims "advanced hinting that relies on native
  OS libraries"); the integration went into azul's own `layout/src/glyph_cache.rs` +
  `cpurender/raster.rs` instead.
- **Superseded by:** `scripts/TEXT3_HINTING_REVIEW_2026_07_06.md` (which audited the resulting
  interpreter against FreeType v40 and fixed 11 bugs in it) and by the shipped crate.
- **Still open:** two plan items are unimplemented and are real gaps:
  (a) **`gasp` consultation** — the plan's "consult gasp for this ppem → should we hint?" step.
  `rg 'gasp'` finds nothing in azul; the review's finder explicitly *refuted* "no gasp
  consultation" as a bug (`glyph_cache.rs:135`), so this is a deliberate deviation, not an
  oversight — but fonts that disable hinting at some ppem ranges are still hinted.
  (b) **Phase 4 validation vs FreeType output** was replaced by CoreText autoregression
  (`layout/tests/coretext_autoregression.rs`, `scripts/coretext_regression.sh`) which only runs
  on macOS with `--features coretext_tests` — i.e. never in CI.
- **Research value:** MODERATE — it is the clearest prose explanation in the repo of *why*
  unhinted text looks wrong and what each of `cvt`/`fpgm`/`prep`/`gasp`/glyf-instructions does.
  But it is now a design doc for shipped code living in another crate; the review doc is the
  better keeper.

---

#### scripts/FONT_INVALIDATION_AND_MEMORY_LAYOUT_ANALYSIS.md

- **Verdict:** RESEARCH — 4 of its 6 named optimizations were implemented, and the XOR-collision
  argument is the durable, transferable insight.
- **Was:** 2026-03-02 two-part analysis. Part 1 traced `font_stacks_hash`, a window-global XOR of
  per-node `font_family_hash`, used to skip the 5-step font resolution pipeline; argued the XOR is
  theoretically unsound (add + remove the same font in one frame → unchanged XOR → missed
  invalidation) and that per-node font-dirty tracking is missing. Part 2 measured solver3 memory:
  ~1,100-1,200 B/node, called out `LayoutNode` (~550 B AoS monolith, ~80 B hot) and
  `CssPropertyCache`'s `Vec<Vec<…>>` as the two bad patterns, and listed 6 prioritized
  optimizations.
- **Landed:**
  - **P4 (per-node font dirty) — DONE.** `font_dirty_nodes: Vec<usize>` at
    `css/src/compact_cache.rs:1453`, populated by comparing each node's hash against
    `prev_font_hashes` in `core/src/compact.rs:420-428` and `:764-770`. Consumed at
    `layout/src/window.rs:2213-2250`, whose comment names the exact defect this doc predicted:
    *"This replaces the collision-prone global XOR approach: XOR(a,b,a,b) == 0 even though fonts
    changed."* A rolling-hash signature over `prev_font_hashes` was added as a second guard.
  - **P1 (LayoutNode hot/cold split) — DONE.** `LayoutNodeHot` / `LayoutNodeWarm` /
    `LayoutNodeCold` at `layout/src/solver3/layout_tree.rs:663` / `:684` / `:721`, with
    `LayoutNode` (`:444`) documented as "HOT tier … should fit in the first 2-3 cache lines".
  - **IFC incremental relayout — PARTLY DONE.** The doc called it a stub; Phase 2d now exists
    (`layout/src/solver3/fc.rs:2767`, "IFC incremental relayout decision tree") keyed on an
    inline-content hash (`:2683-2690`).
  - **P5 (BTreeMap → HashMap) — PARTLY DONE** for `GpuValueCache` (`core/src/gpu.rs:43+` is all
    `HashMap` now).
- **Superseded by:** n/a for Part 1. Part 2's BTreeMap item overlaps
  `scripts/BTREEMAP_TO_VEC_PLAN.md` (not in my assignment).
- **Still open:**
  - **P6 GpuValueCache consolidation** — still 14 separate maps (`core/src/gpu.rs:43-63+`),
    just HashMap-backed instead of BTreeMap. The proposed single
    `HashMap<NodeId, GpuNodeValues>` does not exist.
  - **P2 CssPropertyCache flat allocation** — the `Vec<Vec<…>>` fallback path is still there
    (the compact cache is the fast path, not a replacement).
  - **P3 LayoutNode.children arena** — `children` is still a per-node `Vec<usize>`.
  - **Per-node font chain RE-RESOLUTION** — `font_dirty_nodes` currently only gates an
    all-or-nothing skip (`window.rs:2229` `font_dirty_count`); when any node is dirty, ALL chains
    are re-resolved. The doc's actual proposal (re-resolve only affected chains) is unimplemented.
  - **IFC Phase 2c** — `layout_tree.rs:311` / `:319` still say "Phase 2c will refine this".
- **Research value:** the XOR-fingerprint unsoundness argument (a commutative, self-inverse
  combiner cannot detect a swap) is a genuinely transferable cache-invalidation lesson, and the
  per-node byte budget table (`LayoutNode ~550 B, hot working set ~80 B, 10K nodes → 12 MB`) is
  the kind of measurement worth keeping to justify future SoA work.

---

#### scripts/report-selection.md

- **Verdict:** RESEARCH — the "layout lives on the IFC root, hit-test tags live on DOM nodes"
  mismatch and its IFC-membership resolution is the load-bearing architectural insight for
  selection, and it is still exactly how the code works.
- **Was:** 2026-01-13 report. States the core problem: `UnifiedLayout` is stored on the IFC ROOT
  (the `<p>`), but the text is in `::text` DOM children which have no `inline_layout_result` and
  are not hit-testable (their rect is `null`). Explains WHY the layout cannot be split per DOM
  node (line breaking is holistic; bidi reorders; inline-blocks share the IFC). Proposes `IfcId` /
  `IfcDomMapping` / `IfcMembership` so a hit-tested node can navigate to its IFC root. Contains a
  4-phase migration path (Phases 1 and 2 marked COMPLETED, Phase 3 debug-server integration
  BLOCKED, Phase 4 selection rendering TODO), a live debug-session log, and 5 open questions.
- **Landed:** the winning design. `IfcId` in `layout/src/solver3/{mod,layout_tree,fc}.rs`;
  `IfcMembership` at `layout/src/solver3/layout_tree.rs` + `fc.rs`; `ifc_membership` consumed by
  `layout/src/window.rs`, `solver3/getters.rs`, and `layout/tests/contenteditable_e2e.rs`;
  `ifc_root_layout_index` used in `window.rs`, `solver3/display_list.rs`, `solver3/fc.rs`.
  Phase 4 landed too: `paint_selection_and_cursor` (`display_list.rs`) and `get_selection_style`
  (`solver3/getters.rs:2359`). Phase 3's blocker is resolved — `GetSelectionState` is served from
  `layout/src/e2e/full.rs` on the persistent LayoutWindow, and selection is covered by
  `layout/tests/text3_selection_exact.rs`, `text3_brutal_selection.rs`,
  `text3_regression_selection_edit.rs`.
  The proposed `IfcDomMapping` / `run_to_dom` table does NOT exist — a leaner
  `ifc_membership { ifc_id, ifc_root_layout_index, run_index }` on the LayoutNode won instead.
- **Superseded by:** it supersedes `scripts/TEXT_SELECTION_ARCHITECTURE.md` on the hit-test/lookup
  mechanism.
- **Still open:**
  - **`::selection` pseudo-element is NOT implemented as a pseudo-element.** Selection styling
    landed as first-class CSS properties (`SelectionBackgroundColor`, `SelectionColor`,
    `SelectionRadius` — `css/src/props/property.rs:47`), resolved in `get_selection_style`
    (`solver3/getters.rs:2359-2389`) with a system-style fallback. `CssPathPseudoSelector`
    (`css/src/css.rs:1748-1772`) has no `Selection` variant, so `p::selection { … }` does not
    parse. `PseudoElement::Selection` appears nowhere. `text-shadow` on selected text is
    likewise unsupported.
  - Non-text IFC items (inline-block, image) as atomic selection units — the report's table
    (§ Non-Text IFC Items) is still a plan, not code.
- **Research value:** HIGH. Two transferable concepts: (1) *why* an inline formatting context is
  the indivisible unit of text layout (holistic line breaking, bidi reordering, inline-block
  participation) and therefore why hit-testing needs a node→IFC-root indirection rather than
  per-node layout; (2) the resolved open questions — IFC ids global-with-per-pass-reset, and
  "store logical cursors only, compute selection rectangles at render time" as the correct way
  to stay RTL/bidi-correct. Both are the kind of decision that costs a week to re-derive.

---

### Cross-cutting notes

- **`scripts/coretext_regression.sh` and `layout/tests/coretext_autoregression.rs` are live
  assets**, gated behind `--features coretext_tests` + `target_os = "macos"` — i.e. never
  exercised by CI. Deleting the review doc must not imply deleting these.
- **`layout/tests/common/fakefont.rs` and the three `text3_brutal_*.rs` batteries are live
  tests** with no `#[ignore]`; they are the executable residue of the 2026-07-06 review.
- **`layout/tests/test_coretext_compare.rs`** is the one file the review itself recommended
  deleting; it is superseded by `coretext_autoregression.rs` and cannot build in CI.


## Part 07 — events, callbacks, DOM diffing, iframes, async, drag&drop

Audited 2026-08-01 against master (f1c43ba60). Every status line in these docs was
re-verified against the tree; several were stale in *both* directions.

**Naming note (asked for explicitly):** `EVENT_ARCHITECTURE_ANALYSIS.md` and
`EVENT_ARCHITECTURE_ANALYSIS_DOC.md` are **not** competing versions of the same doc
despite the names. The first is a W3C DOM-Events *conformance matrix*; the second is a
*brittleness/simplification* proposal (two-enum change model). Neither supersedes the
other; both mostly landed. The `_DOC` suffix is an artifact of commit 88b319b27
("only moving code / files around").

**Global rename:** everything the iframe docs call `IFrame` is now `VirtualView`
(`NodeType::VirtualView` `core/src/dom.rs:639`, `VirtualViewNode` `:1225`,
`layout/src/managers/virtual_view.rs`, `DisplayListItem::VirtualView`
`layout/src/solver3/display_list.rs:772`). No `NodeType::IFrame` exists.

---

#### scripts/EVENT_ARCHITECTURE_ANALYSIS.md

- **Verdict:** RESEARCH — durable W3C-vs-Azul event-model parity matrix; keep the model, not the status.
- **Was:** A section-by-section W3C DOM Events / UIEvents / HTML5-DnD conformance audit of the
  "state-diffing" event system (platform updates window state → `determine_all_events()` synthesizes
  events). Documented the dual-filter model (`HoverEventFilter` = hit-tested, `FocusEventFilter` =
  focused node, `WindowEventFilter` = global) as the deliberate divergence from
  `addEventListener(type, handler, {capture})`. Called out that `propagate_event()` in core existed
  but was never called from the hot path, and that `EventData::None` silently broke every
  button-specific filter.
- **Landed:** Yes, essentially all of it. `propagate_event()` `core/src/events.rs:864` is now driven
  from `dispatch_events_propagated()` `dll/src/desktop/shell2/common/event.rs:3976` (call at `:4073`)
  and mirrored in the e2e runner `layout/src/e2e/runner.rs:978`/`:1054`. Capture→Target→Bubble is real:
  `EventPhase` `core/src/events.rs:156`, phases at `:883`/`:939`/`:898`. Old `dispatch_synthetic_events`
  / `CallbackTarget` are gone — tombstone comment at `core/src/events.rs:2401`. The added filters exist
  on all three enums: `MouseOut` / `FocusIn` / `FocusOut` / `CompositionStart|Update|End` at
  `core/src/events.rs:486,508-522` (Window), `:1787-1801` (Hover), `:2001-2011` (Focus).
  `get_all_hovered_nodes()` `layout/src/event_determination.rs:215` does the full hover-chain diff.
  Mouse events now map to **both** generic and button-specific filters
  (`core/src/events.rs:2501-2511`), and `E::Click => Hover(LeftMouseUp)` `:2517`.
- **Superseded by:** n/a (complementary to `_DOC`).
- **Still open:** (1) `beforeinput` — zero hits repo-wide, genuinely unimplemented.
  (2) `contextmenu` has no dedicated filter: `E::ContextMenu => vec![EF::Hover(H::RightMouseDown)]`
  `core/src/events.rs:2526`, so a right-click callback cannot distinguish "menu requested" from
  "right button pressed". (3) §8 W3C VirtualKeyboard API — `show_virtual_keyboard` / `inputmode` /
  `VirtualKeyboardGeometryChanged` have **zero** hits; the whole section is untouched design.
  (4) §6 Priority 6 "SpatialId-based drag transforms" partially landed as
  `SystemChange::UpdateDragGpuTransform` `core/src/events.rs:2718` — verify it is a spatial transform
  and not still display-list mutation.
- **Research value:** High. The transferable concept is the **filter-category-determines-dispatch-strategy**
  model (Hover/Focus/Window) contrasted against W3C's phase-flag model, plus the full parity table
  (what a non-browser toolkit must synthesize to be W3C-equivalent: click from down+up on same node,
  enter/leave from hover-chain set difference, focusin/focusout as the bubbling twins of focus/blur).
  Strip §3/§5/§6 (stale bug lists, now fixed) and keep §1–§2 + §8.

---

#### scripts/EVENT_ARCHITECTURE_ANALYSIS_DOC.md

- **Verdict:** RESEARCH — the single best doc in this cluster; the frame-lifecycle contract in prose.
- **Was:** Argues the event architecture had two unrelated failure modes: user changes passed through
  four representations (`Vec<CallbackChange>` → `CallbackChangeResult` → `CallCallbacksResult` →
  if-blocks) losing exhaustiveness after step 1, and *system* changes (focus, drag pseudo-states,
  cursor blink, autoscroll, scrollbar) had **no representation at all** — ~500 lines of inline
  if-blocks in a 700-line function. Proposes two enums (`CallbackChange` public/FFI,
  `SystemChange` internal), each consumed by one exhaustive `match`, bundled in a `FrameChanges`
  newtype whose only method is `process()` so a platform physically cannot handle one and skip the
  other. Includes a 30-row catalog of system changes and an explicit "what is NOT a SystemChange"
  section (lifecycle hooks stay put). Adding a user capability: 20 places → 5.
- **Landed:** Phases 1, 3 and 4 fully; phase 2 in substance. `SystemChange` `core/src/events.rs:2645`
  (~35 variants incl. `ActivateNodeDrag`, `UpdateDragGpuTransform`, `SetDragOverState`,
  `StartAutoScrollTimer`, `FinalizePendingFocusChanges`, `ScrollNodeIntoView`), FFI-vec'd at `:2761`.
  Single exhaustive sinks: `apply_system_change()` `dll/src/desktop/shell2/common/event.rs:3157`
  ("SINGLE place where all `SystemChange` variants are handled") and `apply_user_change()` `:1789`.
  Phase-4 deletions are all confirmed by absence: `CallCallbacksResult`, `CallbackChangeResult`,
  `process_callback_result_v2`, `needs_processing`, `should_scroll_render`, `cursor_changed` —
  **zero hits repo-wide**. `process_timers_and_threads()` is a trait default method
  `dll/.../common/event.rs:5915`, called once per backend (windows `:4188`, wayland `:5796`,
  x11 `:5130`, headless `:1848`, android `:495`, ios `:835`, macos `:1081`/`:1741`).
- **Superseded by:** n/a — this doc *supersedes* `CALLBACK_INVOCATION_UNIFICATION.md`
  (its `merge_into`/`CallCallbacksResult` target types were subsequently deleted outright).
- **Still open:** The **`FrameChanges` newtype was never built** — zero hits. That was the doc's
  actual enforcement mechanism; without it, both enums are exhaustive but nothing stops a backend
  from draining one list and not the other. This is precisely the "7 hand-rolled loops, implicit
  contract, no shared enforcement" theme, and the codebase says so itself:
  `dll/tests/backend_feature_parity.rs:1-21` is a **source-scanning** test ("a weak check that goes
  red beats a strong abstraction nobody has written") whose header records two real regressions —
  `process_timers_and_threads()` had zero call sites on iOS+Android (no timers, no thread writeback,
  no animation), and `process_accessibility_actions()` had zero implementations on iOS/Android/headless.
  Its own TODO: "When the trait exists, delete this file."
- **Research value:** Highest in the cluster. Transferable concept: **exhaustive-enum-per-actor plus a
  consume-once newtype as the type-level substitute for a hand-written frame-lifecycle contract**, with
  the §7 "why two enums, not one" rationale (public FFI surface vs internal framework actions have
  different trust levels and lifetimes) and the §6 change-vs-lifecycle-hook distinction. Belongs in
  `scripts/research/` verbatim.

---

#### scripts/CALLBACK_INVOCATION_UNIFICATION.md

- **Verdict:** DELETE — its fix shipped, then its target types were deleted entirely.
- **Was:** Documented 4 near-identical callback invocation paths on `LayoutWindow`
  (timer / thread / single / menu), each copy-pasting a 28-field `CallCallbacksResult` literal, 10
  accumulators, a 12-field `CallbackInfoRefData`, and ~50 lines of field forwarding. §8 is a per-field
  × per-path consistency matrix that found **6 fields silently dropped** on the thread/single/menu
  paths (`image_callbacks_changed`, `update_all_image_callbacks`, `queued_window_states`,
  `text_input_triggered`, …). §9 is a separate per-platform scroll-physics-timer audit finding that
  Windows/X11(×2)/Wayland never called `process_callback_result_v2` for scroll-only results, so
  scrolling was silently broken on three platforms.
- **Landed:** §4.1 `merge_into()` shipped as commit 82679e2bd ("Unify 4 callback paths via
  `CallCallbacksResult::empty()` + `merge_into()`. Fixes 6 missing-field bugs"). Then the whole flat
  struct was removed by the two-enum refactor. §9 fixes landed: the scroll timer now returns
  `Update::DoNothing` — `layout/src/scroll_timer.rs:479` with the exact comment the doc's "Option C"
  recommended — and the per-platform boilerplate collapsed into `process_timers_and_threads()`;
  `check_timers_and_threads` survives only as a thin wrapper (`x11/mod.rs:5128`, `wayland/mod.rs:5794`)
  that calls it.
- **Superseded by:** `scripts/EVENT_ARCHITECTURE_ANALYSIS_DOC.md`.
- **Still open:** The 4 paths *still exist* uncollapsed — `run_single_timer` `layout/src/window.rs:5857`,
  `run_all_threads` `:5948`, `invoke_single_callback` `:6067` (+ `invoke_single_callback_at` `:6104`).
  §4.3 `invoke_and_collect()` and §4.4 `CallbackEnv` were never built (zero hits), so the ~11-parameter
  signatures remain. §5's speculative callback sources (notification / tray / file-watcher) do not exist.
- **Research value:** Low as a document, but the §8 method is worth one paragraph elsewhere: a
  **field × path forwarding matrix** is a cheap way to find silent-drop bugs in any fan-in accumulator.
  The §9 per-platform capability table is the direct ancestor of `dll/tests/backend_feature_parity.rs`.

---

#### scripts/DOM_CHANGE_REPORT_ARCHITECTURE.md

- **Verdict:** DELETE — implemented; only the final integration step is unfinished.
- **Was:** Replace the binary "changed/unchanged" layout gate with a per-node change report so each
  downstream stage does minimum work. Specifies `NodeChangeSet` bitflags (TEXT_CONTENT,
  IDS_AND_CLASSES, INLINE_STYLE_LAYOUT vs INLINE_STYLE_PAINT, CHILDREN_CHANGED, CALLBACKS, DATASET,
  ACCESSIBILITY) with `AFFECTS_LAYOUT` / `AFFECTS_PAINT` composite masks, `ExtendedDiffResult`,
  `compute_node_changes()`, a `CssPropertyType::affects_layout()` classifier, and a 4-phase migration
  ending in replacing `is_layout_equivalent()` at the layout entry.
- **Landed:** Phases 1–3 verbatim in `core/src/diff.rs`: `NodeChangeSet` `:41`,
  `ExtendedDiffResult` `:148`, `compute_node_changes()` `:161`, `ChangeAccumulator` `:1245`,
  `to_dirty_flag()` `:1222`, `reconcile_dom_with_changes()` `:1534`. Dedicated test suites exist
  (`core/tests/reconciliation/dom_reconciliation.rs`, `.../node_change_set.rs`). The doc's
  complaint #1 is resolved: `hash_styled_node_data()` is gone —
  `layout/src/solver3/cache.rs:2495` "removed — replaced by `NodeDataFingerprint::compute()`".
  `DirtyFlag` with Layout > Paint > None ordering at `layout/src/solver3/layout_tree.rs:149-150`.
- **Superseded by:** n/a.
- **Still open:** Phase 4 is not done — the binary gate is still the live entry check:
  `dll/src/desktop/shell2/common/layout.rs:583` calls
  `azul_core::styled_dom::is_layout_equivalent(&old_layout_result.styled_dom, &styled_dom)`, so
  `NodeChangeSet` never gets to prune that path. `CssPropertyType::affects_layout()` as a named method
  does not exist (only a test asserting the mask, `core/tests/reconciliation/node_change_set.rs:281`) —
  the layout/paint classification lives inside `compute_node_changes` instead of on the CSS type.
  Incremental display list (explicitly deferred as "future" in the doc) still not done — the display
  list is fully regenerated.
- **Research value:** Moderate. The transferable concept is **severity-ordered change bitflags with
  composite masks** collapsing to a single `DirtyFlag`, which is React-reconciliation logic pushed down
  into a retained layout tree. Not enough novel rationale to warrant `scripts/research/` on its own.

---

#### scripts/DRAG_DROP_REPORT.md

- **Verdict:** RESEARCH — the HTML5-DnD model comparison is the durable part; ~80% of the plan shipped.
- **Was:** Three gaps for HTML5-compatible DnD: (1) visual feedback — Option A "CSS/GPU transform on
  the real node" vs Option B "browser-style ghost bitmap", recommending A; (2) drop-zone filtering —
  a careful reconstruction of the HTML5 `dataTransfer` protocol including **protected mode** (during
  `dragover` only `.types` is readable, `getData()` only in `drop`) and the fact that
  `preventDefault()` on `dragover` is what makes a node a valid drop target; (3) CSS pseudo-classes —
  notes that CSS **never standardized** `:drag`/`:drop()` (dropped from Selectors 4) so every real
  implementation hand-toggles classes from JS, and argues azul should do better with automatic
  `:dragging` / `:drag-over` / `:drag-over-invalid`.
- **Landed:** Most of it. `NodeDrag.drag_offset` `core/src/drag.rs:116`, set at
  `layout/src/managers/drag_drop.rs:263`. Pseudo-classes are real CSS:
  `CssPathPseudoSelector::Dragging`/`DragOver` (`css/src/css.rs:1767,1816-1817`),
  parsed at `css/src/parser2.rs:453-454` and `css/src/dynamic_selector.rs:1645-1646`,
  cascaded via `PseudoStateType::Dragging` in `core/src/prop_cache.rs:1278,1296`.
  The full §3.2 `dataTransfer` API shipped on `CallbackInfo`: `get_drag_types()`
  `layout/src/callbacks.rs:4020`, `get_drag_data()` `:4041`, `accept_drop()` `:4065`,
  `set_drop_effect()` `:4073`. §4's "events never generated" is fixed — `DragEnter`/`DragOver`/
  `DragLeave`/`Drop` are first-class `HoverEventFilter` variants (`core/src/events.rs`, drag block)
  and drag state is driven by `SystemChange::ActivateNodeDrag` / `SetDragOverState` /
  `UpdateDropTarget` / `UpdateDragGpuTransform` / `DeactivateDrag` (`core/src/events.rs:2705-2720`,
  applied at `dll/.../common/event.rs:3605`, pushed at `:5434`).
- **Superseded by:** n/a (the DnD sections of `EVENT_ARCHITECTURE_ANALYSIS.md` overlap but are thinner).
- **Still open:** (1) `:drag-over-invalid` — **zero hits**; the "did any callback accept this drop?"
  → invalid-styling half of the design was never built, so a rejected drop zone cannot be styled.
  (2) §2 Schritt 4 cursor feedback — no `DropEffect` → `CursorIcon::NoDrop/Copy/Alias` mapping exists
  anywhere; the user gets no cursor signal for a forbidden drop. (3) Option B ghost bitmap /
  `setDragImage` — no `drag_bitmap` anywhere; deliberate, per the doc's own recommendation.
  (4) `DragOver` throttling (~350 ms per HTML5) — not evidenced.
- **Research value:** High and specific: **the HTML5 DnD protocol distilled** (protected mode,
  `effectAllowed` on the source vs `dropEffect` on the target, `preventDefault`-as-accept), plus the
  well-argued case that a toolkit should expose automatic drag pseudo-classes *because* CSS failed to.
  That "azul can be better than HTML here, and here is exactly why HTML is like that" reasoning is
  what makes it worth keeping.

---

#### scripts/click_event_analysis.md

- **Verdict:** DELETE — all three problems fixed; earliest and thinnest of the event docs.
- **Was:** German-language triage (2026-01-07) of "clicks on buttons don't work". Three findings:
  the button widget registered `HoverEventFilter::MouseUp` while dispatch emitted only `LeftMouseUp`;
  the debug API mutated window state without running a hit test or the event pipeline; and there was
  **no bubbling at all** — a click on a `<text>` child never reached the parent button. Includes a
  JS capture/target/bubble reference diagram and a hand-rolled parent-walk bubbling sketch.
- **Landed:** All three. Dispatch now returns **both** the generic and the button-specific filter
  (`core/src/events.rs:2501-2511`), so `button.rs:381`'s `HoverEventFilter::MouseUp` registration is
  now correct as-is — Option B of the doc won, not Option A. Bubbling is the real
  `propagate_event()` Capture→Target→Bubble (`core/src/events.rs:864`), far beyond the sketched
  parent-walk; `stop_propagation` exists as `CallbackChange::StopPropagation` /
  `StopImmediatePropagation`. `dll/src/desktop/shell2/common/debug_server.rs` no longer exists.
- **Superseded by:** `scripts/EVENT_ARCHITECTURE_ANALYSIS.md` (same JS-model comparison, done properly).
- **Still open:** none.
- **Research value:** none unique — the JS phase diagram is reproduced better in
  `EVENT_ARCHITECTURE_ANALYSIS.md`. Historical interest only (it is the commit that motivated
  52be8c4ec "Refactor event filter bubbling to work like in JS").

---

#### scripts/ASYNC_TASK_API_DESIGN.md

- **Verdict:** ACTIVE — unimplemented design with 4 unanswered questions blocking a real widget.
- **Was:** Design (2026-06-10) for `AsyncTask` / `spawn_async` / `cancel_async` on `CallbackInfo`:
  a bounded std-only worker pool (`clamp(cores-1, 1, 8)` threads over a `BinaryHeap` + `Mutex` +
  `Condvar`) layered *on top of* the existing `Thread` writeback drain, so results still arrive
  one-at-a-time on the main thread with **no new drain path**. `work` deliberately receives no
  `CallbackInfo` (off-thread, cannot touch DOM); `on_complete` gets the full one. `i32` priority so
  centre map tiles fetch before edges. Explicitly rejects epoll/kqueue/IOCP reactors as the wrong
  trade for blocking HTTP tile fetches.
- **Landed:** **Nothing.** `AsyncTask`, `AsyncTaskId`, `spawn_async`, `cancel_async`,
  `AsyncWorkCallback` — all zero hits repo-wide. No worker pool in `core/src/task.rs` or
  `layout/src/window.rs`. The status line ("DESIGN — not yet implemented … Build only after
  sign-off") is accurate, which makes it the one doc here whose own status was right.
- **Superseded by:** n/a.
- **Still open:** Everything, plus the motivating defect is still live: `MapWidget` still spawns one
  raw `std::thread` per tile with a burst cap instead of a pool —
  `layout/src/widgets/map.rs:1085` `spawn_pending_tile_fetches`, `MAX_SPAWN_PER_CALL: usize = 16`
  at `:1090` (with tests at `:3655`, `:3674` pinning that cap as *intended* behaviour). No priority
  ordering; a pan/zoom storm still spawns hundreds of OS threads. §5's LRU eviction of
  `MapTileCache.tiles` also not done. The 4 §7 decisions (pool sizing, new primitive vs extending
  `Thread`, cancellation granularity, `i32` vs enum priority) remain unanswered — this needs a
  user ruling before any code.
- **Research value:** Moderate-high: **how to get a priority-scheduled bounded async pool with zero
  async deps by reusing an existing main-thread writeback drain**, plus the explicit argument for why
  a thread pool beats a readiness reactor for this workload. Keep §2 and §4 if archiving.

---

#### scripts/IFRAME_ANALYSIS.md

- **Verdict:** DELETE — every named symbol is gone or renamed; superseded by the investigation report.
- **Was:** German first-pass on why `examples/c/infinity.c` rendered nothing. Three bugs:
  `scan_for_iframes()` read `self.layout_results.get(&dom_id)` *before* that entry was inserted, so
  the `?` in a `filter_map` made it always return empty; `invoke_iframe_callback()` had the same
  read-before-insert race; and no code path ever called `IFrameManager::check_reinvoke()` on a scroll
  event, so `EdgeScrolled` was correct-but-dead. Recommends passing `styled_dom` in as a parameter.
  Contains a useful `infinity.c` vs `infinity.rs` comparison showing the Rust workaround
  (`on_scroll` → `Update::RefreshDom`) is an O(n) full rebuild instead of O(visible).
- **Landed:** The recommended fix, structurally. `scan_for_iframes` no longer exists; it is
  `Self::scan_for_virtual_views(&styled_dom, &tree, &self.layout_cache.calculated_positions)` —
  declared `layout/src/window.rs:2798`, called at `:2696` — i.e. `styled_dom` is passed in exactly as
  proposed, killing bugs #1 and #3. Bug #2 is closed by `check_and_queue_virtual_view_reinvoke()`
  `layout/src/window.rs:9171`, invoked from the `ScrollTo` handler
  `dll/.../common/event.rs:2275`. `check_reinvoke` now lives at
  `layout/src/managers/virtual_view.rs:243` with ~10 dedicated unit tests.
- **Superseded by:** `scripts/IFRAME_INVESTIGATION_REPORT.md` (later, deeper, same subject).
- **Still open:** none of its own. (Whether `infinity.c` renders today was not re-run.)
- **Research value:** none — the one durable idea (per-frame `layout_results.clear()` fighting a
  manager that caches `was_invoked` across frames) is stated far better in the investigation report.

---

#### scripts/IFRAME_INVESTIGATION_REPORT.md

- **Verdict:** ARCHIVE — accurate historical bug narrative; fixes verified, git holds the rest.
- **Was:** 2026-02-26 root-cause of a resize flicker (grey rows ↔ yellow rectangle) in `infinity.c`.
  The insight: `layout_and_generate_display_list()` calls `layout_results.clear()`, but the
  IFrameManager's `was_invoked` flag is owned by `LayoutWindow` and survives the clear — so on any
  resize where bounds did **not** expand, `check_reinvoke()` returned `None`, the child DOM was never
  rebuilt, and the compositor logged "child DOM not found". Bug #2: `CallbackChange::ScrollTo` only
  called `scroll_manager.scroll_to()` and never checked re-invocation, so virtual scrolling stopped
  after the first chunk. §8 explicitly retracts a false "✅ WIRED UP" claim in an older
  `scroll6_report.md` — a documented instance of trusting a doc's status line.
- **Landed:** Both fixes, verbatim and still present. Bug #1: `layout/src/window.rs:1189` clears
  `layout_results`, `:1193` calls `self.virtual_view_manager.reset_all_invocation_flags()` under a
  CRITICAL comment restating the exact race (method at
  `layout/src/managers/virtual_view.rs:204`). Bug #2: `check_and_queue_virtual_view_reinvoke()`
  `layout/src/window.rs:9171` (doc-comment traces `ScrollTo → scroll_to() → check_and_queue…` at
  `:9167`), wired at `dll/.../common/event.rs:2275`, with the new
  `ProcessEventResult::ShouldUpdateDisplayListCurrentWindow` honoured on every backend — macOS
  `mod.rs:6261`, wayland `mod.rs:4419`/`2496`, x11 `mod.rs:4064`.
- **Superseded by:** n/a — it supersedes `IFRAME_ANALYSIS.md`.
- **Still open:** Only §9 item 4 (LOW): skip child DOMs in `build_webrender_transaction` to avoid
  double-submitting a display list already submitted via the parent's recursive translation. Partly
  addressed — `dll/src/desktop/wr_translate2.rs:740-759` tracks `foreign_child_dom_ids` — but not
  confirmed as the full de-duplication the doc asked for.
- **Research value:** One transferable rule, worth a line elsewhere rather than the whole doc:
  **a cache keyed on "already invoked" must be invalidated by whoever destroys the thing it caches** —
  here a manager outliving `layout_results.clear()`. Same family as the memory-index theme "the check
  was the defect".

---

#### scripts/IFRAME_SCROLL_DISPLAY_LIST_ARCHITECTURE.md

- **Verdict:** RESEARCH — the virtual-scroll display-list design that won; keep the rationale.
- **Was:** Explains a fix (moving the `IFrame` item after `PopScrollFrame` so WebRender's scroll
  transform stops dragging the iframe viewport off-screen), then argues the fix exposes three deeper
  problems: post-hoc display-list mutation (scanning for a matching `PushScrollFrame`/`PopScrollFrame`
  pair by depth-counting, silently broken by any reordering); an **empty** scroll frame that renders
  nothing and exists only as a hit-test target while declaring absurd content sizes (120M px for an
  infinite list); and **dual scroll authority** — WebRender APZ vs azul's `ScrollManager`. Proposes
  emitting `IFramePlaceholder` + a `HitTestArea` instead of a scroll frame, replacing the placeholder
  by `node_id` after the callback runs, making `ScrollManager` the single authority.
- **Landed:** The core proposal, under the `VirtualView` name.
  `DisplayListItem::VirtualViewPlaceholder` exists — `layout/src/solver3/display_list.rs:781-786`,
  whose doc comment reproduces the argument ("avoids the need for post-hoc scroll frame [mutation]…
  Unlike regular scrollable nodes, VirtualView nodes do NOT get a [scroll frame]"). Replacement by
  node id, not by scroll-frame scan: `layout/src/window.rs:2714` ("Replace the VirtualViewPlaceholder
  with the real VirtualView item"), match at `:2723`, and a second re-point site at `:7859`/`:7871`.
  Compositor treats a surviving placeholder as a no-op: `dll/src/desktop/compositor2.rs:1508-1509`.
  Display-list item doc at `display_list.rs:771` states the single-authority rule outright:
  "Scroll offset is communicated to the VirtualView callback, not via WebRender."
- **Superseded by:** n/a — this design beat the alternatives in `IFRAME_ANALYSIS.md`.
- **Still open:** Migration step 6 — `TAG_TYPE_IFRAME_SCROLL` (0x0600) was never added. The tag space
  in `core/src/hit_test.rs:389-414` stops at `TAG_TYPE_SCROLL_CONTAINER = 0x0500`, so VirtualView
  scroll hit-testing rides the generic scroll-container tag rather than a dedicated one. Worth
  confirming that is intentional rather than an unfinished step.
- **Research value:** High. Transferable concept: **replace post-hoc structural mutation of a
  generated IR with a sentinel/placeholder node that a later pass swaps by identity**, plus the
  companion rule that a virtualized viewport must have exactly one scroll authority and must not be
  parented to the compositor's own scroll spatial node. This is the iframe/virtual-scroll architecture
  keeper; the `IFRAME_*` bug reports are its footnotes.

---

### Tally

| Verdict | Files |
|---|---|
| RESEARCH (4) | EVENT_ARCHITECTURE_ANALYSIS.md, EVENT_ARCHITECTURE_ANALYSIS_DOC.md, DRAG_DROP_REPORT.md, IFRAME_SCROLL_DISPLAY_LIST_ARCHITECTURE.md |
| DELETE (4) | CALLBACK_INVOCATION_UNIFICATION.md, DOM_CHANGE_REPORT_ARCHITECTURE.md, click_event_analysis.md, IFRAME_ANALYSIS.md |
| ACTIVE (1) | ASYNC_TASK_API_DESIGN.md |
| ARCHIVE (1) | IFRAME_INVESTIGATION_REPORT.md |

### Cross-cutting: the frame-lifecycle contract

`EVENT_ARCHITECTURE_ANALYSIS_DOC.md` is the only doc in this cluster that **anticipated** the
"7 hand-rolled event loops share an implicit contract with no enforcement" theme, and it named the
cure: a `FrameChanges` newtype whose sole method consumes both change lists, making "handled one,
forgot the other" unrepresentable. Two thirds of that design shipped (both exhaustive enums, both
single sinks, `process_timers_and_threads()` as a trait default) — but the newtype, the actual
enforcement, did not. What stands in for it today is a **grep test**:
`dll/tests/backend_feature_parity.rs`, which scans all 7 backend sources for required call sites and
whose own header documents two features that were silently missing on entire platforms for
an unknown length of time, and ends "When the trait exists, delete this file."


## Part 08 — rendering / GPU / damage / clipping / images / animation

Audit date 2026-08-01, branch `master`. Every claim below was re-grepped against the
current tree; doc status lines were ignored.

---

#### scripts/DAMAGE_REGION_PLAN.md

- **Verdict:** RESEARCH — the render/present two-channel damage model, still the governing design.
- **Was:** 37 KB plan (2026-06-06 → 2026-07-03) to replace display-list-diff damage with a
  layout-level, two-channel (`RenderDamage` / `PresentDamage`) invalidation model shared by CPU
  and GPU on all four backends. Contains the "small paint, large present" scroll primitive
  (memmove + repaint the exposed strip), a CSS-property→damage classifier, the physical-pixel
  round-outward rule, and a 7-step P0–P7 implementation ladder. Also embeds a long empirical
  log of the headless brutal-test campaign (#11 stale text, #12 false-positive damage, #14
  thin-strip scroll, #16 diagonal pan, #17 natural scroll, #18a/b, #20, #21).
- **Landed:** §3 channel split + the MAX_RECTS/round-outward presenter rule — `layout/src/window.rs:286`
  (`enum FrameDamage`) and `layout/src/window.rs:360` (`to_present_rects_physical`, whose doc
  comment literally cites "per `DAMAGE_REGION_PLAN` §3"). Scroll primitive:
  `layout/src/cpurender/compositor.rs:861` `scroll_shift_region`, `:1052` `shift_diagonal_2d`,
  `:1110` `scroll_fast_path_eligible`, consumed at `dll/src/desktop/shell2/headless/mod.rs:626-633`
  and `layout/src/e2e/cpu_backend.rs:341-348`. #11/#12 fixes: `layout/src/solver3/fc.rs:2793/2957/2999`
  (`inline_content_hash`), `layout/src/solver3/display_list.rs:1037` (HitTestArea) and `:1043`
  (TextLayout `Arc::ptr_eq`). #17: `layout/src/managers/scroll_state.rs:330/432/442` (`natural_scroll`,
  `AZ_NATURAL_SCROLL`). P5 GPU buffer-age: `dll/src/desktop/shell2/linux/wayland/gl.rs:32`
  (`PartialPresentDamage`), `wayland/mod.rs:4523-4618` (`buffer_age()` → `renderer.render(size, age)`
  → `swap_buffers_with_damage`), WR side `webrender/core/src/renderer/mod.rs:732/4326/4397`.
  P7 one-detector: X11/Wayland/macOS/Windows/iOS/Android all now own the shared headless
  `CpuBackend` (`linux/wayland/mod.rs:324`, `macos/mod.rs:6331`, `windows/mod.rs:957`,
  `linux/x11/mod.rs:3816`, `ios/mod.rs:1211`, `android/mod.rs:77`) and read `last_present_damage`.
- **Superseded by:** n/a — it superseded DAMAGE_RENDERING.md (which said "keep
  `partial_present: None`, do NOT set `draw_previous_partial_present_regions: true`"; the plan
  did exactly the opposite and shipped it).
- **Still open:** §4 layout-level damage source never happened — the DL diff is still the damage
  producer (`layout/src/cpurender/compositor.rs:1616` `compute_display_list_damage`, live callers
  at `headless/mod.rs:416` and `e2e/cpu_backend.rs:194`). §5 `css_property_damage` / `DamageCollector`
  / `DamageClass`: **zero hits anywhere** — never written. `is_visually_equal` still falls to
  `_ => false` for `Image` and all gradient variants (`display_list.rs:1088`), so any frame with
  an image re-damages it — the structural-add-is-`Full` coarseness the doc flagged as "#10". The
  dead `scroll_layer`/`compute_exposed_rects` pair (`compositor.rs:588/762`) is still dead code
  with the inverted sign convention documented here — a live trap. Windows (WGL) and macOS
  (NSOpenGL) still have no buffer-age query → always full swap. §6 imperative caret/cursor
  sources still go through the DL diff, not a collector.
- **Research value:** high. The render-vs-present channel split with the asymmetric correctness
  rule ("RenderDamage may shrink to ∅; PresentDamage must never silently be ∅") plus the
  round-outward logical→physical conversion and the bounded-rects-else-Full budget is a
  transferable compositor invariant. The "damage as a MOVE, not a repaint" scroll primitive and
  the sign-convention post-mortem (why `compute_exposed_rects` was never wired) are the durable
  parts. Keep for `scripts/research/`.

---

#### scripts/DAMAGE_RENDERING.md

- **Verdict:** DELETE — superseded design whose central premise was reversed and shipped against.
- **Was:** Earlier (2026-06-03) incremental-repaint architecture note. Behaviour matrix for
  caret blink / scroll / resize × CPU × GPU across 4 platforms; per-platform compositor sub-rect
  damage table (Wayland `wl_surface_damage_buffer`, X11 `XPutImage` dst rect, macOS
  `setNeedsDisplayInRect:`, Windows `StretchDIBits` dst rect); a "Design 1 vs Design 2" scroll
  fork; and the "linked dirty rects" `Move { src, dst, delta }` idea.
- **Landed:** the caret CPU fix (always emit `CursorRect`, transparent when blinked off) is at
  `layout/src/solver3/display_list.rs:2324-2329` + the `is_visually_equal` arm at `:953`. Per-platform
  sub-rect present landed via `FrameDamage::to_present_rects_physical` (`layout/src/window.rs:360`).
  Design 2 (pixel-shift scroll) won over Design 1 and is `scroll_shift_region`.
- **Superseded by:** `scripts/DAMAGE_REGION_PLAN.md`. Its "Corrected premises (don't re-derive)"
  section is now **wrong**: it insists `partial_present: None` is correct and
  `draw_previous_partial_present_regions: true` must not be set; the shipped code sets both
  (`dll/src/desktop/shell2/linux/wayland/mod.rs:4517-4618`, `webrender/core/src/renderer/init.rs:470`).
  Leaving it in-tree actively misleads.
- **Still open:** the GPU caret-as-opacity-property idea (Phase 3) is not implemented — caret
  blink still routes through `SetCursorVisibility` → display-list dirty
  (`layout/src/callbacks.rs:1708`, `layout/src/window.rs:218` comment explains why `RefreshDom`
  can't be used: `is_layout_equivalent` would swallow it).
- **Research value:** only the `Move{src,dst,delta}` linked-dirty-rect primitive and the
  OS-scroll-hint mapping (X11 `XCopyArea`, DXGI `Present1 pScrollRect`, macOS `scrollRect:by:`),
  both of which are restated in DAMAGE_REGION_PLAN §0.6/§6. No unique content.

---

#### scripts/OPENGL_DOM_DIFF_OPTIMIZATION.md

- **Verdict:** DELETE — implemented essentially verbatim.
- **Was:** Plan to short-circuit the full layout pipeline when a `RefreshDom` produces a
  structurally identical DOM (the OpenGL demo's 16 ms animation timer). Four steps: fix
  `calculate_structural_hash` so `Image(Callback)` nodes match across frames (they hash the
  heap pointer today), add `is_layout_equivalent(old, new)`, short-circuit `regenerate_layout()`
  to return `LayoutUnchanged`, and give every shell a lightweight image-callback-only render path.
- **Landed:** all four. `core/src/dom.rs:2903-2913` hashes `cb.callback.cb` + `cb.refany.get_type_id()`
  for `DecodedImage::Callback` instead of the pointer. `core/src/styled_dom.rs:2623`
  `is_layout_equivalent` (+ unit tests at `:4157-4172`). `dll/src/desktop/shell2/common/layout.rs:124`
  `LayoutRegenerateResult::LayoutUnchanged`, returned at `:648`. Lightweight path:
  `dll/src/desktop/wr_translate2.rs:2815` `build_image_only_transaction` (does
  `skip_scene_builder()` at `:2830`), called from `macos/mod.rs:6395`, `windows/mod.rs:1209`,
  `linux/x11/mod.rs:4089`, `linux/wayland/mod.rs:4431`.
- **Superseded by:** n/a.
- **Still open:** the speculative "`Update` as bitflags" section (`REFRESH_IMAGES`,
  `REFRESH_SCROLL`, `REFRESH_GPU_VALUES`) never happened — `core/src/callbacks.rs:77` still has
  exactly `DoNothing | RefreshDom | RefreshDomAllWindows`. Case 3 ("only text changed →
  incremental text relayout") was explicitly out of scope and is still not done.
- **Research value:** none beyond the codebase. The one generalizable note — pointer-derived
  identity defeats cross-frame reconciliation — recurs in HANDOFF_IMAGE_GC and is better told there.

---

#### scripts/OPENGL_TEXTURE_SWAP_OPTIMIZATION.md

- **Verdict:** DELETE — its problem was solved by the sibling doc's approach; its own API was rejected.
- **Was:** Companion plan for the same ~50 % CPU OpenGL-demo problem, but via a **new public API
  surface**: add `Update::RefreshImageCallbacks`, teach all four shells to redraw without setting
  `frame_needs_regeneration`, thread `CallCallbacksResult::image_callbacks_changed` to a selective
  `process_image_callback_updates`, add a C helper
  `AzTimerCallbackReturn_continueAndRefreshImageCallbacks`, and update the C/Rust/C++/Python examples.
- **Landed:** essentially nothing of the proposed API. `Update::RefreshImageCallbacks` does not exist
  (`core/src/callbacks.rs:77`). `image_callbacks_changed` does not exist anywhere (0 hits).
  `process_image_callback_updates` (`dll/src/desktop/wr_translate2.rs:2903`) still takes no changed-node
  set and re-invokes all callbacks. Only `CallbackInfo::update_image_callback`
  (`layout/src/callbacks.rs:1186`) survives, and that pre-existed the doc.
- **Superseded by:** `scripts/OPENGL_DOM_DIFF_OPTIMIZATION.md` — the DOM-equivalence route reaches
  the same "skip layout, only re-run image callbacks + composite" outcome with **no ABI break**
  and no example changes, which is why the `Update` variant (an explicit `#[repr(C)]` ABI-break
  risk the doc itself flags) was never added.
- **Still open:** selective per-node image-callback invocation is still not implemented — every
  frame re-invokes every callback image. Cheap (O(callbacks)) but wrong for a many-viewport app.
- **Research value:** none. Its "Alternative Approaches Considered" A–D section is a decent record
  of why bypassing WebRender for a texture swap is wrong (loses radius clipping, shadows, z-order),
  but that's one paragraph.

---

#### scripts/CLIPPING_ANALYSIS_REPORT.md

- **Verdict:** DELETE — a wrong-diagnosis debug snapshot; its recommended fix was rejected.
- **Was:** (Despite the filename, this is the English "WebRender Clipping Analysis Report".)
  Debug-log analysis of scroll-container content not being clipped. Concludes the bug is that
  `CommonItemProperties.clip_rect` equals the primitive bounds rather than the clip region, and
  recommends "Option 1 (Recommended)": intersect `clip_rect` with the active clip bounds in
  `compositor2.rs`.
- **Landed:** the recommended fix was **not** applied and was correct not to be —
  `dll/src/desktop/compositor2.rs:302-307` still sets `clip_rect: rect` (the primitive's own
  bounds) with `clip_chain_id: current_clip_chain`, exactly as the sibling German doc's §8.4 says
  it should. The `[WR SCENE]` / `[WR CLIP]` / `[CLIP DEBUG]` probe strings the report is built on
  no longer exist in `webrender/core/src/clip.rs` or `scene_building.rs`.
- **Superseded by:** `scripts/WEBRENDER_CLIPPING_ANALYSIS.md` §8.4, which explicitly refutes this
  document's central claim ("`clip_rect` in `CommonItemProperties` is NOT the viewport clip").
- **Still open:** none.
- **Research value:** none — the mental model it proposes is the incorrect one.

---

#### scripts/WEBRENDER_CLIPPING_ANALYSIS.md

- **Verdict:** RESEARCH — the correct WebRender spatial-vs-clip separation model; still load-bearing.
- **Was:** German technical analysis of how `SpatialTree`/`ScrollFrame` and `ClipTree`/`ClipChain`
  interact. Core thesis: a WebRender `ScrollFrame` is **only a transform** and performs no clipping;
  `SpatialId` says *where*, `ClipChainId` says *what is visible*, and both must be set. Corollary:
  the scroll clip must be defined in the **parent** space (a stationary viewport), never in the
  scroll space, or the clip scrolls with the content. Plus a §8 catalogue of the six common
  mistakes and a debugging checklist.
- **Landed:** the model is exactly what the code does. `dll/src/desktop/compositor2.rs:941`
  `define_scroll_frame(parent_space, external_scroll_id, content_rect, adjusted_frame_rect, …)`,
  `:980` `define_clip_rect(parent_space, adjusted_frame_rect)` (parent space — the doc's golden
  rule), `:998` `define_clip_chain(parent_clip, [scroll_clip_id])` (parent-chained, per §8.5),
  and `content_rect` origin == `adjusted_frame_rect.min` at `:914-915` (per §8.3). Item-level
  `clip_rect` = primitive bounds per §8.4 (`:302`).
- **Superseded by:** n/a — it supersedes CLIPPING_ANALYSIS_REPORT.md.
- **Still open:** none as a bug report. It is documentation, and the invariants it names are not
  asserted anywhere in code or tests — a future refactor could re-break the parent-space rule silently.
- **Research value:** high, and the single best short explanation in the repo of the
  spatial-tree-vs-clip-tree split (the same separation Chromium's property trees and Flutter's
  layer tree make). Directly transferable to anyone touching `compositor2.rs`. Worth moving to
  `scripts/research/` (translating it to English would help).

---

#### scripts/webrender-diff-report.md

- **Verdict:** RESEARCH — the only inventory of what the vendored WebRender fork actually changed.
- **Was:** Full diff report of `webrender/` vs upstream Mozilla WR at `e1c924eb` (~111 files,
  ~21k/17k lines). Five categories: (1) peek_poke byte-stream serialization replaced by 9 typed
  `Vec`s in `DisplayListPayload`, with count fields added to the `Set*` marker items; (2) `dyn gl::Gl`
  → azul's `GenericGlContext`; (3) `MallocSizeOf` stripped; (4) ~80 % pure formatting; (5)
  `ItemRange<T>` → `&[T]` in `scene_building.rs`. Then a root-cause section: gradients render with
  zero stops because `define_border_radius_clip` pushes clip items *between* `SetGradientStops`
  and the `Gradient` item, and the iterator resets `cur_stops` on any non-marker item.
- **Landed:** the P0 gradient fix — `push_stops` now sits immediately before `push_gradient` in both
  branches (`dll/src/desktop/compositor2.rs:1621/1630`, radial `:1808/1817`, conic `:1926/1935`,
  with the comment "Push stops immediately before gradient to avoid clip items interleaving").
  The typed-Vec payload and count fields are real: `webrender/api/src/display_list.rs:871`
  (`SetGradientStops { stop_count }` iterator arm), `:1817` (push). P2 deserialize-path counts
  were fixed too (`display_list.rs:335-348` now computes `stop_count`/`primitive_count`/`point_count`).
- **Superseded by:** n/a. Overlaps `scripts/fix-gradients-filters-plan.md` Step 1, which is the
  action plan for the same finding.
- **Still open:** P3 items, both still present: `webrender/api/src/display_item_cache.rs:36` still
  does `std::mem::transmute_copy` on `GlyphInstance` (padding-byte hazard, only affects the
  Gecko-style retained item cache azul doesn't use), and `push_item_to_section`
  (`display_list.rs:1217`) still ignores its `_section` argument so item-group caching stays
  disabled. Neither is on azul's hot path.
- **Research value:** yes — a fork-delta inventory is exactly what you need before rebasing onto a
  newer WebRender, and the "marker item + greedy iterator" bug class (aux data must be adjacent to
  its consumer, or carry an explicit count) is a transferable serialization lesson. Keep.

---

#### scripts/IMAGE_PIPELINE_ANALYSIS.md

- **Verdict:** DELETE — the four proposed phases all landed; the TODO table is stale.
- **Was:** Architecture review of image loading/caching, concluding that CSS `background-image`
  is entirely unimplemented (only the URL string is stored), `ImageSource` is an empty stub struct,
  and `ImageCache` is never reachable from display-list generation. Proposes four phases: thread
  `&ImageCache` into `LayoutContext`/`DisplayListContext`, resolve `StyleBackgroundContent::Image`
  via `get_css_image_id`, replace the `ImageSource` stub with a real enum, and add intrinsic-size
  resolution. Includes a 20-row TODO table spanning filters, box-shadow, gradients, menu icons.
- **Landed:** Phases 1+2 — `image_cache: &ImageCache` is a real parameter/field
  (`layout/src/solver3/display_list.rs:1472/1527/4869`) and the background arm resolves
  (`:1492` and `:1552`, `image_cache.get_css_image_id(image_id)`). Phase 3 — `ImageSource` is a
  real enum in both shapes (`layout/src/font_traits.rs:199`: `Ref/Url/Data/Svg/Placeholder`;
  `layout/src/text3/cache.rs:2697`), plus a live `ImageSource::Node` variant used by the
  chokepoint for runtime image swaps (`display_list.rs:4579-4589`). Phase 4 —
  `get_image_ref_for_image_source(source, image_cache, size)` at `display_list.rs:4869`.
  Most of the medium-priority TODOs are also done: WR box-shadow (`compositor2.rs:2013`
  `push_box_shadow`), backdrop-filter (`:2065`), CPU image blit
  (`layout/src/cpurender/raster.rs:2893` `render_image`).
- **Superseded by:** partly `scripts/fix-gradients-filters-plan.md` (which owns the filter /
  shadow / opacity rows of the TODO table and shipped them).
- **Still open:** nothing from the checklists; the "Long-term Enhancements" list (lazy loading
  with placeholder, async decode, SVG through `ImageSource`) is aspirational and partly done
  (`ImageSource::Svg`/`Placeholder` exist). Separately, the known open bug that replaced/image
  nodes do not flex-grow/stretch is **not** mentioned by this doc and remains open.
- **Research value:** none — codebase-specific plumbing.

---

#### scripts/IMAGE_RENDERING_DEBUG_REPORT.md

- **Verdict:** DELETE — single-bug debug trace, fixed with the doc's own Option A.
- **Was:** Root-cause trace for "no images render at all": `collect_and_measure_inline_content`
  in `fc.rs` discarded the `ImageRef` and pushed `ImageSource::Url(String::new())`, so
  `get_image_ref_for_image_source` returned `None` and no `DisplayListItem::Image` was ever
  emitted; the working replaced-element path in `display_list.rs` was unreachable because it sat
  behind an `else if` on `inline_layout_result`. Offers Option A (store `ImageRef` in
  `ImageSource`) and Option B (bypass inline layout for images).
- **Landed:** Option A, and then some. `ImageSource::Ref(ImageRef)` exists
  (`layout/src/font_traits.rs:200`) and the inline paint path resolves it plus a newer
  `ImageSource::Node` live-resolution variant (`layout/src/solver3/display_list.rs:4579-4599`).
  The `get_image_ref_for_image_source` helper it asks for is real
  (`display_list.rs:4869`) and does the `ImageCache` lookup the doc left as a `TODO`.
- **Superseded by:** `scripts/IMAGE_PIPELINE_ANALYSIS.md` (same problem space, broader) and by
  the resource chokepoint/resolver work, which replaced snapshot-at-IFC-time with
  resolve-at-paint-time (`ImageSource::Node` + `resolved_content().image_for_paint`).
- **Still open:** none.
- **Research value:** none — the `[IMAGE DEBUG]` probe strings it documents no longer exist.

---

#### scripts/HANDOFF_IMAGE_GC_2026_07_04.md

- **Verdict:** ACTIVE — the image half shipped, the font GC and the windowed verification are open.
- **Was:** Handoff for the 1 GB video-frame leak. Three cooperating add-only registries
  (`ResourceUpdate::DeleteImage` translated but never constructed; `currently_registered_images`
  insert-only; `scan_used_images` dead code). Its sharpest finding is §2d: because
  `ImageRefHash = data as usize` (the heap pointer), the leak was *masking* an aliasing bug —
  the moment you free anything, the allocator can hand the address to a new image, which then
  collides with the freed key and silently renders the **old texture**. Hence "Option C
  (monotonic ids instead of pointers) is not optional, it is a prerequisite" before Option A
  (mark-and-sweep) or Option B (epoch-deferred delete).
- **Landed:** Option C — `core/src/resources.rs:1175` `image_ref_get_hash` now returns
  `ImageRefHash { inner: ir.id }` with a comment spelling out the retired-id safety argument.
  Option B — `dll/src/desktop/wr_translate2.rs:1826` constructs
  `ResourceUpdate::DeleteImage(resolved.key)` (the "zero hits" the doc reported is now one real
  emitter). Test: `dll/tests/image_lifecycle.rs:221`
  `stale_image_is_deleted_after_retention_window`, exercising `scan_used_images` at `:149/194`.
- **Superseded by:** n/a.
- **Still open:** (1) **Font GC** — `ResourceUpdate::DeleteFont`/`DeleteFontInstance` are still
  only translated (`wr_translate2.rs:1409/1415`) and constructed only in a helper with no callers;
  `core/src/resources.rs:1448-1466` documents this in-place ("No `DeleteFont`/`DeleteFontInstance`
  … `last_frame_registered_fonts`" is the intended hook, `core/src/resources.rs:1337`). Fonts
  still leak, slowly. (2) **`scan_used_images` still ignores `_css_image_cache`**
  (`layout/src/window.rs:3785` — the parameter is still underscore-prefixed), so a
  CSS-`url()`-only image is invisible to the live-set scan; safe today only because the epoch
  retention window is the actual gate. (3) The §5 windowed, GPU-instrumented, four-backend
  verification pass (RSS + GPU memory plateau, no flicker, allocator-reuse stress) has never
  been run — the headless test proves the logic, not the numbers.
- **Research value:** high — §2d is a genuinely reusable hazard write-up: *a leak can be load-bearing*.
  Pointer-derived identity keys are safe only while nothing is ever freed, so adding a GC to a
  leaking cache converts a memory bug into intermittent silent data corruption unless you change
  the key scheme first. Pairs with the same lesson in OPENGL_DOM_DIFF (pointer hashes defeat
  reconciliation). Worth `scripts/research/` even though the doc is otherwise a handoff.

---

#### scripts/fix-gradients-filters-plan.md

- **Verdict:** DELETE — all six steps implemented.
- **Was:** Six-step plan covering the WebRender greedy-stop-consumption bug (add count fields to
  `SetGradientStops`/`SetFilterOps`/`SetFilterPrimitives`/`SetPoints`), gradient offset/DPI/
  border-radius fixes in `compositor2.rs`, seven missing CSS filter function variants, emission
  of `PushFilter`/`PushBackdropFilter`/`PushOpacity`/`BoxShadow` display-list items (never
  generated at all), wiring the compositor stubs to real WR calls, and a `get_backdrop_filter`
  bug querying `CssPropertyType::Filter`.
- **Landed:** Step 1 — `webrender/api/src/display_list.rs:871` iterator honours `stop_count`,
  `:1817` push writes it. Step 2 — gradient stop ordering `compositor2.rs:1621/1630`, border-radius
  clip via `define_border_radius_clip` at `:1607`. Step 3 — all seven variants present:
  `css/src/props/style/filter.rs:58-64` (`Brightness/Contrast/Grayscale/HueRotate/Invert/
  Saturate/Sepia`). Step 4 — emission at `layout/src/solver3/display_list.rs:2532` (`PushOpacity`),
  `:2546` (`PushFilter`), `:2562` (`PushBackdropFilter`), `:3515` (`BoxShadow`). Step 5 —
  `compositor2.rs:2013` `push_box_shadow`, `:2065` `push_backdrop_filter`, `:2114` `push_shadow`.
  Step 6 — `core/src/prop_cache.rs:2752` `impl_get_prop!(get_backdrop_filter, …, BackdropFilter, …)`
  now queries the right type.
- **Superseded by:** n/a; shares its Step 1 with `scripts/webrender-diff-report.md`.
- **Still open:** none of the six. (Whether the *visual* output matches Chrome for each filter is
  a reftest question this plan never covered.)
- **Research value:** none unique — the transferable "aux-data markers must be adjacent to their
  consumer" lesson is better stated in webrender-diff-report.md.

---

#### scripts/ANIMATION_SHADER_DESIGN.md

- **Verdict:** RESEARCH — unimplemented but high-quality comparative design; keep as design rationale.
- **Was:** 23 KB, 2026-07-08, explicitly read-only research. **Part 1**: a DOM-morph animation
  system, benchmarked against View Transitions / FLIP / Framer Motion `layoutId` / springs /
  Motion One / Rive / Lottie, landing on "declarative shared-layout FLIP on live nodes" because
  azul's diff already emits the correspondence map (`DiffResult.node_moves`) that View
  Transitions fakes with `view-transition-name`. Details the zombie/exit-retention model
  (`RetainedZombie`, non-interactive composited overlays, "logic sees Dom B, screen shows
  B ∪ retained-A-zombies"), the composited-vs-layout perf fork, and velocity-preserving
  retarget-on-interrupt as the reason springs beat bezier. **Part 2**: custom shader CSS layers
  (glassmorphism) — reuse blinc's `@flow`→WGSL codegen and glass shaders via `naga` transpile,
  never its wgpu runtime; Tier 1 = `ImageRef::callback` + `GlShader` with a previous-frame
  backdrop; Tier 2 = a `FilterGraphOp::CustomShader` node in WR's SVGFE graph.
- **Landed:** nothing. No `core/src/animation.rs` (still absent, and `ARCHITECTURE.md:184`'s
  reference to it is still stale). Zero hits for `AnimationManager`, `RetainedZombie`,
  `on_before_unmount`, `Update::Retain`, `animate_layout`. No `Spring` variant in
  `css/src/props/basic/animation.rs`. No `StyleBackgroundContent::Shader`. No
  `FilterGraphOp::CustomShader` in `webrender/`.
- **Superseded by:** n/a.
- **Still open:** everything — this is a proposal, not a plan of record. Its substrate claims do
  still hold (`core/src/diff.rs` node_moves, `GpuValueCache` transform/opacity keys already driving
  scrollbar thumbs, `override_css_property` as the top cascade layer), so it remains actionable.
  Both parts carry the same ABI caveat: `CssProperty`/`DisplayListItem` are `#[repr(C)]` +
  api.json-frozen, so new variants must be batched into a planned ABI break.
- **Research value:** the highest of the batch. Two independently valuable comparative surveys —
  (a) DOM-morph animation vs View Transitions/FLIP/Framer Motion/Rive with a concrete argument for
  live-node FLIP over snapshot cross-fade, plus the interruption/retarget-with-velocity criterion;
  (b) a shader-extension-point map of WebRender's backdrop capture→resolve→composite pipeline and
  the two native hook points. Move to `scripts/research/`.

---

#### scripts/SMOOTH_ZOOM_DESIGN.md

- **Verdict:** ACTIVE — signed-off-pending design, never built; the described "today" is still today.
- **Was:** 4 KB design (2026-06-10) for smooth map zoom. Two-phase model like every production
  slippy map: during the wheel gesture apply a cursor-anchored `transform: scale(2^(animated−grid))`
  to the grid container (O(1)/frame, no tile re-raster); on settle (~120 ms idle or integer-z
  crossing) commit the zoom, rebuild the tile grid, reset the transform. Proposes
  `zoom_target`/`zoom_anchor_px`/`zoom_settle_deadline` on `MapTileCache` and calls out the damage
  requirement explicitly (a zoom tick damages the whole VirtualView grid region, and must not be
  eaten by the "nothing changed → skip" fast path). Ends with four open decisions awaiting sign-off.
- **Landed:** nothing. `layout/src/widgets/map.rs:879` `map_on_scroll` still does the instant
  `dz = dy.signum() * 0.5; cache.viewport.zoom = (zoom + dz).clamp(min, max)` the doc describes
  as "today", then `trigger_all_virtual_view_rerender()`. Zero hits for `zoom_target`,
  `zoom_anchor`, `zoom_settle`.
- **Superseded by:** n/a.
- **Still open:** all of it, including the four unanswered decisions (transform vs re-raster,
  cursor vs centre anchor, ease/settle timings, CPU compositor transform support). Its open
  question "does cpurender honour a runtime transform on a VirtualView grid node?" is unresolved
  and is a real prerequisite. Cross-reference: the doc's damage requirement is served by the
  GPU-value damage channel that shipped since (per DAMAGE_REGION_PLAN §0.5 round 2), so that
  half of the risk is now retired.
- **Research value:** modest but real — the "transform the already-rasterised layer during the
  gesture, re-raster on settle" two-phase pattern is the standard slippy-map/zoom-UI technique
  and is stated crisply here. Fold into `slippy-map-design` notes rather than keeping standalone.

---

#### scripts/REFTEST_BUG_ANALYSIS.md

- **Verdict:** DELETE — all six root causes fixed.
- **Was:** Triage of six failing reftests into five root causes: (1) a `num_children > 3`
  "assume overflow" heuristic in the scrollbar-info fallback painting spurious scrollbars,
  (2) no `calc()` parser, (3) `get_z_index()` a stub returning 0, (4) `grid-column`/`grid-row`
  placement never forwarded to Taffy, (5) `grid-template-areas` entirely unimplemented,
  (6) inline-block shrink-to-fit ~2× too wide. Includes per-bug "can we fix without Gemini"
  routing.
- **Landed:** (1) the heuristic is gone — `layout/src/solver3/getters.rs:2501`
  `get_scrollbar_info_from_layout` is now one line, `node.scrollbar_info.unwrap_or_default()`,
  with a regression test at `:7026`. (2) `calc()` is implemented — `CalcAstItem` flat
  stack-machine in `css/src/props/layout/dimensions.rs:29/101`, evaluated by
  `layout/src/solver3/calc.rs`. (3) `get_z_index` at `getters.rs:1759` reads
  `LayoutZIndex` from the real node state. (4) `layout/src/solver3/taffy_bridge.rs:966-981`
  sets `taffy_style.grid_column`/`grid_row` (fast compact path + `grid_placement_to_taffy`
  fallback). (5) `grid-template-areas` parses (`css/src/props/property.rs:172/685/940`,
  `css/tests/test_grid_area_parse.rs`) and is forwarded at `taffy_bridge.rs:887`.
  (6) shrink-to-fit no longer has a dedicated failing test; the IFC intrinsic-sizing path was
  rewritten wholesale since.
- **Superseded by:** n/a.
- **Still open:** none traceable to this document. The reftest *harness* is separately known to
  be flaky/non-hermetic (chrome-stable dependency), which is a different problem.
- **Research value:** none — pure triage log.

---

#### scripts/BUTTON_TEXT_MISSING_ANALYSIS.md

- **Verdict:** DELETE — bug fixed, and the specific fix has since been replaced by a spec-based rewrite.
- **Was:** German "problem solved" report for missing button text in the hello-world C example.
  Two causes: `inline_layout_result` stored on the IFC text child rather than on the inline-block
  parent (fixed by copying it up in `fc.rs`), and `paint_inline_object` painting only
  background/border and never the inline-block's own content (fixed by calling
  `paint_inline_content` from the `InlineContent::Shape` arm). Also notes a duplicate
  `(NT::Button, PT::Display)` arm in `ua_css.rs` (harmless — first match wins).
- **Landed:** the outcome, not the patch. `layout/tests/inline_block_text.rs` exists and covers
  the case. The duplicate UA-CSS arm is gone — `core/src/ua_css.rs:718` now has exactly one
  `(NT::Button, PT::Display) => Some(&DISPLAY_INLINE_BLOCK)`. Neither posted patch survives:
  the `fc.rs` "propagate inline_layout_result to the inline-block parent" hack has no trace
  (0 hits for `first_child_with_ifc`), and `paint_inline_object`
  (`layout/src/solver3/display_list.rs:4579-4607`) does **not** call `paint_inline_content` —
  the `Shape` arm only calls `paint_inline_shape`.
- **Superseded by:** the inline-block-as-pseudo-stacking-context rewrite —
  `layout/src/solver3/display_list.rs:4610` (`+spec:inline-block:a60a89 — inline-block painted
  atomically as pseudo-stacking-context per E.2`), with `paint_inline_shape` at `:4629-4634`
  explicitly bailing out when the inline-block establishes a stacking context so its background
  isn't double-painted by `generate_for_stacking_context`. Content now reaches the paint via the
  stacking-context descent, not via an ad-hoc call from the IFC object painter.
- **Still open:** none.
- **Research value:** none — the code it describes no longer exists.

---

### Tally

| Verdict | Count | Files |
|---|---|---|
| DELETE | 9 | DAMAGE_RENDERING, OPENGL_DOM_DIFF_OPTIMIZATION, OPENGL_TEXTURE_SWAP_OPTIMIZATION, CLIPPING_ANALYSIS_REPORT, IMAGE_PIPELINE_ANALYSIS, IMAGE_RENDERING_DEBUG_REPORT, fix-gradients-filters-plan, REFTEST_BUG_ANALYSIS, BUTTON_TEXT_MISSING_ANALYSIS |
| RESEARCH | 4 | DAMAGE_REGION_PLAN, WEBRENDER_CLIPPING_ANALYSIS, webrender-diff-report, ANIMATION_SHADER_DESIGN |
| ACTIVE | 2 | HANDOFF_IMAGE_GC_2026_07_04, SMOOTH_ZOOM_DESIGN |
| ARCHIVE | 0 | — |

9 + 4 + 2 = 15.


## Part 09 — platform windowing, menubar, system styling, X11

Shell module located at `dll/src/desktop/shell2/` (79 `.rs` files, ~67.8k lines) with
per-platform dirs `windows/ macos/ linux/{x11,wayland,dbus,gnome_menu} android/ ios/ headless/`
and the shared `common/` (event.rs, layout.rs, compositor.rs, debug_server/, e2e_test.rs).
Device backends live in `dll/src/desktop/extra/`.

---

#### scripts/PLATFORM_WINDOW_REFACTORING.md

- **Verdict:** DELETE — all 5 phases implemented verbatim; plan is now the code.
- **Was:** A 5-phase plan to collapse `PlatformWindow` (V1 lifecycle trait) + `PlatformWindowV2`
  (37-method event trait) into one trait, extract the 28 duplicated per-platform getters into a
  `CommonWindowState` struct + declarative macro (explicitly rejecting a `common()/common_mut()`
  accessor because a single `&mut CommonWindowState` kills split borrows), drop the "V2" suffixes,
  dedup the X11/Wayland `timerfd` code, and flatten the `LinuxWindow` enum. Also pinned
  `Compositor`/`CpuCompositor` as deliberately-unused, reserved for a future AGG CPU backend.
- **Landed:** single `pub trait PlatformWindow` at `dll/src/desktop/shell2/common/event.rs:1349`
  (no `PlatformWindowV2` anywhere in shipping code — only two stale doc-comments at
  `layout/src/window.rs:6066` and `core/src/events.rs:2641`); `pub struct CommonWindowState` at
  `common/event.rs:913`; `macro_rules! impl_platform_window_getters` at `common/event.rs:1219`
  with the borrow-checker rationale preserved at `common/event.rs:686`; invoked by all 7 shells
  (`windows/mod.rs:4878`, `macos/mod.rs:2751`, `linux/x11/mod.rs:4610`,
  `linux/wayland/mod.rs:953`, `ios/mod.rs:1524`, `android/mod.rs:326`, `headless/mod.rs:2037`).
  Phase 3 rename done: `common/event.rs` + `common/layout.rs` exist, no `*_v2.rs`. Phase 4 done:
  `linux/timer.rs:12 start_timerfd` / `:68 stop_timerfd`, called from `linux/x11/mod.rs:4666,4675`
  and `linux/wayland/mod.rs:1009,1018`. Phase 5 done: `linux/mod.rs:37 enum LinuxWindow` is now
  226 lines (was 334). `GnomeMenuManagerV2` renamed back to `GnomeMenuManager`
  (`linux/gnome_menu/manager.rs:23`); no `WindowProperties` anywhere. Compositor trait kept
  (`common/compositor.rs:362`, `common/cpu_compositor.rs:13`) as instructed.
- **Superseded by:** n/a (it *is* the shell2/common hoisting, for the window-state seam).
- **Still open:** one leftover the doc never covered and the 2026-07-31 seam audit named — the
  hoisted trait covers *state accessors + event processing*, but the **frame-lifecycle contract**
  (raise/consume/retire, wake sources, close consumption) is still hand-rolled per event loop:
  `poll_event`/`wait_for_events` remain platform-only (no `common/event.rs` definitions), so the
  seam-audit's shell2/common proposal is a *second, unlanded* hoist on top of this one.
  Two stale `PlatformWindowV2` doc-comments should be updated.
- **Research value:** none for the plan itself; the one transferable nugget (macro-generated
  per-field getters beat a single `common_mut()` accessor because split borrows survive) is
  already inline at `common/event.rs:686`/`:1213`.

---

#### scripts/PLATFORM_INTEGRATION_AUDIT.md

- **Verdict:** DELETE — status matrix contradicted by shipped backends; superseded by scripts/research/.
- **Was:** A ✅/🔶/📝/❌ matrix of every device/OS integration (UDP, SQLite, PDF, gamepad,
  geolocation, sensors, biometric, permission, camera, screen capture, video, mic, audio sink,
  video codec) across macOS/Linux/Windows/iOS/Android, plus a prioritized "extend to desktop"
  plan, the dlopen-not-link-bind cross-compile rule, and a 2026-05-21 windowing/input review
  (multi-monitor OK; desktop multi-touch, pen/Wacom, safe-area listed as gaps).
- **Landed:** nearly every 🔶/❌ cell is now a real backend. `dll/src/desktop/extra/` has
  per-platform files for `camera/{v4l2,avfoundation,windows,android}.rs`,
  `screencap/{linux,macos,dmabuf}.rs` (Linux = real PipeWire portal, `screencap/linux.rs`),
  `sensors/{linux,windows,apple,android}.rs`, `biometric/{linux,windows,apple,android}.rs`,
  `geolocation/{linux,windows,macos,ios,android}.rs`, `keyring/*`, `audio/{alsa,cpal_*,aaudio,
  avfoundation_*}.rs`. The named windowing gaps closed too: XInput2 touch+pen on X11
  (`linux/x11/dlopen.rs:~`, `linux/x11/defines.rs` "XI2 — touch + pen/tablet. ABI per
  scripts/WACOM_TOUCH_API_RESEARCH.md", decode_valuator in `linux/x11/mod.rs`), Windows
  touch/pen (`windows/mod.rs` WM_POINTER* constants 0x0245-0x0247), safe-area insets
  (`layout/src/callbacks.rs`, `macos/mod.rs`, `ios/mod.rs`). Central dlopen chokepoint at
  `dll/src/desktop/mod.rs` (`open_first_lib`) enforces the cross-compile rule.
- **Superseded by:** `scripts/research/01_camera_screen_capture.md`, `02_biometric_auth.md`,
  `03_sensors_gamepad_stylus.md`, `04_system_integration.md`, `08_permission_dom_nodes.md` —
  the durable per-area API research the matrix only summarized.
- **Still open:** `extra/video_codec/` is still selector-only (FFI stub per its own docs); video
  *playback* is still the SMPTE-bars test pattern; the permission **request** side is TODO on
  every platform (`extra/permission/{linux,windows,macos,ios,android}.rs` read status only);
  Android display-cutout via JNI.
- **Research value:** the "any desktop system lib must be dlopen'd, never link-bound, so the dll
  cross-compiles" rule — but it is already enforced in code and restated in scripts/research/.

---

#### scripts/PLATFORM_DEBUG_PLAN.md

- **Verdict:** ACTIVE — Phases 0–3 shipped; Phase 4 (per-OS runtime comparison) never ran.
- **Was:** The 0.2.0 post-release platform-layer debug plan: a bug inventory from real user runs
  (C1 macOS `render_api.unwrap()` abort, C2–C5 Windows/Linux worker + gamepad crashes, B1 Linux
  `libazul.so` ~80 undefined `Py*` symbols, B2 130 MB demos, B3 missing execute bit, R1–R7
  rendering, F1–F6 device wiring, M1 RAM), then Phase 0 API re-validation, Phase 1 always-on
  `plog_*` logging, Phase 2 typed "unavailable" return codes, Phase 3 an `azul-self-test` CLI,
  Phase 4 = run it on every OS and diff the logs.
- **Landed:** `examples/azul-self-test/` exists; `plog_info!` etc. at
  `dll/src/desktop/shell2/common/debug_server/mod.rs:81`; the capability probe at
  `dll/src/desktop/extra/capability.rs` (per-subsystem `{available, backend, reason}`);
  Phase-0 output committed as `scripts/problems/api-validation.md` alongside
  `problems-{linux,windows,macos}.txt`; F4's fix shipped as `WindowStateSource`
  (`common/event.rs`, `linux/x11/mod.rs`, `macos/mod.rs`, `windows/mod.rs`); C1 fixed — the 5
  remaining `render_api...unwrap()` sites in `macos/mod.rs:6384,6398,6416,6417,6466` all sit
  **after** the CPU-backend early-return (`macos/mod.rs:6370-6374` "GPU backend: WebRender
  transaction"), so they are mode-guarded, not latent.
- **Superseded by:** partially by the 2026-07-31 seam audit (which re-derived and fixed several
  of the same P1 symptoms) — but the runbook itself was never executed.
- **Still open:** Phase 4 in full — `scripts/problems/` contains **no** `selftest-<os>.log`, so
  R3/R4/R5/R6/R7 (Windows first-frame blank, fonts, content offset, image updates), F5/F6,
  C2/C3 (Windows worker aborts), C5 (Linux gilrs double-free) and M1 (RAM 90–120 MB) have never
  been confronted with real per-OS data. This matches the memory note "real-hardware verification
  for macOS/Windows/iOS/Android REMAIN".
- **Research value:** the *method* — always-on tagged `plog_` subsystem traces + a self-describing
  self-test binary whose logs are diffed across OSes — is the only way found to localize
  platform-layer bugs without a device farm. Worth keeping as a paragraph, not a 20 KB file.

---

#### scripts/PLATFORM_DND_MENU_RESEARCH.md

- **Verdict:** RESEARCH — compact 4-platform file-DnD protocol comparison; all of it shipped, the protocol notes stay valuable.
- **Was:** 2026-06-20 blind-implementation research for four items: (A) macOS app menu bar
  wire-up from the DOM's `get_menu_bar()`, (B) Windows file DnD via OLE `IDropTarget` replacing
  drop-only `WM_DROPFILES`, (C) X11 XDND v5, (D) Wayland `wl_data_device`. Each entry names the
  exact call sequence, the mandatory replies (XdndStatus, `wl_data_offer.accept` with the *enter*
  serial), the version gates, and the classic deadlocks (flush before closing the write fd).
- **Landed:** all four. Shared substrate `layout/src/managers/file_drop.rs` +
  `handle_file_drag_entered/exited/drop` now exist on every desktop shell —
  `macos/events.rs:889,930,962`, `windows/mod.rs:1663,1687,1706`,
  `linux/x11/events.rs:1018,1041,1056`, `linux/wayland/mod.rs:3537,3560,3575`.
  Windows: `windows/dnd.rs` (213 lines) — `OleInitialize` + `RegisterDragDrop` + `IDropTarget`,
  header comment explicitly says it replaces the legacy `WM_DROPFILES` path.
  X11: full XDND atom set interned at `linux/x11/mod.rs:814-820`, `XdndAware`=5 set at
  `linux/x11/mod.rs:1633`. Wayland: `wl_data_device_manager/device/offer` C types + listeners at
  `linux/wayland/defines.rs:113-134`. macOS menu bar: `set_application_menu`
  (`macos/mod.rs:5458`) is now called (`:5499`) and `setup_main_menu` (`:2231`) shares
  `menu::build_app_submenu`.
- **Superseded by:** n/a.
- **Still open:** none of the four items; the doc's own "verify per target" build matrix is stale.
- **Research value:** HIGH — this is the DnD-across-platforms concept the research bucket is for:
  one page that maps a single internal `FileDropManager` seam onto four wire protocols
  (NSDraggingDestination / OLE COM / XDND ClientMessages / wl_data_offer fd-pipe), including the
  per-protocol handshake obligations you cannot infer from an API listing.

---

#### scripts/MENUBAR_HANDOFF_PROMPT.md

- **Verdict:** DELETE — an agent bootstrap prompt for a dead branch; its facts live in code + memory.
- **Was:** A self-contained "paste into a fresh agent" prompt (2026-06-09) for finishing the
  software menu bar: standing rules for branch `mobile-ios-android` ("do NOT push"), the codegen
  pipeline, the `WindowPosition::RelativeToParentWindow` model, the CSS priority/scoping model
  (why chrome must be injected at the **Dom** level before `create_from_dom`), the menubar widget
  design, then BUG 1–4 and FEATURE A/B/C.
- **Landed:** the models are all real — `layout/src/widgets/menubar.rs:74 build_menubar_dom`,
  `dll/src/desktop/shell2/common/layout.rs:1157 inject_software_menubar` (called at `:274`,
  Linux-only, gated on `should_use_gnome_menus()` + root `get_menu_bar()`). BUG 1/3 fixed (see
  MENUBAR_INJECTION_PLAN below); BUG 2 re-measured as a non-bug (system colors correct).
- **Superseded by:** the codebase + memory `azul-css-cascade-model` / `azul-push-to-master`
  (its branch/no-push rules now actively *contradict* the standing rule that all azul work goes
  straight to master).
- **Still open:** two of its features never landed — **FEATURE A**: `Menubar` is still not an
  api.json widget (`api.json` has `"Titlebar"` at :47903 with ctor+`dom`; there is no `Menubar`
  entry, and `layout/src/widgets/menubar.rs` exposes only the free function `build_menubar_dom`).
  **FEATURE B**: the software titlebar is *still* injected at the StyledDom level —
  `common/layout.rs:1123 inject_software_titlebar` builds a separate `StyledDom` and
  `append_child`s it, and `csd::wrap_user_dom_with_decorations` (`dll/src/desktop/csd.rs:76`) is
  still called from `common/layout.rs:345`. So titlebar and menubar use *two different* injection
  models, which is exactly what the doc asked to unify.
- **Research value:** none (prompt scaffolding); the CSS-scoping explanation is duplicated in
  memory and in `core/src/styled_dom.rs` comments.

---

#### scripts/MENUBAR_INJECTION_PLAN.md

- **Verdict:** ARCHIVE — dated fix log (six ✅ root-cause writeups) with a mostly-ticked checklist.
- **Was:** Began as "inject a software menu bar on X11 to exercise window positioning", became a
  chronological log: BUG 1 dropdown never opened (root cause: `invoke_single_callback` hard-coded
  the hit node to `{root, null}` → `get_hit_node()` null for *every* dispatched callback —
  cross-platform, commit `66c343f36`); #10 double-popup (`matches_filter_phase` ignored the
  propagation phase → every callback whose hit target was a descendant fired in *both* capture and
  bubble); menu grab + dismissal (`XGrabPointer` after `XMapWindow` without `XSync` →
  `GrabNotViewable`, unchecked; dismissal leaked the X window — commit `069a2b3e2`); BUG 3
  "View"→"V" (text3 `measure_intrinsic_widths` omitted per-glyph kerning that the line breaker
  added — regression test `layout/tests/menubar_item_clip.rs`); context-menu offset re-measured
  as (0,0). Plus earlier "CURRENT STATE" sections and a Step 2/Step 3 recipe.
- **Landed:** `layout/src/widgets/menubar.rs` (41 KB, 10+ unit tests),
  `common/layout.rs:1157 inject_software_menubar`, the old csd.rs bar removed (`csd.rs:47` now
  just points at the widget), `XGrabPointer`/`XUngrabPointer` bound at
  `linux/x11/dlopen.rs:97,98`, Wayland `xdg_popup` + `popup_done` listener at
  `linux/wayland/defines.rs:276,472-484`, `layout/src/widgets/drop_down.rs` exists (Step 3).
- **Superseded by:** later X11 work logged in HANDOFF_LINUX_X11.md firings 43–46 (click-outside
  dismissal, positioning at cursor, reposition-after-size, height-clamp + scroll).
- **Still open:** its own tail backlog — expose `Menubar` in api.json (not done, see above);
  unify titlebar injection to the Dom-level pattern (not done); "BUG 2 light-on-dark menubar
  colors" is left as `STILL OPEN` at :90 while the file's own tail says system colors are correct
  — unresolved contradiction, needs one visual check.
- **Research value:** low as a document; the two *general* root causes it uncovered
  (null hit-node for all dispatched callbacks; capture-phase double-fire for descendant targets)
  are the durable content and are already fixed + regression-tested.

---

#### scripts/SYSTEMSTYLE.md

- **Verdict:** RESEARCH — the native system-styling/theming API survey; its recommendations shipped and it stays the reference.
- **Was:** A critique + API survey arguing the then-current `std::process::Command` discovery
  (`reg.exe`, `defaults read`, `gsettings`, `kreadconfig5`) reads *saved config*, not *resolved
  styling*, and must be replaced by native IPC/FFI: Windows `UISettings`/`GetSystemMetrics`/
  `SystemParametersInfoW(SPI_GETNONCLIENTMETRICS)`/`SPI_GETHIGHCONTRAST`; macOS
  `NSApp.effectiveAppearance` + semantic `NSColor` (labelColor, controlAccentColor) + `NSFont` +
  `NSScroller`; Linux XDG Desktop Portal `org.freedesktop.portal.Settings.Read(
  "org.freedesktop.appearance","color-scheme")`. It then enumerates the *missing* dimensions
  (Mica/Acrylic/vibrancy materials, double-click time+distance, drag threshold, caret blink,
  scrollbar visibility policy, text scaling, focus visuals, icon/cursor themes, sound themes),
  proposes a "superset" struct, and a `libloading` "soft load" architecture so nothing link-binds.
- **Landed:** the whole discovery recommendation. **Zero** `process::Command` remains in any
  system-style path (`css/src/system.rs`, `linux/system_style.rs`, `windows/system_style.rs`,
  `macos/system_style.rs`). Windows dlopen'd fn-pointer table at
  `windows/system_style.rs:52-65` (`GetSystemMetrics`, `GetDoubleClickTime`,
  `GetCaretBlinkTime`, `SystemParametersInfoW`); macOS objc-runtime queries at
  `macos/system_style.rs:234 effectiveAppearance`, `:255 labelColor`, `:259 controlAccentColor`,
  `:315 doubleClickInterval`; Linux XDG portal at `linux/system_style.rs:80-115
  query_xdg_portal()` with GNOME/KDE fallbacks (`:491 discover_gnome_style`,
  `:562 discover_kde_style`). `DwmSetWindowAttribute` bound at `windows/dlopen.rs:597`.
  The superset struct is `SystemStyle` in `css/src/system.rs` (3495 lines, 48 tests).
- **Superseded by:** n/a — SYSTEMSTYLE_INTEGRATION_PLAN.md is the *consumption* half, not a
  replacement; this file is the *discovery* half and won.
- **Still open:** the materials/translucency dimension (Mica/Acrylic backdrop request,
  `NSVisualEffectView` material selection, Linux blur) is queried/bound but not exposed as a
  user-facing window option; sound themes and cursor/icon themes were never modelled.
- **Research value:** HIGH — this is the "native system styling/theming" concept: a
  three-platform mapping from each styling dimension to the exact API that yields the *resolved*
  value, with the explicit anti-pattern (parsing CLI output / registry keys) named. It is the
  single most reusable file in this cluster.

---

#### scripts/SYSTEMSTYLE_INTEGRATION_PLAN.md

- **Verdict:** ACTIVE — 4 of 7 tasks still unimplemented; the gap table it opens with is still literally true.
- **Was:** The companion plan for *consuming* `SystemStyle`: a table showing every OS-queried
  metric that is queried but never used, then Task A (feed `InputMetrics` into
  `GestureDetectionConfig`), B (caret blink/width from the OS instead of the hardcoded 530 ms),
  C (`wheel_scroll_lines` instead of the `* 20.0` hardcode), D (`TextRenderingHints` →
  WebRender `FontInstanceOptions`, incl. subpixel), E (color emoji COLR/SVG via allsorts+resvg),
  F (Wayland `xdg-decoration` CSD negotiation), G (KDE/GNOME detection tests), plus appendices
  with the xdg-decoration protocol, env vars, and X11 CSD properties.
- **Landed:** **Task F only.** `zxdg_decoration_manager_v1` / `zxdg_toplevel_decoration_v1` +
  listener are hand-built at `linux/wayland/defines.rs:43-57,749`, and `should_inject_csd` is at
  `dll/src/desktop/csd.rs:59`. The *query* side of every metric now exists (Task A/B/C inputs are
  populated — `windows/system_style.rs` GetDoubleClickTime/SM_CXDOUBLECLK/SM_CXDRAG/wheel lines
  at `:163`, `macos/system_style.rs:315` doubleClickInterval).
- **Superseded by:** n/a. Note the plan's `css/src/system_native_{macos,windows,linux}.rs` file
  layout is stale — that code moved into `dll/src/desktop/shell2/*/system_style.rs`.
- **Still open:** Task A — no `GestureDetectionConfig::from_input_metrics` exists;
  `layout/src/managers/gesture.rs:112` still hardcodes `drag_distance_threshold: 5.0` and there
  are **zero** `GestureManager::new` call sites in `dll/`, so `InputMetrics` reaches no consumer.
  Task B — `CURSOR_BLINK_INTERVAL_MS = 530` is still a const in
  `layout/src/managers/text_edit.rs`, used from `layout/src/window.rs:11291`;
  `caret_blink_rate_ms` has no reader outside `css/`. Task C — `wheel_scroll_lines` is read only
  in `windows/system_style.rs:163`; the scroll path still does `scroll_amount * 20.0` at
  `windows/mod.rs:3785-3786`. Task D — `webrender/glyph/src/font.rs:110` still forces
  `FontRenderMode::Alpha`. Task E — no COLR/CPAL handling anywhere in `webrender/glyph/src` or
  `layout/src/text3`. Task G — `dll/src/desktop/shell2/linux/system_style.rs` has **0** `#[test]`.
- **Research value:** low as design (it is a wiring TODO list), but the gap table is a reusable
  audit pattern: "queried ✅ / consumed ❌" per metric catches exactly this class of dead
  integration.

---

#### scripts/REMAINING_TITLEBAR_BUGS.md

- **Verdict:** DELETE — bugs 7/8/9/12 fixed with tests; 10 was already marked deferred; 11 obsolete.
- **Was:** Follow-up to HELLO_WORLD_LAYOUT_INVESTIGATION listing titlebar bugs 7–12: title text
  clipped for lack of `flex-grow:1`; title color hardcoded `#4c4c4c` ignoring dark mode;
  `TitlebarMetrics.padding_horizontal` never read; `discover_macos_style()` never queries real
  traffic-light geometry; button click not incrementing the counter via the debug API;
  titlebar should be `display:block` not flex.
- **Landed:** all in `layout/src/widgets/titlebar.rs` — Bug 7: `const_flex_grow(1)` at `:302` +
  `const_min_width(0)` at `:305` (and the second builder at `:872-873`); Bug 8:
  `title_color: ColorU` field `:117`, read from `system_style.colors.text` at `:205`/`:224` with
  the light/dark fallbacks, applied at `:297`/`:869`; Bug 9: `padding_horizontal` applied at
  `:191-192` with tests at `:1336`/`:1492`; Bug 12: block/flex are both emitted
  (`:240 Flex` / `:252 Block`, `:839`/`:843`) — the container is now switchable, not flex-only.
  Unit tests at `:1114`, `:1199`.
- **Superseded by:** n/a.
- **Still open:** Bug 10 only, and it was filed "deferred – low priority": `discover_macos_style`
  still uses hardcoded `TitlebarMetrics::macos()` values — no `standardWindowButton` /
  `contentLayoutRect` query anywhere in `macos/system_style.rs` or `macos/mod.rs`, and the
  requested "// Verified: macOS 11 – macOS 15" comment was never added. Bug 11 (debug-API click)
  is obsolete — the debug server moved to `common/debug_server/` and the synthetic-input path is
  now `common/e2e_test.rs`; the current e2e corpus exercises clicks end to end.
- **Research value:** none.

---

#### scripts/X11_API_REFERENCE.md

- **Verdict:** RESEARCH — primary-source EGL/Xlib/XIM/MIT-SHM reference with citations; still the working reference for the X11 shell.
- **Was:** A 2026-06-03 web-researched, quote-backed reference for the raw-Xlib+EGL X11 backend:
  §1 EGL presentation (the decisive Khronos rule that the color buffer is **undefined** after
  `eglSwapBuffers` unless `EGL_SWAP_BEHAVIOR == EGL_BUFFER_PRESERVED`, and that
  swap-with-damage still swaps the *whole* buffer so damage rects never excuse an undefined
  region); §2 non-blocking event drain, Expose `count`, ConfigureNotify storms, WM_DELETE_WINDOW;
  §3 `XLookupString` vs `Xutf8LookupString`, XKB, and the XIM/XIC setup order
  (`setlocale` → `XSetLocaleModifiers("")` → `XOpenIM`, `XFilterEvent` on *every* event);
  §4 CPU present via `XCreateImage`/`XPutImage` and the MIT-SHM fast path (setup order, depth
  match, BGRX swizzle, `ShmCompletion`); §5 GLX-vs-EGL pitfalls (the
  `eglCreatePlatformWindowSurface` takes `&window` footgun); plus a takeaways section and 8
  Khronos/spec source URLs.
- **Landed:** the load-bearing prescriptions are implemented — the whole IME chain is bound and
  used (`linux/x11/defines.rs:651 XFilterEvent`, `:703 XSetLocaleModifiers`, `:704 XOpenIM`,
  `:706 XCreateIC`, `:713 Xutf8LookupString`, `:715 XSetICFocus`, all loaded in
  `linux/x11/dlopen.rs:196-220`); the control-keysym rule became the backspace-tofu fix
  (`linux/x11/events.rs:874-875`, mirrored `linux/wayland/mod.rs:2673-2674`); FBO-0 clear and
  `XPending`-based drain are in the X11 shell.
- **Superseded by:** n/a.
- **Still open:** §4.2 MIT-SHM was never implemented — `linux/x11/dlopen.rs:168-169,258-259`
  binds only `XCreateImage`/`XPutImage`; no `XShmQueryExtension`/`XShmPutImage`/`ShmCompletion`
  anywhere, so the CPU present path is still the slow full-`XPutImage` round trip.
- **Research value:** HIGH — durable, citation-backed platform-API knowledge (the EGL undefined-
  back-buffer rule alone has bitten this project twice), and it is exactly the kind of
  cross-platform-windowing substrate the research bucket is for. Keep verbatim, sources included.

---

#### scripts/HANDOFF_LINUX_X11.md

- **Verdict:** ARCHIVE — 2099-line dated session/cron log; every actionable item is either landed or tracked elsewhere.
- **Was:** The Linux desktop-shell handoff log (2026-06-03 onward, branch `mobile-ios-android`):
  §0–§7 a Wayland-first status block (stale-self-pointer listener rebind, EGL backbuffer garbage,
  blank-window-on-empty-swap, idle/close unresponsive, xdg-decoration SSD, CPU-mode/`AZ_BACKEND`,
  hit-tester refresh) plus the X11 ports owed and the systemic double-drop analysis (#23);
  §7b input/IME/a11y research; §10 a Wayland-vs-X11 parity audit; then §12 = **56 numbered cron
  firings** on the X11 box, each a root-cause + commit (XI2 shadowing core mouse events, text
  selection, caret blink, CJK/CFF outline decode, Hangul cmap fallback, clipboard, menu
  positioning/dismissal/height-clamp, exit-GL crash, cpurender overflow double-draw, and finally
  the #47 CSS-cascade leak → `CssPathSelector::Root(CssScopeRange)`).
- **Landed:** X11 ports owed all present — control-char filter `linux/x11/events.rs:874`,
  CPU hit-tester rebuild `linux/x11/mod.rs:3538,3742 rebuild_from_layout_with_gpu`,
  X11 force-CPU `linux/x11/mod.rs:1659 AzBackend::resolve` + the CPU tuples at `:1691,1709,1754`.
  Its two "OPEN ARCHITECTURAL QUESTIONS" are both answered in code: **Q1** multi-window event
  routing → hybrid — child windows now share the parent's `Display`
  (`linux/x11/mod.rs:1387-1401` + `dispatch_shared_display_event` `:1043`, used at `:1025,2419,
  2486`) *and* the multi-window wait polls every window's fd (`run.rs:1733-1742`, X11
  `XConnectionNumber` / Wayland `wl_display_get_fd`), replacing the 16 ms spin; **Q2** menu
  dismissal → `XGrabPointer`/`XUngrabPointer` bound (`linux/x11/dlopen.rs:97,98,188`) and Wayland
  `xdg_popup` + `popup_done` (`linux/wayland/defines.rs:276,472-484`).
  #47 shipped as `CssPathSelector::Root(CssScopeRange)` + `scope_inline_css`
  (`core/src/styled_dom.rs:1207,2346`, tests `:4300-4327`).
- **Superseded by:** the 2026-07-31 seam audit (memory `azul-seam-audit-2026-07-31`) for the
  event-loop/Wayland findings, `CSS_ROOT_SCOPE_REFACTOR.md` + memory `azul-css-cascade-model`
  for #47, `DAMAGE_RENDERING.md` for incremental rendering, `X11_API_REFERENCE.md` for the API
  facts. Its branch/no-push instructions contradict the current push-to-master rule.
- **Still open:** its two final follow-ups. **A:** bare `set_css` width/height still may not
  size a node — the layout-hot restyle tiers in `core/src/prop_cache.rs` contain **no**
  `Root(...)` arm (grep: zero hits), so the layout-hot matcher plausibly never sees the scope
  selector; this is headless-reproducible and never re-tested. **B:** the FastDom/XML path was
  to be scoped too (`core/src/styled_dom.rs:975` still carries the scoping comment).
  Also still listed and unclosed: GPU partial-present (#30, needs `EGL_EXT_buffer_age` +
  a `PartialPresentCompositor`), and pen-pressure delivery into app callbacks needs the user's
  tablet.
- **Research value:** low as a whole (it is a log). Two transferable methods are worth extracting
  into one short note if anything is kept: (1) the headless render-verification loop
  (`AZ_BACKEND=headless` + `AZ_HEADLESS_SNAPSHOT_PATH` → read the PNG → patch one CSS prop → relink
  the C harness against `libazul.so` → re-render) as the only way found to verify render fixes
  without a live window; (2) the Q1/Q2 write-up of the two standard multi-window/menu-grab models
  (per-window connections vs one shared pump; XGrabPointer vs xdg_popup's built-in grab) — that
  part is genuine cross-platform windowing design and could fold into a research note.

---

### Tally

| Verdict | Files |
|---|---|
| DELETE (4) | PLATFORM_WINDOW_REFACTORING, PLATFORM_INTEGRATION_AUDIT, MENUBAR_HANDOFF_PROMPT, REMAINING_TITLEBAR_BUGS |
| RESEARCH (3) | PLATFORM_DND_MENU_RESEARCH, SYSTEMSTYLE, X11_API_REFERENCE |
| ACTIVE (2) | PLATFORM_DEBUG_PLAN, SYSTEMSTYLE_INTEGRATION_PLAN |
| ARCHIVE (2) | MENUBAR_INJECTION_PLAN, HANDOFF_LINUX_X11 |

### Relation to the 2026-07-31 seam audit / shell2-common hoisting

- **No conflicts.** PLATFORM_WINDOW_REFACTORING is a *completed* hoist of window **state** into
  `common/event.rs` (`CommonWindowState` + `impl_platform_window_getters` + provided event-processing
  defaults). The seam audit's proposal is a *different, unlanded* hoist: the **frame-lifecycle
  contract** (raise/consume/retire, wake sources, close consumption). `poll_event` /
  `wait_for_events` / the present gate are still defined per platform, so the refactor doc neither
  anticipates nor blocks it — it supplies the trait to hang it on.
- HANDOFF_LINUX_X11 §0 pre-dates and independently re-derives two seam-audit themes: the Wayland
  "drain the socket non-blockingly instead of relying on `eglSwapBuffers`" fix (an early instance of
  "no blocking calls on the UI thread") and the present gate ("swap only when `total_draw_calls > 0`").
- PLATFORM_DEBUG_PLAN's Phase 4 is the missing verification layer the seam audit also flagged: the
  e2e corpus cannot see platform present paths.


## Part 10 — mobile platforms, cross-compilation, sensors/stylus/gamepad/biometrics

Verified against master @ f1c43ba60 (2026-08-01). All `path:line` below are from the
current working tree, not from the docs' own status lines.

---

#### scripts/ANDROID_IMPLEMENTATION_PLAN.md

- **Verdict:** DELETE — Phases 1–8 shipped; the "zero Java" premise is superseded.
- **Was:** A full 8-phase bring-up plan for an Android backend that did not exist at all
  (no `shell2/android/`, no NDK deps, no build.rs). Chose `android-activity` +
  NativeActivity + `ANativeWindow_lock` CPU blit over EGL/GameActivity/Gradle, and
  specified an aapt2/zipalign/apksigner APK pipeline plus a ~50-line
  `NativeInputConnection.java` IME bridge (Phase 5).
- **Landed:** `dll/src/desktop/shell2/android/mod.rs` (1337 lines) + `android/accessibility.rs`
  (533); module wiring at `dll/src/desktop/shell2/mod.rs:29-30,61-63`; deps at
  `dll/Cargo.toml:295,297,310`; `configure_android()` at `dll/build.rs:112,195-205`;
  `scripts/build-android.sh` (187 lines, ABI map at `:33-35`, `lib/$ABI/` at `:57,78,145`);
  `scripts/android/AndroidManifest.xml`. Event loop `android_main` at
  `android/mod.rs:430`, `handle_poll_event:561`, `drain_input:671`, `map_keycode:940`,
  `render_frame:964` (the ANativeWindow lock/blit). CI: `.github/workflows/rust.yml`
  `build_mobile:3009`, `build_mobile_apps_android:3303` (arm64 + `x86_64-linux-android:3020`),
  and an emulator install/launch job `post-release.yml:313 apk_install`.
- **Superseded by:** the plan's key design decision #1 ("zero Java code"). The tree now
  ships four Java bridges — `scripts/android/AzulActivity.java`,
  `AzulAccessibilityBridge.java`, `AzulFilePicker.java`, `NativeGestureBridge.java` — and
  `AndroidManifest.xml:26` launches `com.azul.app.AzulActivity`, not
  `android.app.NativeActivity`. Any future IME work belongs on that bridge, not on the
  doc's `NativeInputConnection`.
- **Still open:** Phase 5 soft keyboard / IME is **not implemented anywhere** —
  `NativeInputConnection` appears only in docs (`grep` hits: ANDROID_IMPLEMENTATION_PLAN.md,
  SUPER_PLAN.md, research/04, and a dead `dll/Cargo.toml` comment); no
  `show_soft_input`/`hide_soft_input`/`commitText` anywhere in `dll/src`. Android text
  input is hardware-keycode only. Phase 8 (AAB/Play Store) never attempted.
- **Research value:** none beyond the code — the ANativeWindow-lock-vs-EGL and
  no-Gradle-packaging rationale is now embodied in `build-android.sh` and `android/mod.rs`
  doc comments.

---

#### scripts/IOS_IMPLEMENTATION_PLAN.md

- **Verdict:** DELETE — Phases 1–3, 5, 7 shipped; only text input + clipboard left.
- **Was:** A 7-phase plan to take a ~305-line stub iOS backend to a working app: fix a
  literal `pub mod linux;` typo under `cfg(target_os="ios")`, add CPU rendering via
  `CGImage` → `CALayer.contents`, a `CADisplayLink` tick, `UITouch` → `process_window_events`,
  `UIKeyInput`/`UITextInput` text entry, lifecycle + safe-area/orientation, `UIPasteboard`,
  a11y, and an Xcode-project-free `.app` bundle/sign/simctl pipeline.
- **Landed:** `dll/src/desktop/shell2/ios/mod.rs` (1617 lines) + `ios/accessibility.rs` (603).
  Module fix at `shell2/mod.rs:33-34,58-60`. CGImage blit `display_layer` at `ios/mod.rs:207`
  with `release_frame_pixels:161`; CADisplayLink built at `:913-925`, tick at `:823`; touch
  handling `handle_touch:304` + `touches_began/moved/ended/cancelled:528-552`;
  `layout_subviews:567` with `safeAreaInsets` at `:613`; full AppDelegate lifecycle
  `:1039,1083,1096,1104,1116,1121`. Pen feed `update_pen_state_full` at `ios/mod.rs:504`.
  `scripts/build-ios.sh` (141 lines) + `scripts/ios/Info.plist` + `entitlements.xcent`.
  CI `.github/workflows/rust.yml:3201 build_mobile_apps` produces per-example `.app` zips.
- **Superseded by:** partly — the plan's "no additional crates, raw `objc` 0.2" line held for
  the shell (`dll/Cargo.toml:198`), but the surrounding feature backends went `objc2`
  (`dll/Cargo.toml:207-276`), so the tree runs a two-runtime split the plan didn't anticipate.
  Also, the plan predates the native-gesture seam: `ios/mod.rs:665-776` wires
  UIKit `UIGestureRecognizer` → `inject_native_gesture`, which no phase describes.
- **Still open:** Phase 4 (both levels) — no `UIKeyInput`, no `insertText:`, no
  `becomeFirstResponder` anywhere in `dll/src`; iOS has **no text input at all**. Phase 6
  clipboard — zero `UIPasteboard` references in `dll/src`. App Store IPA/actool/altool path
  never built.
- **Research value:** none — CGImage-from-RGBA and CADisplayLink recipes now exist as working
  code.

---

#### scripts/MOBILE_API_REVIEW.md

- **Verdict:** DELETE — the audit was acted on; T1/T2 fully, T3 nearly.
- **Was:** A 2026-05-20 design review of every P2–P7 feature against "the Azul way". Verdict:
  structurally sound but "data flows one way and must be POLLED" — no backreference
  `set_on_X` hook on any producer; capture widgets were one-way streets (so azul-meet was
  "0% feasible"); PDF export was a process-global channel that discarded its `ok` bool. Plus
  a "globals nuance" (per-process transport vs per-window manager → cross-window bleed) and
  a tiered T1/T2/T3 fix list.
- **Landed:** T1 — `set_on_frame`/`with_on_frame` on `layout/src/widgets/camera.rs:82,92`,
  `screencap.rs:75,85`, plus `widgets/video.rs` and `widgets/microphone.rs`; `VideoFrame` is
  now `core/src/video.rs:89`. T2 — `HoverEventFilter::SensorChanged`/`GamepadInput`/
  `BiometricResult`/`KeyringResult` at `core/src/events.rs:669,672,693,696` (and the
  Window mirrors at `:1329`); MapWidget `set_on_viewport_changed`/`set_on_pin_tap` at
  `layout/src/widgets/map.rs:197,223`. PDF uncouple — `PENDING_EXPORTS` and
  `CallbackInfo::export_to_pdf` no longer exist; the standalone entry point lives at
  `dll/src/unified/pdf.rs` / `dll/src/desktop/extra/pdf/mod.rs`. T3 —
  `CallbackInfo::get_permission_status` implemented at `layout/src/callbacks.rs:3669`;
  `DbRows`/`DbValue`/`DbValueVec` are in `api.json`.
- **Superseded by:** n/a.
- **Still open:** (a) **Wacom pad backend is still dead on every platform** — `WacomPadState`
  exists (`layout/src/managers/gesture.rs:394`) but the only caller of `update_pad_state` is
  the unit test at `gesture.rs:3112`; no Wintab/libwacom/NSEvent producer
  (`grep -l Wintab dll/src` → empty). (b) The dead doc in `core/src/camera.rs:5,14,20,89`
  still names `azul_layout::managers::camera` / `CameraStream` / `start_camera` — none exist
  (`layout/src/managers/` has no `camera.rs`; no `fn start_camera` in the tree). (c) No
  gamepad rumble and no `GamepadConnected`/`Disconnected` event variants.
- **Research value:** the "push, don't poll + backreference hook on every producer" rule and
  the pen-precedent argument against per-process transport channels are the durable part, but
  they're now enforced by shipped code rather than by this doc.

---

#### scripts/MOBILE_SESSION_LOG.md

- **Verdict:** ARCHIVE — 404 KB append-only cron log, superseded by git history.
- **Was:** An append-only "one entry per cron tick" journal opened 2026-05-19 for the
  `mobile-ios-android` branch. 268 headings but only **one** `## 2026-` date header (line 9) —
  the intended daily grouping never materialised; everything is flat `### Tick — …` /
  `### Sprint … GATE` entries. Content drifted well past mobile: Android/iOS bring-up →
  gesture api.json codegen → e2e debug-server events → CI green → crates.io release chain →
  Docker/GHCR → an apt repository. Entries are narrative + commit SHAs + "cargo check GREEN
  in N s".
- **Landed:** everything the tail flags as shipped is verifiable — cdylib+`ctor` demo APKs
  (commit 3b5a53376, `examples/azul-maps/src/lib.rs`), crate-parametric build scripts
  (f137d77e2), `build_mobile` drop-in libs job (`rust.yml:3009`).
- **Superseded by:** git log + the CI workflows. The log's own "STILL OPEN: ship prebuilt
  mobile libazul per target" and "per-example APK/.app CI (#8)" are both now done
  (`rust.yml:3009,3201,3303`).
- **Still open:** two decisions it escalated were later resolved by implementation (X11 XI2
  hand-roll at `linux/x11/defines.rs:605,643`; Wayland tablet-v2 at
  `linux/wayland/defines.rs:1246,1435`), so nothing survives as an open item. The
  `scripts/mobile/golden/` snapshot dir it references is still empty (`.gitkeep` only).
- **Research value:** none as a document; a couple of hard-won facts are worth carrying
  elsewhere — Homebrew rustc shadowing rustup breaks all mobile cross-compiles, and
  `css/src/corety.rs::from_c_str` had to take `*const i8` because `c_char` is unsigned on
  Android while api.json emits `i8` literally.

---

#### scripts/CROSS_COMPILE_COMPAT.md

- **Verdict:** RESEARCH — durable retro-OS floor + API-introduction-date reference.
- **Was:** A 2026-04-11 audit arguing that because every desktop backend dlopens its platform
  APIs (LoadLibraryW/dlsym/objc runtime), azul is close to running on very old OSes. Gives a
  per-DLL / per-framework / per-.so table with minimum OS versions, three named soft-fallback
  gaps, a 32-bit pointer-size checklist, and three API-introduction-date tables
  (Win95→Win11 22H2, macOS 10.0→10.14, Xlib 1987→Wayland 2012).
- **Landed:** all three "Soft Fallbacks Implemented" claims verify —
  `SetWindowLongPtrW` → `SetWindowLongW` at
  `dll/src/desktop/shell2/windows/dlopen.rs:670-674`; `has_visual_effect_view()` at
  `dll/src/desktop/shell2/macos/mod.rs:5066` (used at `:5083,5119`);
  `respondsToSelector:` scroll guard at `dll/src/desktop/shell2/macos/events.rs:399`.
  `scripts/cross_check.sh` exists. The "opt-in" rust9x item went much further than planned:
  a whole `.github/workflows/rust9x.yml` builds the rust9x stage1 compiler and publishes it
  as a permanent release asset for a `build_rust9x` consumer job.
- **Superseded by:** n/a — but one factual claim has decayed: "No static linking to any Win32
  DLL" no longer describes the whole build. Commit 4d6122ec4 ("name every system DLL the
  prebuilt azul.lib references") means the shipped import library does name system DLLs, and
  `winapi`/`windows` crates now link `advapi32` for keyring/Hello (`dll/Cargo.toml:332,337,339`).
- **Still open:** item 4, the `TrackMouseEvent` soft fallback, is **not** done —
  `dll/src/desktop/shell2/windows/dlopen.rs:696` uses `get_symbol("TrackMouseEvent")?`
  (hard fail), unlike the neighbouring `GetPointerType`/`GetPointerPenInfo` which use
  `.ok()`. That keeps the Win95 floor closed. Pre-10.5 macOS `addTrackingRect:` fallback also
  never attempted (accepted: 10.5 is the floor).
- **Research value:** **high, and the best keeper in this cluster.** The transferable concept
  is "dlopen-everything as a portability strategy": if no platform symbol is statically bound,
  the minimum supported OS becomes a per-call-site policy (`?` = required, `.ok()` = optional)
  rather than a link-time constant — and the API-introduction-date tables are exactly the data
  needed to audit that policy. Belongs in `scripts/research/`.

---

#### scripts/DESKTOP_SENSOR_BACKENDS_RESEARCH.md

- **Verdict:** DELETE — all five backends implemented exactly as specified.
- **Was:** A dense 2026-05-21 wiring brief for tasks #8/#9/#10: Windows motion sensors (WinRT
  `Windows.Devices.Sensors`, with unit conversions and the cross-compile-to-gnu argument for
  the `windows` crate over `windows-sys`), Windows keyring (Win32 `CredWriteW`/`CredReadW`/
  `CredDeleteW`), Windows biometric (Windows Hello `UserConsentVerifier` + the
  `IUserConsentVerifierInterop` HWND requirement on Win32), Linux keyring (libsecret via
  dlopen, insisting on the non-variadic `*v_sync` forms because stable Rust cannot call
  variadic fn-pointers), Linux biometric (fprintd over D-Bus via the already-present `zbus`,
  explicitly not PAM).
- **Landed:** every item, in the files it named.
  `dll/src/desktop/extra/sensors/windows.rs:15` (`Accelerometer, Gyrometer, Magnetometer`);
  `extra/keyring/windows.rs:18,73,83` (`CredWriteW`/`CredReadW`);
  `extra/biometric/windows.rs:1-15` (`UserConsentVerifier` + the interop note);
  `extra/keyring/linux.rs:1-8,95` (libsecret dlopen, `*v_sync` rationale in the header);
  `extra/biometric/linux.rs:1-14` (fprintd `net.reactivated.Fprint` over zbus, commit
  73ea61542). Cargo features present at `dll/Cargo.toml:332,337,339`. The task-#10 tail
  (azul-vault on the public api.json surface) closed in commit df32dedd1.
- **Superseded by:** n/a.
- **Still open:** none.
- **Research value:** none — it is a wiring checklist, and the reasoning that mattered (why
  `*v_sync`, why not PAM, why the interop HWND) was copied into the module headers.

---

#### scripts/WACOM_TOUCH_API_RESEARCH.md

- **Verdict:** DELETE — the ABI tables were transcribed into `defines.rs` and work.
- **Was:** A 2026-05-21 blind-ABI reference for the Linux pen/touch feeds, written because the
  X11 shell hand-rolls its Xlib bindings: exact `#[repr(C)]` field orders for XI2
  (`XIDeviceEvent`, `XIValuatorClassInfo`, `XGenericEventCookie`), the packed-valuator decode
  rule, XI evtype constants; then `wl_touch` + `zwp_tablet_v2` protocol tables; then a
  "MARSHALLING FIX SPEC" declaring the Wayland backend **non-functional at registry-bind**
  (the `transmute` wrapper fields dropped both opcode and interface, and `wl_fixed_t` was
  typed `f64` instead of `i32`); then hand-rolled `wl_interface` descriptor data for all nine
  tablet-v2 pen *and pad* objects (pad descriptors needed only so eager `pad_added` doesn't
  crash).
- **Landed:** XI2 — `dll/src/desktop/shell2/linux/x11/defines.rs:605` (`XIDeviceEvent`),
  `:643` (`XISelectEvents`), with the decode + feed in `linux/x11/mod.rs` and
  `update_pen_state_full` at `linux/x11/mod.rs:693`. Wayland — `zwp_tablet_pad_v2` struct at
  `linux/wayland/defines.rs:1246`, its hand-rolled `wl_interface` builder at `:1435`, tool
  listener wiring in `linux/wayland/events.rs` (`ZWP_TABLET_TOOL_LISTENER`), pad handler at
  `events.rs:787`, `wl_touch` listener types in `defines.rs` with the "x/y are wl_fixed_t
  (i32, 24.8); /256.0 in the handler" note applied, and the pen feed at
  `linux/wayland/mod.rs:2837`. All five platforms now call `update_pen_state_full`
  (macos/events.rs:267, ios/mod.rs:504, windows/mod.rs:2592, x11:693, wayland:2837).
- **Superseded by:** n/a.
- **Still open:** the pad half is descriptors-only by design (parse-and-drop, no listeners),
  which is why `WacomPadState` has no producer — see the MOBILE_API_REVIEW entry.
- **Research value:** low now. The one transferable lesson — "Rust cannot call variadic
  fn-pointers, so every libwayland request must be transmuted to a concrete per-request
  signature that re-injects the hardcoded opcode + `wl_interface`" — is worth one paragraph
  if the Wayland backend is ever rewritten, but the working `defines.rs` is the better
  reference.

---

#### scripts/research/02_biometric_auth.md

- **Verdict:** RESEARCH — keep; the "step 2" half was never built.
- **Was:** A 5-platform inventory of native biometric APIs (iOS `LAContext`, Android
  `BiometricPrompt`, macOS `LAContext`, Windows `UserConsentVerifier`, Linux polkit/fprintd)
  with the W3C WebAuthn mapping, proposing an `App::request_biometric_auth(...)` surface on
  the *platform backend → manager override slot → `CallbackInfo` accessor* pattern. Its
  central argument: a correct integration delivers **two** outputs — (1) an authentication
  assertion, and (2) a hardware-bound signed assertion from the secure element
  (Secure Enclave / TrustZone / TPM), because step-1-only apps are replay- and
  rooted-device-vulnerable.
- **Landed:** step 1 only, on all five platforms —
  `dll/src/desktop/extra/biometric/{apple,android,linux,windows}.rs` (616 lines total) behind
  `extra/biometric/mod.rs`; manager at `layout/src/managers/biometric.rs`; completion event
  `HoverEventFilter::BiometricResult` at `core/src/events.rs:693`; the four public types
  (`Biometric`, `BiometricKind`, `BiometricPrompt`, `BiometricResult`) are in `api.json`.
- **Superseded by:** n/a — `scripts/DESKTOP_SENSOR_BACKENDS_RESEARCH.md` §3/§5 is the
  narrower implementation brief that consumed this doc's Windows/Linux sections.
- **Still open:** **step 2 in its entirety.** Zero hits across `dll/`, `core/`, `layout/` for
  `SecureEnclave`, `SecKeyCreateRandomKey`, `KeyCredentialManager`, `PublicKeyCredential`, or
  `passkey`. Also unbuilt: the WebAuthn mapping (§9) for the future web backend, and
  `scripts/mobile/golden/biometric.png` (the dir is an empty `.gitkeep`).
- **Research value:** high. The transferable concept is the **two-tier biometric abstraction**
  — expose a cheap "unlock the settings panel" boolean *and* a hardware-bound challenge/response
  signature over the same API, so security-sensitive callers are not silently served the weak
  path — plus the platform-by-platform mapping of that split onto Keychain/Keystore/TPM and
  onto W3C `UserVerification` vs `PublicKeyCredential`. Correctly placed in `scripts/research/`.

---

#### scripts/research/03_sensors_gamepad_stylus.md

- **Verdict:** RESEARCH — keep; gamepad-on-mobile and tablet-pad remain unbuilt.
- **Was:** A 50 KB, 3-feature brief (motion sensors, gamepads, Wacom/stylus) covering all five
  platforms per feature — `CoreMotion`, Android `SensorManager`, macOS `IOHIDManager`, Linux
  IIO sysfs, `Windows.Devices.Sensors`; `GameController.framework`, Android
  `SOURCE_GAMEPAD`, evdev, `Windows.Gaming.Input`, and a `gilrs` build-vs-buy analysis;
  Apple Pencil / Android stylus / `NSEvent` tablet / libwacom+libinput / Wintab-vs-Windows-Ink.
  Proposes `SensorManager`, `GamepadManager` and an extended `PenState` + `TabletPadManager`,
  ends with a W3C cross-reference table, a sprint ordering, and ~10 explicit
  `TODO: verify` items.
- **Landed:** sensors on all five — `dll/src/desktop/extra/sensors/{apple,android,linux,windows}.rs`
  (538 lines) + `layout/src/managers/sensors.rs` + the `SensorChanged` filter
  (`core/src/events.rs:669`). Stylus fully — the proposed field extensions exist at
  `layout/src/managers/gesture.rs:339,341,347,354,360` (`is_eraser`, `barrel_button_pressed`,
  `tangential_pressure`, `barrel_roll_rad`, `tool_id`) and `PenSqueeze`/`PenDoubleTap` are
  real filters (`core/src/events.rs:1727,1730`); all five backends feed
  `update_pen_state_full`. Gamepad partially — desktop via the `gilrs-azul` fork
  (`dll/Cargo.toml:169`, `extra/gamepad/desktop.rs`, 164 lines) + `GamepadInput` filter.
- **Superseded by:** n/a.
- **Still open, concretely:**
  - `dll/src/desktop/extra/gamepad/apple.rs` (17 lines) and `android.rs` (16 lines) are
    **doc-comment stubs** — `pub fn start() {}` / `pub fn poll() {}`. No `GCController`
    backend on iOS, no `AzulGamepad.java` helper on Android, so `get_gamepad_state()`
    returns `None` on both mobile platforms.
  - Rumble: no `rumble` symbol anywhere in `layout/src/managers/gamepad.rs` or
    `extra/gamepad/`, despite the doc insisting rumble lands with discovery.
  - `TabletPadManager` (step 6): `WacomPadState` type exists, producer does not (see
    MOBILE_API_REVIEW).
  - The ~10 `TODO: verify` hardware questions (Windows accelerometer pre-rotation, gilrs's
    macOS path, PS5 evdev motion node, macOS Input-Monitoring for HID digitizer reads,
    Apple Pencil hover semantics) are all still unanswered — none is device-testable in CI.
- **Research value:** high. Transferable concepts: (1) the "superset of every platform" API
  design rule — one surface, best-available implementation per backend, `Unsupported` rather
  than a missing method; (2) the build-vs-buy analysis for `gilrs` (why a desktop crate is
  taken but mobile needs native `GameController`/`InputDevice` paths); (3) the pen-capability
  model (pressure/tilt/rotation/tangential/barrel-roll as an optional-capability set, mapped
  onto Pencil, Wintab, Windows Ink, libwacom and `zwp_tablet_tool_v2` at once). Correctly
  placed in `scripts/research/`.


## Part 11 — web / WASM backend plans and milestones (10 files)

### HEADLINE: the web/WASM backend is **ALIVE**, not dormant, not dead

Evidence (all verified in the working tree, not from doc status lines):

- **21,228 lines of shipping Rust** in `dll/src/web/` across 13 modules
  (`dll/src/web/transpiler_remill.rs` alone is 9,600 lines / 463 KB;
  `symbol_table.rs` 3,371; `eventloop.rs` 2,488; `loader_js.rs` 1,113).
- Wired into the crate root: `dll/src/lib.rs:180-184` (`pub mod web;` behind
  `feature = "web"`), features defined at `dll/Cargo.toml:724` (`web`),
  `:751` (`web-transpiler`), `:763` (`web-transpiler-static`).
- **Recent commits**: last change to `dll/src/web/` is `85f1df8b8`, **2026-07-29**
  (3 days before this audit). Cadence: 174 commits 2026-05, 35 in 2026-06,
  5 in 2026-07 — decelerating but current. July commits are substantive:
  `fe13d510b` (wasm-opt fallback + brotli on the wire), `44a3b0fad`
  (`AZ_BACKEND=web-prelift`), `8d694e263` (git-ref-addressed lift cache).
- **Live CI/CD**: `.github/workflows/dockery.yml` builds+publishes
  `ghcr.io/fschutt/azul-web-base` (pre-lifted WASM cache, `AZ_BACKEND=web-prelift`)
  and is dispatched from `.github/workflows/rust.yml:5220` on **every website
  publish**. `.github/workflows/docker-base.yml` builds the
  `web-transpiler-static` (in-process remill/LLVM/LLD) image.
- **Every shipped demo has a web Dockerfile**: `examples/azul-meet/Dockerfile`,
  `azul-paint`, `azul-vault`, `azul-camera`, `azul-screenshare`,
  `azul-spirit-level`, `azul-self-test` — all `FROM ghcr.io/fschutt/azul`.
- **21 `examples/c/web-*.c` test apps** + 40 harness scripts in `scripts/m9_e2e/`.
- `wasm32-unknown-unknown` is a first-class CI target (`rust.yml:424`, sub-crates
  only, `check_build_dll: false`), and `core/Cargo.toml:48` /
  `layout/Cargo.toml:174` carry a real `web_lift` feature that shipping code
  `#[cfg]`s on (`layout/src/solver3/fc.rs:394-403`, `cache.rs:1985`, `sizing.rs:681`).
- 60+ `AZ_*` env knobs are live in the web module (`AZ_PREFLIGHT`, `AZ_LIFT_CACHE`,
  `AZ_ENABLE_SHARDS`, `AZ_NATIVE_REMILL`, …).

**Consequence: none of the 10 files is deletable "because the effort died."**
Each has to be judged on whether its *specific* content landed.

#### Architecture, in one line
This is **not** a `wasm32-unknown-unknown` port. The server runs the native
libazul, **lifts x86-64/aarch64 machine code → LLVM IR → wasm32 via remill**
at startup, pre-renders HTML per route, and ships lifted callbacks + an
`azul-mini.wasm` eventloop to the browser. `dll/Cargo.toml:336-340` explicitly
rejects the "just compile azul to wasm32" alternative as a different product.

#### Supersession chain (chronological, verified)

```
WEB_BACKEND_PLAN_2026_05_18  (M0–M10 roadmap)
  └─ M8_ARCHITECTURE_2026_05_19  (expands M8 → M8.0…M8.10)
       ├─ M8_7_HYDRATION_PLAN_2026_05_16   (M8.7 sub-plan; landed as hydration.rs)
       ├─ M8.8_NEW_SESSION_PROMPT          (M8.8 SymbolTable; self-marked CLOSED)
       └─ M9_WASM_DOM_HANDOFF              (self-marked SUPERSEDED)
            └─ M9_REVIEW_AND_OPTION_A      (synthetic addrs — LANDED)
WASM_SHIPPING_OPTIONS (05-18, decision deferred) ──► resolved by M10-D/M10-E shards
WEB_BACKEND_1TO1_PLAN (06-10, architecture reference — still cited)
  └─ WEB_1TO1_SUPERPLAN (06-11/12, live S0–S7 ledger)  ── S2–S7 STILL OPEN
WEB_WASM_DIET_PLAN_2026_07_04 (newest; L0 partly landed, L1–L3 open)
```

**Winner:** `WEB_WASM_DIET_PLAN` (size strategy) + `WEB_BACKEND_1TO1_PLAN` §6/§6b
(the architectural rulings) are the live design. `WEB_1TO1_SUPERPLAN`'s slice
list S2–S7 is the only surviving open-work checklist. Everything M0–M9 is done.
Code milestone markers now run to **M12.7** — past the last of these docs.

---

#### scripts/WEB_1TO1_SUPERPLAN.md

- **Verdict:** ACTIVE — sole live checklist; S2–S7 unimplemented, g147 bypass still in tree.
- **Was:** 106 KB living progress ledger for "make the web backend run azul apps 1:1
  with desktop," written 2026-06-11/12 during a cron-driven autonomous loop. ~90 % is a
  chronological bug-hunt log (class-A/class-B sret bugs, "MECHANISM B", trap hunts I–IV,
  relift timings) with the newest entries at the top and dozens of `(historical)` /
  `(superseded)` sections. The load-bearing 10 % is §"Slices (user's order)" (S0–S7) and
  §"Key architecture facts (verified 2026-06-11)".
- **Landed:** S0 infra — lift cache (`AZ_LIFT_CACHE`, `dll/src/web/transpiler_remill.rs`),
  preflight gate `AZ_PREFLIGHT` (env knob present), content-keyed object cache; the
  `mrs/msr NZCV` remill-fork fix. S1 input events — `AzStartup_dispatchEvent` at
  `dll/src/web/eventloop.rs:2361-2420` implements the S1 broadcast routing
  (RESIZE/SCROLL/KEYDOWN/KEYUP → `u32::MAX`), focus tracking, `AzStartup_hitTest` on
  SENTINEL, `viewport_w/h`. MECHANISM B fix (alloc/core default `Recursable`) is in
  `symbol_table.rs`'s classifier. ua_css/S7 verified pixel-identical per the log.
- **Superseded by:** n/a — it is the newest *slice* tracker. Its size strategy is
  superseded by `WEB_WASM_DIET_PLAN_2026_07_04`.
- **Still open:**
  - **S2 (CSS out)** — implemented then *reverted* (§"CONSOLIDATING to green"). Verified
    absent today: zero hits for `CallbackChange` / `take_changes` / `format_css` in
    `dll/src/web/eventloop.rs`. Callbacks still receive raw event bytes as their
    `CallbackInfo` ptr. `examples/c/web-setcss-min.c` + `scripts/m9_e2e/web-setcss-cdp.js`
    exist as the waiting gate.
  - **S3 timers** — no `tickTimer`, no `AddTimer` TLV kind, no `setInterval` in
    `dll/src/web/loader_js.rs`. `Instant::now` HostCall not injected (no `HostCall`
    variant in `FnClass`, `symbol_table.rs:108-208`).
  - **S4 images / S5 threads (Web Workers) / S6 `AzHttp_*` → fetch** — none present.
  - **g147 FC-assignment mis-lift** — the carried-forward bug. Its `web_lift`-gated
    bypass is still live at `layout/src/solver3/fc.rs:394-403` ("Remove when the
    FC-assignment mis-lift is fixed"), plus diag markers at `fc.rs:383-463`,
    `cache.rs:1981-2058`, `sizing.rs:681,736`.
  - S1 100 % close-out blocked on ≥19-node `web-events.c` (class-B).
  - `AZ_NATIVE_REMILL=1` e2e validation still unmeasured.
- **Research value:** low as prose; its `mechb_harness` method (lift a suspect function,
  execute it natively against mock State/memory, trace mem-ops) is a genuinely
  transferable debugging technique for any binary-lift pipeline. Worth ~1 page if the
  rest is retired.
- **Note:** if kept ACTIVE, it should be *truncated* — the `(historical)`/`(superseded)`
  sections (roughly lines 190–435 and 578–1300) are pure log and belong in git only.

---

#### scripts/WEB_BACKEND_1TO1_PLAN.md

- **Verdict:** RESEARCH — §6/§6b are the durable DOM-as-render-target + host-call rulings.
- **Was:** 2026-06-10 architecture reference for closing the gap from "the counter works"
  to "the web backend runs the App 1:1 with desktop." Contains the mental model ("the WASM
  module *is* the azul App; JS is a thin host"), a now/target gap table, a 5-phase plan
  (event loop → diff → visual layer → timers/threads → full events), the wire protocol,
  and two **resolved user decisions**: §6 the render-target model and §6b the host-call
  injection layer.
- **Landed:** Phase 0.2 preflight (`AZ_PREFLIGHT`). Wire protocol confirmed shipped:
  `PATCH_KIND_*` 1–12 at `dll/src/web/eventloop.rs:2077-2088`, `AzStartup_buildPatch` at
  `:2183`, `azApplyPatches` in `loader_js.rs`. `core/src/diff.rs::reconcile_dom` exists and
  `eventloop.rs:274,966` now references `reconcile_dom_with_changes`-shaped logic.
- **Superseded by:** n/a for §6/§6b (never revisited). Its Phase list is *tracked* by
  `WEB_1TO1_SUPERPLAN`'s S2–S7 — the two overlap, and the SUPERPLAN is the live one.
- **Still open:** Phases 1–5 essentially all of them — no `process_window_events` /
  `dispatch_events_propagated` / `apply_user_change` in `dll/src/web/eventloop.rs` (grep:
  zero hits); no timers, no Web Workers; §2's "arena `NodeId` == `az_N`" invariant is
  still an *unasserted* contract (no debug check in `html_render.rs`) — the plan's own
  flagged linchpin risk.
- **Research value:** **HIGH — the best keeper of the ten.** Two transferable concepts:
  (1) **"browser DOM as a passive render target"** — the WASM owns cascade + layout +
  text layout and emits only *semantic* patches (text/class/inline-style/structure);
  never glyph positions, never `getBoundingClientRect`, zero measurement round-trips. 1:1
  fidelity comes from making the layout engine spec-accurate, not from measuring the
  browser. This is precisely the software-rasterizer-vs-canvas tradeoff resolved *against*
  canvas — the opposite of Flutter Web's CanvasKit and of egui's canvas backend, and the
  reason azul-on-web keeps real DOM text, selection, and a11y. (2) **§6b host-call
  injection at the IR layer** — a fixed, tiny set of host services (clock, fetch, sensors,
  WebRTC-for-UDP) detected by symbol name during lift and given synthetic JS-import
  bodies, so user code calling `Instant::now()` is oblivious; with an ABI that supports
  both sync-return and post-result-later. That is a reusable pattern for any
  lift/emulation-to-browser system.

---

#### scripts/WEB_BACKEND_PLAN_2026_05_18.md

- **Verdict:** DELETE — M0–M10 roadmap fully executed, superseded twice over.
- **Was:** The original 10-milestone roadmap (2026-05-18) to make
  `examples/c/hello-world.c` clickable in a browser: M0 unblock compile errors, M1 verify
  server-side `POST /az/exec/`, M2 `dladdr` callback discovery, M2.5 codegen wrapper pairs,
  M3 no-op WASM per callback, M4 browser fetch/instantiate/dispatch, M5 swap in
  `RemillTranspiler::lift_function`, M6 intrinsic lowering, M7 symbol intercept, M8
  `azul-mini.wasm` HeadlessWindow simulator, M9 browser patcher, M10 e2e validation.
- **Landed:** All of it. `discover_and_transpile_callbacks` (M2) is documented shipping at
  `doc/guide/en/internals/web.md:474`; `azul-mini.wasm` + `AzStartup_*` (M8) is
  `dll/src/web/eventloop.rs` with 37 `#[no_mangle]` exports; the browser patcher (M9) is
  `loader_js.rs::azApplyPatches`. Code markers M5/M6/M7 appear throughout `dll/src/web/`.
- **Superseded by:** `scripts/M8_ARCHITECTURE_2026_05_19.md` (which explodes M8), then the
  whole M9→M12 chain. Its own M8 section says "user-driven; revised direction."
- **Still open:** none traceable to this doc.
- **Research value:** none — the only mildly interesting idea (stage runtime wiring before
  lift correctness so each failure has one possible source) is a generic de-risking rule.

---

#### scripts/WEB_WASM_DIET_PLAN_2026_07_04.md

- **Verdict:** ACTIVE — newest plan; L0 half-landed, L1/L2/L3 untouched, mini still ~25 MB.
- **Was:** The binary-size plan (2026-07-04, self-marked "PLAN ONLY"). Diagnoses that
  `azul-mini.wasm` at ~25 MB is the *expected* output of lifting ~9.6 MiB of native text
  (layout+core+css+allsorts+taffy+alloc+hashbrown), against an original <500 KB budget
  (`M8_ARCHITECTURE_2026_05_19.md:709`). Four composable levers: **L0** recover pipeline
  losses (wasm-opt fallback, brotli wire), **L1** classifier-level subsystem cuts +
  browser-native substitution (image decode, hyphenation, font parsing, ICU4X), **L2**
  per-app reachability at server startup, **L3** module layering + per-fn shards
  (boot/layout/cb/rare bundles). Plus §7 explicit non-goals.
- **Landed (L0 only):** brotli on the wire is **shipped** — `dll/src/web/server.rs:658`
  `brotli_compress`, `:669` `Accept-Encoding` sniff, `:679-733` the wasm-serving path with
  `Content-Encoding: br` + `Vary`, `:230` the precomputed brotli for the hot mini
  (commit `fe13d510b`). wasm-opt fallback partly addressed: `postprocess_wasm_opt` at
  `transpiler_remill.rs:5631` now enumerates features explicitly (`:5648` "we list them
  explicitly rather than `--all-features`") and errors loudly (`:5692`), with
  `AZ_REMILL_SKIP_WASM_OPT` as the escape.
- **Superseded by:** n/a — the newest doc in this cluster.
- **Still open:** almost everything.
  - §3.5 per-fn size accounting / `AZ_LIFT_REPORT` — **does not exist** (grep: 0 hits).
    The plan says all L1/L2 work must be sequenced by this report, so this is the
    blocking first step.
  - §3.4 deny-list guard (`turso_|printpdf|rustls|regex_|lopdf|ureq` must be unreachable)
    — no such check in `dll/src/web/` (only unrelated `denylist` comments at
    `symbol_table.rs:19,2499`).
  - §3.1–§3.3 L1 cuts — no `JsImport` or `HostCall` variant exists in `FnClass`
    (`symbol_table.rs:108-208`: Recursable, BoundaryImport, BumpAlloc/Realloc/Dealloc,
    CallIndirect(Layout4), ResolveCallback, HashmapRandomKeys, Leaf, LibcMemcpy/Memset/
    Snprintf, NeverLift). Hyphenation `embed_all` still on at `layout/Cargo.toml:48`.
  - §4 L2 per-app reachability — not started.
  - §5 L3 — the shard machinery exists but is **off by default**:
    `symbol_table.rs:84-87` `shards_enabled()` requires `AZ_ENABLE_SHARDS` and the
    polarity flip is still pending (`:76-83`, `:2477-2483`).
- **Research value:** MEDIUM–HIGH. The **binary-size dieting taxonomy** is transferable:
  L0 pipeline waste → L1 substitute what the host already provides → L2 per-app
  reachability → L3 layered lazy bundles, with the framing "getting to sub-MB requires
  cutting *what* is lifted, not squeezing *how* it is lifted." §5's argument that
  **bundle granularity beats per-fn** (per-fn shards lose wasm-ld `--gc-sections`/LTO and
  repeat State-struct glue; group by call-graph community instead) is a real, non-obvious
  finding. §7's rejection of rustc `-Oz` on the *native* dylib (net regression — new
  unlifted jump tables; lift fixes are tuned to opt-3 codegen) is a hard-won negative
  result. If this doc is ever retired, §5 + §7 must survive.

---

#### scripts/WASM_SHIPPING_OPTIONS.md

- **Verdict:** RESEARCH — the WASM shipping-strategy tradeoff; decision made, rationale still useful.
- **Was:** 6 KB, 2026-05-18, "record-only" design-options note. After the per-page data
  mirror (`732960155`) brought mini.wasm from 27 MiB to 13.5 KiB for hello-world, it asks
  how to share lifted libazul functions across callbacks without duplicating them per-cb.
  Four options: **A** per-fn shared wasm (`/az/api/AzDom_addChild.wasm`), **B** one shared
  `libazul-runtime.wasm`, **C** status quo (per-cb transitive lift), **D** hybrid keyed on
  fan-in. Recommends deferring until a multi-cb workload can be measured, and lists four
  things to keep regardless.
- **Landed:** The decision was **A**, as "M10-D per-fn WASM sharding":
  `dll/src/web/mod.rs:40-52` (`shards_enabled()`), `symbol_table.rs:76-87` +
  `:113-123`, `:226`, `:2470-2496` (`FnClass::BoundaryImport`, `env.sub_<hex>` boundary
  imports), `transpiler_remill.rs` `BoundaryShard`. `WEB_WASM_DIET_PLAN:284` records the
  M10-E measurement: shared-runtime variant at **13.4 kB** for hello-world.
  All four "keep regardless" items shipped: `Pcs::HiddenPtrReturn`
  (`eventloop.rs:387,633`, `symbol_table.rs:147`), synthetic-address lift
  (`symbol_table.rs:266,672`), the per-page mirror, the post-link stack relocator.
- **Superseded by:** `WEB_WASM_DIET_PLAN_2026_07_04.md` §5, which *revises* the verdict —
  it argues for **bundles (boot/layout/cb/rare), not per-fn**, because per-fn shards lose
  cross-fn LTO. So option A won the mechanism and option D-ish bundling is the current
  direction.
- **Still open:** shards are still opt-in (`AZ_ENABLE_SHARDS`); the polarity flip to
  default-sharded is pending; the multi-cb duplication measurement the doc asks for
  (§Recommendation step 1) was never run.
- **Research value:** MEDIUM. Clean, short, honest statement of the **WASM shipping
  strategy** tradeoff: per-fn dedup + lazy loading vs HTTP waterfall + per-module overhead
  vs one big shared runtime blocking first paint. Directly comparable to how
  Blazor WASM ships one big runtime and how emscripten's `-sMAIN_MODULE/SIDE_MODULE`
  splits. Good `scripts/research/` candidate — merge with `WEB_WASM_DIET_PLAN` §5 so the
  revised verdict (bundles > per-fn) sits next to the original options.

---

#### scripts/M9_WASM_DOM_HANDOFF.md

- **Verdict:** DELETE — self-marked SUPERSEDED at line 1; all six phases executed.
- **Was:** The M9 session handoff: move the DOM into WASM and kill the JS-side hit-test.
  Six phases (~1,200 LOC budget): (1) layout-cb wrapper signature with a destination
  buffer, (2) `AzStartup_buildLayoutInfo` + JS instantiate, (3) `EventloopState` embeds
  `LayoutWindow` + `AzStartup_initLayoutCache`, (4) wasm-side `AzStartup_hitTest`,
  (5) diff + TLV patch emission, (6) `loader.js` minimization. Includes the AAPCS64 X8
  indirect-result-register explanation and committed architectural decisions.
- **Landed:** All six. Verified exports in `dll/src/web/eventloop.rs`:
  `AzStartup_buildLayoutInfo`, `AzStartup_initLayoutCache`, `AzStartup_hitTest`,
  `AzStartup_setLayoutCbTableIdx`, `AzStartup_setRefAny`, `AzStartup_getCurrentDomPtr`,
  `AzStartup_buildPatch`, `AzStartup_dispatchEvent` (rewritten, `:2361`).
  `PATCH_KIND_*` 1–12 at `:2077-2088`. M9 markers appear 54× in `dll/src/web/`.
- **Superseded by:** `scripts/M9_REVIEW_AND_OPTION_A.md` (explicitly, in its own banner) —
  the review judged the plan over-architected and replaced most of the scaffolding with a
  1-parameter synthetic-address fix.
- **Still open:** none. The doc's own leftovers (wasm memory inflation to 1 GiB, the
  data-mirror filter heuristic) were declared dead ends and reverted in `b1470628a`.
- **Research value:** none uniquely — the AAPCS64/X8 vs "wasm functions return one scalar"
  explanation is genuinely good pedagogy, but it is already carried in the code
  (`eventloop.rs:387,633`, `symbol_table.rs:147`) and in
  `doc/guide/en/internals/web.md`.

---

#### scripts/M9_REVIEW_AND_OPTION_A.md

- **Verdict:** DELETE — the synthetic-address scheme landed verbatim; rationale now lives in code.
- **Was:** 2026-05-18 post-M9 retrospective. §1 names the actual root cause in one
  paragraph: remill lifts ARM64 `adrp x<n>, IMM` using `%program_counter`, so with
  `lift_addr = post-ASLR runtime address` the computed page lands ~192 MiB into a 16 MiB
  wasm linear memory → trap. §2 audits every M9 piece as keep/delete. §3 specifies the
  synthetic-address scheme (per-image bands, PC-relative distance preservation, the three
  `lift_addr` call sites, the cross-function BL algebra proof). §§4–5 reject
  separate-per-module memory and static slot reservation. §§8–9 record supersession.
- **Landed:** Fully. `SymbolEntry::synthetic_addr` at
  `dll/src/web/symbol_table.rs:266`; `assign_synthetic_addresses()` at `:672` (called from
  `:644`), with the per-image `synth_base` at `:351`; `native_to_synth()` at `:900`;
  synth-keyed lookup at `:872-879`; `enumerate_low32_data_for_wasm` at `:453`. The
  `html_render.rs` `type_id` translation (implementation note 2) is present. The scheme is
  documented in `doc/guide/en/internals/web.md:89` "Synthetic-address lift" and
  `:108` "Per-image rebasing".
- **Superseded by:** n/a — this one *won*. Its own §7 leftovers were later absorbed:
  the 1 GiB memory experiments are gone; the memory map has since moved (heap base now
  160 MiB per the SUPERPLAN post-rebase fix).
- **Still open:** none from this doc. One descendant survives: the "memory-map audit"
  class the SUPERPLAN flags — any dylib growth past the 160 MiB bump base re-collides with
  the synth band. That belongs with the SUPERPLAN, not here.
- **Research value:** LOW-MEDIUM. The insight — *when lifting native code to a 32-bit
  sandbox, rebase every image into a small synthetic address space before lifting rather
  than growing the sandbox to absorb runtime addresses* — is transferable, and §3.5's
  algebraic proof that intra-image BL targets resolve for free is elegant. But
  `symbol_table.rs:251-266` already carries the same explanation in doc comments, and
  `web.md:89-128` carries the narrative. Not worth a second copy.

---

#### scripts/M8_7_HYDRATION_PLAN_2026_05_16.md

- **Verdict:** RESEARCH — the state-hydration model (why JSON, not a memory dump) is not written down elsewhere.
- **Was:** 2026-05-16 spec for M8.7, written from a verbatim user directive. Defines a
  `HeadlessApp` wrapper for the web backend (app_data, config, font_cache, window_state,
  current_dom, layout_cb), server-side startup validation that the root `RefAny` is
  JSON-round-trippable, the hydration payload shape, wasm-side hydration, and the
  dispatch flow. Four addenda record successive user redirections (postcard envelope,
  upstream serde derives behind a feature flag, WASM-side layout recomputation).
- **Landed:** `HeadlessApp` is real at `dll/src/web/headless.rs:25-45` with exactly the
  planned fields, plus `ValidationError::RefAnyNotSerializable` (`:47-55`). The payload
  is `dll/src/web/hydration.rs` — whose module doc at **lines 4-5 cites this file by
  path**: "Per `scripts/M8_7_HYDRATION_PLAN_2026_05_16.md` addendum 2 (user direction)".
  Shipped shape: single postcard envelope, base64 in
  `<script id="az-state" type="application/octet-stream">`, narrow *projection* wrapper
  types rather than serde derives on upstream structs. `AzStartup_hydrate` at
  `eventloop.rs:530` and `AzStartup_hydrateStyledDom` at `:1084`. Server-side validation
  documented at `doc/guide/en/internals/web.md:374`.
- **Superseded by:** partially — `doc/guide/en/internals/web.md` (§`AzStartup_hydrate`
  :827, 36 hydration mentions) is now the maintained description of the *mechanism*.
- **Still open:** the "lift the user's own `_fromJson`" alternative is still gated
  (banner at line 1): `AzStartup_hydrate` remains **hand-rolled**, i.e. hack #11/#18 from
  the M8.8-era hacks review is unretired. The doc's "pre-compile every api.json function
  at startup" architecture is explicitly recorded as *still future work* in
  `web.md:403-406` — `classify.rs` produces 2,532 classifications that nothing consumes.
- **Research value:** MEDIUM. The **DOM/state hydration model** argument is the durable
  part and is **absent from `web.md`** (checked): you cannot memcpy the server's `RefAny`
  to the client, because a `Vec` inside it holds pointers into *server* address space, so
  state must cross the boundary through a serializer the user registers
  (`AZ_REFLECT_JSON`) and be *rebuilt* client-side — validated at server startup so a
  misconfigured app fails fast instead of rendering a broken page. That generalizes to any
  server→client continuation of a live native process. Extract §"Why JSON, not raw bytes"
  + the validation-fails-fast rule into `scripts/research/`, or fold it into `web.md:374`,
  before retiring the rest. Also: **if the file is deleted, `dll/src/web/hydration.rs:4-5`
  must be updated** — it points at this path.

---

#### scripts/M8_ARCHITECTURE_2026_05_19.md

- **Verdict:** DELETE — M8.0–M8.10 all executed; live doc is `doc/guide/en/internals/web.md`.
- **Was:** The M8 milestone architecture (2026-05-19): "single tab = single window", the
  server renders once then reduces to an asset server, and everything after first
  interaction is client-side. Specifies the four served asset kinds (`/<route>` HTML,
  `/az/mini.<hash>.wasm`, `/az/layout/<hash>.wasm`, `/az/cb/<sym>.<hash>.wasm`), the full
  `AzStartup_*` extern-C surface, `listener.js`, server-side discovery/lifecycle, and a
  phased plan M8.0–M8.10 (~30–50 h). Ends with "what a new agent should do first" and
  build/run commands.
- **Landed:** Everything. The asset URL scheme is live (`server.rs`, `loader_js.rs:266-476`
  fetches `/az/manifest.json`, `/az/fallback.ttf`, per-cb and layout wasm). The
  `AzStartup_*` surface shipped and then grew well past this spec — 37 exports today vs
  the ~12 sketched here. `EVENTLOOP_SYMBOLS` is `dll/src/web/mod.rs:60+`. Code markers
  M8.2/M8.3/M8.4/M8.5/M8.6/M8.7/M8.9 are all present in `dll/src/web/`. The canonical
  description is now `doc/guide/en/internals/web.md` (1,170 lines, §"Three-phase
  architecture" :325, Phase A :359, Phase B :924, Phase C :963).
- **Superseded by:** `doc/guide/en/internals/web.md` for the architecture;
  `M8.8_NEW_SESSION_PROMPT` → `M9_*` → `WEB_1TO1_*` for the plan.
- **Still open:** exactly one line, and it is the origin of a live problem — M8.10's
  "**Wasm size budget (azul-mini target <500KB)**" at `M8_ARCHITECTURE_2026_05_19.md:709`.
  That budget is cited as the baseline by `WEB_WASM_DIET_PLAN_2026_07_04.md:18`, and the
  mini is currently ~25 MB. **Copy that budget line into the diet plan before deleting
  this file** (the diet plan cites it by path:line).
- **Research value:** none beyond the budget number — the "single tab = single window,
  server renders once then becomes an asset server" model is restated better in
  `WEB_BACKEND_1TO1_PLAN` §6 and in `web.md`.

---

#### scripts/M8.8_NEW_SESSION_PROMPT.md

- **Verdict:** DELETE — self-marked "M8.8 closed 2026-05-16"; SymbolTable shipped.
- **Was:** A session prompt reframing 14 sequential bug fixes as **one** architectural
  bug: "the lift pipeline has no canonical source of truth for symbol identity — five
  subsystems each compute address→name independently, reconciled only at wasm-ld link
  time, after `opt -O2` has already constant-folded across the inconsistencies." Four
  stages: (1) build the SymbolTable, (2) a cheap layout-cb-executes probe, (3) dispatch +
  hit-test + diff/patch as the second subsystem, (4) mechanical cleanup. Explicit
  instruction not to add a 15th workaround.
- **Landed:** `dll/src/web/symbol_table.rs` is 3,371 lines and is the pipeline's identity
  authority (`SymbolEntry`, `SymKind`, `FnClass`, `assign_synthetic_addresses`,
  `native_to_synth`, per-image bands, TLV/TLS geometry). Shipped in `b26d04b42`; M8.8
  markers appear 27× in `dll/src/web/`. Stage 3 landed as M9. `web.md:129` documents the
  "Symbol-name flow through the pipeline".
- **Superseded by:** `M9_REVIEW_AND_OPTION_A.md` (per its own banner) for what came next.
- **Still open:** none from the doc's stages. Its *thesis* remains live guidance and is
  now enforced structurally by the module's existence.
- **Research value:** LOW-MEDIUM as content, but this is the cluster's cleanest instance
  of a recurring methodology already in memory (`azul-gates-with-wrong-premises`): **when
  N fixes in a row are all at the byte/name-string level, the abstraction below them is
  missing — stop fixing symptoms and build the authority.** One paragraph, worth folding
  into a methodology note rather than kept as a 24 KB session prompt.

---

### Tally

| Verdict | Files |
|---|---|
| ACTIVE (2) | `WEB_1TO1_SUPERPLAN.md` (truncate the log half), `WEB_WASM_DIET_PLAN_2026_07_04.md` |
| RESEARCH (3) | `WEB_BACKEND_1TO1_PLAN.md`, `WASM_SHIPPING_OPTIONS.md`, `M8_7_HYDRATION_PLAN_2026_05_16.md` |
| DELETE (5) | `WEB_BACKEND_PLAN_2026_05_18.md`, `M9_WASM_DOM_HANDOFF.md`, `M9_REVIEW_AND_OPTION_A.md`, `M8_ARCHITECTURE_2026_05_19.md`, `M8.8_NEW_SESSION_PROMPT.md` |
| ARCHIVE (0) | — |

### Deletion pre-conditions (do these first)

1. `dll/src/web/hydration.rs:4-5` cites `scripts/M8_7_HYDRATION_PLAN_2026_05_16.md` by path
   — that file is a RESEARCH keeper, so no action needed *unless* it moves to
   `scripts/research/`, in which case update the comment.
2. `WEB_WASM_DIET_PLAN_2026_07_04.md:18` cites `M8_ARCHITECTURE_2026_05_19.md:709` (the
   <500 KB budget) and `:15,:71` cite `WEB_1TO1_SUPERPLAN.md` / `WEB_BACKEND_PLAN_2026_05_18.md`
   line numbers. Inline those three facts into the diet plan before deleting the sources.
3. `WEB_1TO1_SUPERPLAN.md:11` points readers at `scripts/SESSION_PROMPT_web_1to1.md` and
   `WEB_BACKEND_1TO1_PLAN.md` — both stay, so the chain holds.


## Part 12 — web-lift handoffs & session logs (12 files)

### Effort status (established before classifying)

The "web lift" is **dormant, not abandoned**, and it is *not* the same thing as azul's ordinary
wasm32 support (that is separately alive: `scripts/check_wasm_instant.sh`, `8673855fe fix(core):
wasm gets a real clock`).

Evidence the lift pipeline is still load-bearing:

- `third_party/remill` submodule is pinned at **`1d5dd7f`** — exactly the SSE-ISEL commit the
  2026-06-25 handoff said "still needs bumping". It was bumped.
- `dll/src/web/` is live production code: `transpiler_remill.rs` (463 KB), `symbol_table.rs`
  (153 KB), `loader_js.rs`, `eventloop.rs` (114 KB), touched as late as **2026-07-29**.
- `scripts/build_remill.sh` is the *documented bootstrap* for the `web-transpiler-static`
  feature — referenced by `dll/build.rs:265` (error message), `dll/Cargo.toml:757`,
  `doc/src/dllgen/build.rs:97`, `doc/guide/en/internals/web.md`.
- CI publishes a pre-lifted base image: `.github/workflows/dockery.yml`, `docker-base.yml`
  (`8b…/perf(docker): cache remill independently of AZUL_REF`, 2026-07-08).
- Lift-motivated workarounds survive in shared source: `core/src/dom.rs:1069`,
  `core/src/styled_dom.rs:2242`, `core/src/compact.rs:711`, `core/src/prop_cache.rs:1073`,
  `layout/src/solver3/fc.rs:394-416`, and the `web_lift` cargo features
  (`core/Cargo.toml:48`, `layout/Cargo.toml:174`).

Split by architecture:

- **aarch64/macOS lift — DONE and validated.** hello-world renders + clicks end-to-end; the
  out-param/BTreeMap/cache-bypass workarounds were all deleted in **`b5e6a7e55` (2026-06-10)**
  ("delete obsolete web-lift workarounds"). `dockery.yml:70` calls arm64 "the validated path".
- **x86-64/Windows lift — STILL BLOCKED.** `dll/src/web/loader_js.rs:520` still reads
  `if (false && initRc === 0 && …)` — the hydrate gate is disabled, exactly as the 2026-06-24/25
  handoffs left it. `dockery.yml:74` says "the x86 self-lift is still WIP, so this [amd64 image]
  is best-effort".
- **Last substantive commit: 2026-06-25** (`9328e9af0` WIP backup checkpoint). Five weeks idle;
  only Docker/packaging touches since.

So: two files are genuinely **ACTIVE**, one is a first-class **RESEARCH** keeper, one more is a
tool README worth keeping, and the rest are historical.

---

#### scripts/WEB_LIFT_BUG_COMPENDIUM.md

- **Verdict:** RESEARCH — the single durable artifact of the entire effort; a portable failure-mode catalogue.
- **Was:** Written 2026-06-12, after the aarch64 lift reached full hello-world render→click parity, explicitly to make the x86/Windows port cheap. Catalogues every bug chased on the aarch64 lift and tags each with a portability verdict: `[INHERIT]` (target-agnostic, lives in `dll/src/web`), `[ISA]` (decoder/semantics gap, re-solve with the same method), `[ABI]` (AAPCS64-specific, must be re-derived for System V *and* Windows x64 separately), `[OPEN]`. 25 numbered bugs across four categories plus a §"METHODS that actually cracked these" section.
- **Landed:** Verdicts are checkable and check out. A1 classifier Leaf-stub → `cb017d266` + `dll/src/web/symbol_table.rs:2670-2676`. A6 alias-scope strip → `dd055272e`. B1-x86 CVTSI2SS/jump-table → the fork's x86 devirt (`1d5dd7f` lineage). C1 X8/sret → the fork branch name itself, `m12-q-reg-x8-sret` (`.gitmodules`). D1 (the one `[OPEN]` entry) → `layout/src/solver3/fc.rs:394-416` `force_ifc` bypass, **still present today**.
- **Superseded by:** n/a — it is the supersession target for the four chronological logs below.
- **Still open:** D1 (FC-assignment mis-lift; the `force_ifc` bypass in `fc.rs`). Two genuinely durable findings from sibling docs are **missing** from it and should be folded in before archiving anything: (a) the **repr(Rust) niche-discriminant mis-read → `#[repr(C, u8)]`** class (from the 06-03 log; live in `core/src/dom.rs:1069`), and (b) the **SwissTable/hashbrown correctness hang** that closed out the x86 effort (2026-06-25).
- **Research value:** High and genuinely transferable beyond azul. Concepts: (1) the INHERIT/ISA/ABI partition of binary-lifting bugs — which fixes port across ISAs and which do not; (2) `noalias`/`alias.scope` metadata that is sound on real hardware becomes *unsound* on wasm's single linear memory (A6); (3) libc/allocator/TLS primitives are out-of-image and must be classified, not lifted (A2/A3/A4/A8, incl. the Mach-O `tlv_get_addr` vs ELF `%fs` vs Windows `_tls_index`/`%gs:0x58` divergence); (4) §M3 "a no-op stub looks identical to a mis-lift at the call site — grep `class=` before debugging any garbage value"; (5) §M7 "hang and garbage-value are the same bug class"; (6) §M8 "when a probe changes the result, stop probing"; (7) §M9 "isolated single-instruction lifts prove nothing about the production pipeline". **Recommend `scripts/research/binary-lifting-failure-modes.md`.**

#### scripts/HANDOFF_FABLE_web_lift_x86_windows_2026_06_13.md  (104 KB)

- **Verdict:** ACTIVE — its blocker is still live in the tree; carries the only Windows build recipe.
- **Was:** The x86-64/Windows port's running log, opened 2026-06-13 and appended through 2026-06-24 (12 dated sections + 9 "tick" entries). Starts from "user code runs lifted, counter 5→6, full layout OOBs", passes a full end-to-end milestone on 06-21, then chases B1-SSE (three successive root causes, two of them wrong), the browser OOB, and the `hydrateStyledDom` trap.
- **Landed:** `e55af776d` (x86/Windows port), `4080a12f7`, `2ba1b59de` (REC_MARKER pc-pollution fix, described verbatim in the comment at `dll/src/web/loader_js.rs:503-511`), the x86 jump-table devirt + TraceLifter convergence guard in the fork. The `false &&` hydrate gate at `loader_js.rs:520` is the doc's own hand-off state, still unchanged.
- **Superseded by:** partially — `scripts/HANDOFF_web_lift_x86_SSE_swisstable_2026_06_25.md` continues the same thread and supersedes its §"NEXT = solver func232" with a sharper root cause. Read the 06-25 file first; this one is the substrate.
- **Still open:** the whole x86 path. Hydrate disabled (`loader_js.rs:520`); the solver trap chain 69→68→116→232 into font-cache resolution (`eventloop.rs:1642` region); the two documented remill-fork bugs in §4 that were never committed to the fork. The MSVC build recipe (§2) exists nowhere else — `/INCREMENTAL:NO` requirement, PDB CWD staging, `RUSTC_BOOTSTRAP=1 -Z build-std` line, `ninja … amd64.bc` gotcha.
- **Research value:** Moderate. The self-correcting root-cause sequence for B1-SSE is a good worked example of §M8/§M9 (three published root causes; the first two — cvtsi2ss decode, then "stateful full-function lift" — were both wrong; the real one was "remill's devirt is AArch64-only, x86 falls through to a sweep"). The REC_MARKER story is a real transferable pitfall: **rewriting a self-recursive call target to a sentinel poisons any PC-relative data addressing that uses the same `%pc`** — 256 MB of drift per recursion level on x86.

#### scripts/HANDOFF_web_lift_x86_SSE_swisstable_2026_06_25.md

- **Verdict:** ACTIVE — the newest state of the only unfinished branch; 11 KB, fully current.
- **Was:** The last web-lift session (2026-06-25). Opens by **disproving** the previous stated root cause ("AzButton_dom garbage children / dropped movups — do not re-chase"), then establishes that the real deep bug was incomplete remill x86 SSE coverage: Rust's auto-vectorizer and explicit SSE4.1 intrinsics in the layout solver emit packed/double instructions XED decoded but that had no `DEF_SEM`/`DEF_ISEL` → `HandleUnsupported` → `__remill_error`. Explains why HashMaps trapped while BTreeMaps worked (SwissTable is auto-vectorized).
- **Landed:** 17 SSE ISELs committed to the fork; **verified** — `git submodule status third_party/remill` returns `1d5dd7fa4cec…`, the exact commit the doc says still needed bumping. Lift coverage claim: 0/3031 functions unsupported.
- **Superseded by:** n/a — nothing came after it.
- **Still open:** Everything the doc calls the remaining blocker. (1) The **SwissTable correctness mis-lift**: the layout solve hangs (`[2c] hydrateStyledDom rc=0` then never `[2d]`); a HashMap's table pointer lifts to garbage so the hashbrown probe loop never terminates. Coverage and `__remill_*` stubs are both ruled out — this is a mis-lift of a *supported* instruction. (2) `dll/src/web/loader_js.rs` **still lacks** `__remill_read_memory_32`, `__remill_compare_exchange_memory_64/8`, `__remill_atomic_begin/end` — I grepped; zero hits. The doc flags this as a latent break for when hydrate is re-enabled, and it is still true. (3) The hydrate gate (`loader_js.rs:520`). (4) The listed uncommitted diagnostic state (vendored `third_party/rust-fontconfig` with `chain_cache.insert` disabled) — note the Cargo patch was since removed (`Cargo.toml:59-65` documents the revert), so that item is stale.
- **Research value:** Two transferable items. (a) **Auto-vectorization is a lifting hazard**: `HashMap` vs `BTreeMap` behaving differently under a lifter is a *coverage* signal, not a data-structure bug — SIMD-accelerated containers (SwissTable, memchr, SIMD JSON) hit decoder holes that scalar code never reaches. (b) The **XED iform gotcha**: `roundps xmm,xmm,imm` decodes as `_XMMps_`, not `_XMMdq_`; a semantics author who registers only the "obvious" iform gets a silently-still-unsupported instruction. Plus the fast-verification loop: lift one instruction with `remill-lift-17 -bytes … -ir_out` and `grep -cE 'i32 (noundef )?257\)'` (257 = the `HandleUnsupported` hypercall) instead of a 27-minute relift.

#### scripts/mechb_harness/README.md

- **Verdict:** RESEARCH — a reusable technique with a runnable recipe, and it sits next to its own artifacts.
- **Was:** 2.1 KB README for the native-aarch64 executor built 2026-06-12 to root-cause "mechanism B" (`<[&str]>::join` returning a `String` whose `len` held a heap pointer). Documents what it *proved*: the lift of `join_generic_copy` was always correct; the real cause was `classify_for_name` defaulting crate `alloc` to `FnClass::Leaf`, so `join` was never lifted and the caller read 24 bytes of stale stack.
- **Landed:** `cb017d266`; the fix is cited back at `dll/src/web/symbol_table.rs:2670-2676`, which **names this directory by path** — deleting it would orphan a source comment. Sibling artifacts are all present: `harness.cpp` (9.4 KB), `join_bytes.hex`, `join_h9c9d.disasm`, `probe.ll`.
- **Superseded by:** n/a — it is §M2 of the compendium, expanded.
- **Still open:** none as a task. The compendium recommends re-pointing it at x86 for the D1 hunt; the 06-13 log records a later attempt (`btn_harness.cpp`) that hit an impasse because the harness couldn't reproduce real-input-dependent corruption — worth noting as a limit of the method.
- **Research value:** The core technique: **compile the lifted IR back to the *host* ISA and execute it under a C harness that implements the `__remill_*` memory shims over a flat buffer**, converting an uninstrumentable wasm/std bug into a normal native binary you can step in lldb, with a symbolic memory-op trace (SLICE/P/SEP/RET/STK/HEAP). This is what static IR reading and isolated-instruction lifts structurally cannot give you. Keep in place (do **not** move to `scripts/research/` — the source comment points here).

#### scripts/HANDOFF_FABLE_web_lift_2026_06_10.md

- **Verdict:** ARCHIVE — its one headline task was completed the same day it was written.
- **Was:** Handoff from Opus 4.8 to Claude Fable, 2026-06-10, `web-lift-text-layout` branch. Establishes the two-mis-lift-class mental model (Class A: loop-bound `Vec::len()` SROA → 0, **fixed** by universal volatile guest loads + NEON decoders; Class B: multi-word Vec/struct return via X8/sret, **open**) and catalogues the `#[cfg(feature = "web_lift")]` workarounds to delete once Class B is fixed.
- **Landed:** `b5e6a7e55` (2026-06-10) deleted essentially the whole §4 catalogue — out-params reverted to by-value (`layout/src/font_traits.rs:55` now reads `-> Result<Vec<Glyph>, LayoutError>`), the g115/g118/g120 HashMap-cache bypasses restored, the lift-motivated BTreeMap migrations reverted to `std::HashMap`, and markers gated behind `az_mark` (`layout/src/lib.rs:116-153`). 12 files, −407 lines.
- **Superseded by:** `scripts/WEB_LIFT_BUG_COMPENDIUM.md` (the Class A/B distinction is C1/C3 there, generalized); the deletion commit itself.
- **Still open:** one survivor from §4.D — `force_ifc` at `layout/src/solver3/fc.rs:394-416`, which is compendium D1. Everything else in the catalogue is gone.
- **Research value:** none beyond the compendium. Its two "traps for your successor" (the `initializes` attribute is an LLVM-20/21 artifact absent from the LLVM-17 pipeline; rust-analyzer shows stale arity errors, trust `cargo check`) are tool-version-specific and already expiring.

#### scripts/HANDOFF_web_flexbox_lift_2026_06_01.md

- **Verdict:** ARCHIVE — earliest log, fixes landed, and its central claim is retracted in its own body.
- **Was:** 2026-06-01. First proof that the lifted solver could produce box positions matching native. Six numbered "REAL FIXES", then a headline blaming "remill mis-lifts large `match`→jump-table dispatch" — followed immediately by a `⚠️ CORRECTION` section (same day, from read-only disasm) showing `get_property_slow` has *no* jump table and no indirect branch, and that the real cost surface was 627 transitively-lifted `allsorts_azul` font-parsing functions.
- **Landed:** All six. Niche-read false-`Err` (`layout/src/solver3/positioning.rs` return types). `CssProperty::clone` discriminant-zeroing → `core::mem::take` at `core/src/styled_dom.rs:2242` (comment still there). `apply_css_property_to_compact` if-let dispatch at `core/src/compact.rs:711`. `Layout::from_size_align_unchecked`. `FnClass::HashmapRandomKeys`. The allsorts `FnClass::Leaf` web font boundary.
- **Superseded by:** `scripts/WEB_LIFT_BUG_COMPENDIUM.md` (A3, A9, B5) and every later handoff.
- **Still open:** one stale TODO — "`doc/fonts/SourceSerifPro-Regular.ttf` … TODO: re-track in git". PART 2 (the `RequestResources` TLV, kind 13, for browser-side font/image supply) was never started; that plan lives in `WEB_BACKEND_1TO1_PLAN.md` territory, not here.
- **Research value:** Low, but one lesson generalizes and is *not* in the compendium: **cheap read-only disassembly refuted a multi-session hypothesis in hours** — the correction section is a model of it. The niche-discriminant finding here is the germ of the `#[repr(C, u8)]` rule (below).

#### scripts/HANDOFF_web_rwlock_glyphdecode_2026_06_03.md  (99 KB)

- **Verdict:** ARCHIVE — chronological log; but extract the `#[repr(C, u8)]` rule before archiving.
- **Was:** Reverse-chronological session log, 2026-06-03 → 2026-06-06, entries g50–g128. Covers the RwLock spin fix, font resolution coming up, the FMUL-by-element NEON decoder, and the shape_text out-param. Its centrepiece (g117–g121) is the discovery that **the web lift mis-reads any `repr(Rust)` niche discriminant** — enum-niche or `Result`/`Option`-niche — so a raw load at offset 0 works but the `match`/derived-`Clone`/`?` logic mis-routes; the fix is `#[repr(C, u8)]` to force an explicit u8 tag at offset 0.
- **Landed:** The `repr(C, u8)` guards survive: `core/src/dom.rs:1069-1075` carries the rationale verbatim ("remill mis-decodes that niche encoding"). The FMUL-by-element decoder landed in the fork (`6aabb45`). The out-param chain landed and was later deleted (`b5e6a7e55`).
- **Superseded by:** `scripts/HANDOFF_web_helloworld_NEXT_2026_06_06.md` (direct successor log) and the compendium.
- **Still open:** none actionable — every named blocker was either fixed or reclassified.
- **Research value:** One item, and it is a real gap in the compendium: **Rust's niche-optimized enum layout is a first-class binary-lifting hazard**, distinct from the ABI and decoder classes. The compendium has no entry for it. Also durable: the g67/g68 lesson at the tail — *diagnostic markers written to fixed absolute addresses landed inside the lifted code's own wasm stack* (`0x40000-0x40800` vs stack `[0x30000..0x50000]`), producing a textbook heisenbug; markers must live in a reserved band (`[0x50000..0x110000]`). That is the concrete instance behind compendium §M8. Fold both into the compendium; then archive.

#### scripts/HANDOFF_web_helloworld_NEXT_2026_06_06.md  (233 KB, 2593 lines)

- **Verdict:** ARCHIVE — the largest log in the set; distilled and, in places, self-refuted.
- **Was:** The blow-by-blow investigation trail g113→g223 across 2026-06-06→06-09, plus a 06-08 "DEEP-FIX PLAN". Chases the "Vec-return `len` mis-lift" through the hashbrown `EMPTY_GROUP` mirror hunt (g196–g213), the execution-differ build (`AZ_REG_TRACE`, g205–g211), and ends at g219–g223 sharply separating Class A from Class B.
- **Landed:** g213 (`static_empty` is `[0xFF; 8]`, the scan's `>= 16` width filter missed it → fix `>= 8`) shipped as `c0861ee07`, the EMPTY_GROUP mirror. Volatile guest loads shipped (`transpiler_remill.rs`). The tooling built here — `AZ_FUEL`, `AZ_READ_TRACE`, `AZ_REG_TRACE`, `AZ_WRITE_TRACE`, `AZ_REMILL_KEEP_SCRATCH`, `AZ_LOWOPT_FNS` — is still in `transpiler_remill.rs`.
- **Superseded by:** `scripts/HANDOFF_FABLE_web_lift_2026_06_10.md` (its clean summary) → `scripts/WEB_LIFT_BUG_COMPENDIUM.md`.
- **Still open:** none. g223 explicitly retires its own premise: with universal volatile guest loads, "SROA'd `Vec::len()` in a loop bound reads 0" is *structurally impossible* (403/403 guest loads volatile in the checked function), so the deep-fix plan that occupies a third of the document is obsolete.
- **Research value:** Low per byte, but it is the best surviving record of **how many published root causes were wrong**: g145 debunked as a marker misread; g155 "fix implemented" that did not fix it; g199 volatile-memset that did not fix it; g210 disproven by g211; g220's "deletable" reversed by g221. That pattern is already generalized as compendium §M8/§M9 — no need to keep 233 KB to make the point.

#### scripts/HANDOFF_web_vec_return_len_mislift_2026_06_06.md  (54 KB)

- **Verdict:** ARCHIVE — and its "DEFINITIVE CONCLUSION" is now known to be wrong; do not use as guidance.
- **Was:** 2026-06-06 root-cause hunt for the recurring "function returning a `Vec`-containing value by sret reads a pointer-shaped `len`" bug, with six confirmed instances and six per-function out-param workarounds. Sections g129→g139 record four successive *source-level* workaround forms (slice iter, range, adjacent collect, volatile-read + `get_unchecked`) all failing identically.
- **Landed:** The out-param workarounds landed and were then all removed by `b5e6a7e55`. `NeverLift(resolve_intrinsic_track_sizes)` and the `enforce_sp __az_indirect_dispatch` wrap survive in the transpiler.
- **Superseded by:** g223 in `HANDOFF_web_helloworld_NEXT_2026_06_06.md`, and definitively by compendium **A1** — the true cause of the pointer-shaped garbage was the classifier defaulting `alloc`/`core` to `FnClass::Leaf`, so the producing function *was never lifted* and the caller read 24 bytes of stale stack. Not a field-offset shift in the sret store, which is what this document asserts throughout.
- **Still open:** none.
- **Research value:** Negative-to-neutral as a conclusion, but genuinely instructive as a case study, and this is the cleanest write-up of it: **a garbage value with pointer-shaped bits reads exactly like an ABI/field-offset bug and exactly like a never-executed callee.** Fifteen relifts and six source rewrites went into the wrong branch of that fork. The corrective rule is compendium §M3 (grep `class=` first). If any single doc is quoted when explaining why §M3 exists, quote this one.

#### scripts/FABLE_PROMPT_web_lift_sret.md

- **Verdict:** DELETE — kickoff prompt whose session ran and whose goal was met.
- **Was:** 2.8 KB opening message for a Claude Fable session, pointing at `HANDOFF_FABLE_web_lift_2026_06_10.md`. Goal: root-cause the class-B sret mis-lift so the `#[cfg(feature = "web_lift")]` out-param workarounds could be deleted. Carries three hypotheses (H1 decoder/CFG gap, H2 LLVM-17 DCE of the X8 store, H3 `bl`→`sub_` X8 threading) and hard constraints (fix in transpiler or fork, never azul source).
- **Landed:** effort completed — `b5e6a7e55` (2026-06-10) deleted the out-param workarounds it targeted, co-authored by Claude Fable 5.
- **Superseded by:** `scripts/HANDOFF_FABLE_web_lift_2026_06_10.md`, which contains everything the prompt summarizes.
- **Still open:** none.
- **Research value:** none. Machine-specific paths (`/Users/fschutt/Development/azul-mobile`), and both "traps" it flags are already reproduced in the handoff it points at.

#### scripts/PROMPT_web_helloworld_NEXT.md

- **Verdict:** DELETE — kickoff prompt built on a premise later proven false.
- **Was:** 3.7 KB task prompt for the hello-world lift, instructing the next agent to treat "a SYSTEMIC remill lift-fidelity failure in OPTIMIZED Rust code — SROA'd `Vec::len()` reads 0, sret aggregate returns mis-lift, `for`-loops over ranges iterate 0 times" as established and not re-derive it. Step 1 is a `opt-level = 1` per-package experiment.
- **Landed:** effort completed on a different footing — hello-world renders and clicks on the aarch64 lift.
- **Superseded by:** `scripts/HANDOFF_web_helloworld_NEXT_2026_06_06.md` g223 (the premise is structurally impossible under volatile guest loads) and compendium A1 (the real cause was classification).
- **Still open:** none.
- **Research value:** none — actively misleading if read without the g223 correction. Delete rather than archive.

#### scripts/SESSION_PROMPT_web_1to1.md

- **Verdict:** ARCHIVE — a pointer-doc whose substance lives in the plan it points at.
- **Was:** 4.5 KB kickoff for "make the web backend run the App 1:1 like the desktop app". States the resolved architecture model (WASM is the single source of truth for cascade+layout; the DOM is a passive target patched with *semantic* changes only, never absolute positions, never a measurement query back; three coupling points; host calls injected at the IR layer via a new `FnClass::HostCall`), then orders the work: Phase 0.2 preflight gate → Phases 1-2 real loop + diff → timers → visual layer. Flags two blocking decisions, D1 (geometry ownership) and D2 (text as real DOM text vs positioned glyph spans).
- **Landed:** Phase 0.2 shipped — `eb1011f3a` ("preflight clean-lift gate + engine-aware lift cache"); `AZ_PREFLIGHT` appears at `dll/src/web/transpiler_remill.rs` (5 sites) and `dll/src/web/mod.rs`.
- **Superseded by:** `scripts/WEB_BACKEND_1TO1_PLAN.md` and `scripts/WEB_1TO1_SUPERPLAN.md` (106 KB) — both outside this part's assignment; whoever audits those owns the live status. This file is the wrapper, not the content.
- **Still open:** Phases 1-2 are not done. `dll/src/web/eventloop.rs` still exposes `AzStartup_buildCounterPatch`, the hardcoded hello-world counter `SetText` path the prompt says to replace with the lifted `process_window_events` chokepoint. D1 and D2 appear never to have been resolved. **Route these leftovers to the `WEB_*_1TO1_*` plan docs rather than keeping this prompt alive.**
- **Research value:** One idea worth preserving *if the plan docs don't already state it*: the **host-call injection seam** — detecting `Instant::now`, HTTP, geolocation/sensor calls during the symbol scan and rewriting them to JS `env` imports at the IR layer, so user code is oblivious to running lifted in a browser. That is a clean general answer to "how does lifted native code reach platform services", and it is the same mechanism as `HashmapRandomKeys`/`fmaxf`.

---

### Companion non-md artifacts

Assessed against actual references in the tree, not just mtime.

| Artifact | Status | Basis |
|---|---|---|
| `scripts/build_remill.sh` | **KEEP — live** | Not dead weight. Referenced as the documented bootstrap by `dll/build.rs:265` (feature-gate error message), `dll/Cargo.toml:757`, `doc/src/dllgen/build.rs:97`, `doc/guide/en/internals/web.md`. `docker/README.md:100` confirms CI does *not* call it (the Dockerfile clones + builds remill itself), but the `web-transpiler-static` local path still tells users to run it. |
| `scripts/remill-patches/0001-fix-use-after-free-in-Lift.cpp.patch` | **KEEP — live, tiny** | Applied by `build_remill.sh` (its only referrer). A real use-after-free fix: `hyperCall->getName()` returns a `StringRef` into storage freed by `eraseFromParent()`. 1 KB. Dies only if `build_remill.sh` dies. |
| `scripts/mechb_harness/` | **KEEP — cross-referenced from source** | `dll/src/web/symbol_table.rs:2673` names the directory by path in the comment explaining the A1 fix. Also cited by the compendium (§M2), `WEB_1TO1_SUPERPLAN.md`, `WEB_WASM_DIET_PLAN_2026_07_04.md`. Deleting it orphans a production source comment. 68 KB total. |
| `scripts/web_relift.sh` | **DEAD on this machine; keep only with the x86 work** | Hardcodes `cd /Users/fschutt/Development/azul-mobile` and a macOS-only `remill-lift-17` path, and kills processes by `ps -axo` (BSD flags). Cannot run here. Value is as a recipe (orphan-kill → launch → poll the port, never the pid), not as an executable. |
| `scripts/web_relift_win.sh` | **Companion to the ACTIVE x86 work — keep** | The Windows sibling, referenced by `HANDOFF_FABLE_web_lift_x86_windows_2026_06_13.md` §2 as the standard relift launcher. Keep it exactly as long as that handoff stays ACTIVE. |
| `scripts/web_lift_triage.py` | **DEAD weight** | Hardcodes the same macOS `/Users/fschutt/...` lifter and `target/aarch64-apple-darwin/release/libazul.dylib`, and its entire payload is a **frozen literal list of ~22 mangled symbol names** (specific `h…` hashes) from one 2026-06 build. Those hashes will not match any current binary. Zero referrers anywhere in the tree. Delete with the aarch64 handoffs. |
| `scripts/classB_artifacts/` | **DEAD weight — empty directory** | Contains *only* a `.gitignore` that ignores `*.ll`/`*.bc` and four named files, none of which are present. Its one referrer is `scripts/WEB_1TO1_SUPERPLAN.md` ("regenerate via the CAPTURE RECIPE"). It is a placeholder for artifacts that were never committed and that the recipe says to regenerate. Delete. |

**Net:** of the seven, three are live (`build_remill.sh`, `remill-patches/`, `mechb_harness/`), one is tied to the ACTIVE x86 thread (`web_relift_win.sh`), and three are dead weight (`web_lift_triage.py`, `classB_artifacts/`, and `web_relift.sh` as an executable — its macOS paths make it unrunnable here).


## Part 13 — language bindings / FFI / codegen (12 files)

### Cluster-wide verified baseline (2026-08-01, branch master)

Ground truth used to judge every file below:

- **35 codegen backends exist today** in `doc/src/codegen/v2/`: `lang_{ada, algol68,
  cobol, cpp, crystal, csharp, d, fortran, freebasic, go, haskell, java, julia,
  kotlin, lisp, lua, nim, node, ocaml, odin, pascal, perl, php, powershell,
  python, racket, red, ruby, rust, smalltalk, swift, v, vb6, zig}` + `lang_c.rs`,
  `lang_php_ext.rs`, `lang_reexports.rs`. `scala` rides `java`.
  `doc/src/codegen/experimental/` **no longer exists**.
- **Emission is wired** for all of them in `doc/src/codegen/v2/generator.rs`
  (nim/racket/red at `generator.rs:287-305`, d/crystal/v/swift/julia at
  `generator.rs:308+`). Artifacts confirmed on disk: `target/codegen/azul.nim`,
  `azul.rkt`, `azul.reds`.
- **Tiers** live in `scripts/e2e_language_matrix.sh`: `ALL_LANGS` (35) at :90,
  `SHIPPED_LANGS` (16, gate CI) at :113, `BETA_LANGS = (python odin nim racket red
  d crystal v swift julia)` at :128, everything else ALPHA.
- **Frontpage whitelist** `FRONTPAGE_LANGUAGES` at `doc/src/docgen/mod.rs:150-174`
  (29 entries + 6 cpp dialects), with dated promotion comments citing
  `BINDINGS_REVIEW_2026_07_04.md`.
- **CI**: one `scripting` family runs 27 langs (`.github/workflows/rust.yml:2623`);
  `jiro4989/setup-nim-action@v2` at :2716, `Bogdanp/setup-racket@v1.11` at :2738,
  and an explicit "Red … `redc` is intentionally NOT installed" note at :2921-2923.
- **Host-invoker allowlist** `HOST_INVOKER_KINDS` at
  `doc/src/codegen/v2/managed_host_invoker.rs:54-84` — 20 kinds, now **including
  `ThreadCallback`**; `WriteBackCallback` is still absent.

**Inbound source references (deleting these docs breaks live comments):**

| Doc | Cited from |
|---|---|
| `BINDING_STRATEGY_PER_LANGUAGE.md` | `doc/src/codegen/v2/managed_host_invoker.rs:78`, `lang_lua/wrappers.rs` (×2), `lang_kotlin/mod.rs`, `layout/src/thread.rs` |
| `BINDINGS_REVIEW_2026_07_04.md` | `doc/src/docgen/mod.rs:154`, `lang_node/managed.rs`, `lang_ocaml/types.rs`, `scripts/e2e_language_matrix.sh:117` |
| `RED_FFI_FINDINGS.md` | `doc/src/codegen/v2/lang_red/mod.rs` (×2), `examples/red/hello-world.red`, `doc/guide/en/hello-world/red.md` (×2), `scripts/e2e_language_matrix.sh:1620` |
| `WIRING_red.md` | `doc/src/codegen/v2/lang_red/mod.rs` (×2) |
| all others | none |

---

#### scripts/BINDINGS_REVIEW_2026_07_04.md

- **Verdict:** ARCHIVE — dated 27-language audit snapshot; most findings burned down within 2 days.
- **Was:** 1726-line, 310KB per-language end-user review of all 27 bindings then
  shipping, cross-checked against `e2e_language_matrix.sh`. Organizing unit is one
  `##` section per language (ordered shipped-issues → candidate-near →
  candidate-far → blocked), each with fixed subsections: guide/install
  truthfulness, safety, idiomatic-ness, ergonomics, completeness, **Blockers to
  ship**, quick wins, Verdict. A ranking table at lines 8-36 scores all 27 by
  install friction and days-to-ship. Closing addendum (1707-1726) already marks
  the same-day C++/emitter fixes ✅. Marker counts: `Verdict` ×27, `FAIL` ×6,
  `✅` ×5 (all addendum), `TODO` ×2, and 101 `Blockers to ship` bullets which are
  the real open-findings ledger — the doc never uses `OPEN`/`UNRESOLVED`/`❌`.
- **Landed:** It is the *authority* cited by the promotion of 6 languages into
  `SHIPPED_LANGS` (`scripts/e2e_language_matrix.sh:115-118`) and
  `FRONTPAGE_LANGUAGES` (`doc/src/docgen/mod.rs:152-155`). Verified fixed since:
  rust `cargo add` install flow rewritten (`api.json:3058`); `AZ_LINK_PATH`
  honoured (`dll/build.rs:643-646`); macOS `install_name_tool -id @rpath/…`
  (`.github/workflows/rust.yml:1664-1677`); `azul.h` added to every cpp install
  block in `api.json`; node platform-library probe
  (`doc/src/codegen/v2/lang_node/mod.rs:270-296`); java/kotlin guides now use
  `ButtonOnClickCallbackInvokerCallback` (`doc/guide/en/hello-world/kotlin.md:85`,
  `java.md:95`); C guide `label_dom` consistency (`c.md:179-200`); python guide
  `with_on_click` / `Update.RefreshDom`; C# untyped-return double-free now neutered
  via reflective `__Consume` (`doc/src/codegen/v2/lang_csharp/managed.rs:398-402`);
  C++ `release()` present in cpp03/11/17/20 (cpp14 delegates).
- **Superseded by:** the tier ledger in `scripts/e2e_language_matrix.sh:100-200`,
  which is machine-checked; the doc's per-language prose is not.
- **Still open:** lua GC/finalizer never arms on C-returned objects (universal
  leak) and LuaJIT x86-64 NYI on by-value-aggregate calls — the latter is
  *enshrined* as a permanent skip at `scripts/e2e_language_matrix.sh:187, 1117-1120`;
  ocaml `dune build` fails (no `azul.opam`, no executable stanza);
  `azul-java.zip` pom expects `src/main/java` but ships flat; ruby
  `gem install ffi` `Gem::FilePermissionError` on macOS system ruby; node
  double-register in `registerCallback` (not re-verified either way); and the
  candidate-far/blocked tier (go trampoline, vb6 stdcall, algol68 no-FFI) which
  are architectural. **Migrate these ~8 bullets to issues before archiving** — and
  fix the 4 source comments that cite this path.
- **Research value:** none as a document (it is a snapshot). The *review shape*
  (guide-truthfulness / safety / idiomatic / ergonomics / blockers per language)
  is reusable but is only 20 lines of it.

---

#### scripts/BINDING_STRATEGY_PER_LANGUAGE.md

- **Verdict:** RESEARCH — per-VM callback/threading contract; cited normatively by 4 source files.
- **Was:** Dated 2026-05-12. Sets the "done" bar for a language binding (AZ_DEBUG
  click probe, counter 5→8 — not "compiles"), argues *against* per-VM native
  extensions in favour of one `libazul.so` + thin pure-FFI wrappers, and derives
  the only two things pure FFI genuinely can't do (cross-thread callbacks needing
  a VM lock; compile-time type safety). Contains the **per-VM lock-acquire table**
  (CPython `PyGILState_Ensure`, MRI `rb_thread_call_with_gvl`, JVM
  `AttachCurrentThread`, CLR `[UnmanagedCallersOnly]`, N-API tsfn, OCaml
  `caml_acquire_runtime_system`, SBCL/Chez auto-attach, Lua/Perl/PHP = no lock →
  writeback-only) plus the "module system principle" (split emitted bindings along
  api.json's 24 modules) and a per-language status table.
- **Landed:** The module-system principle shipped for the JVM: `AzulNative<Module>`
  per-module classes at `doc/src/codegen/v2/lang_java/mod.rs:92-97` (rationale
  cites the 64KB `<clinit>` cap verbatim) and `functions::generate_native_module_files`.
  `ThreadCallback` is now in `HOST_INVOKER_KINDS`
  (`managed_host_invoker.rs:74-84`) with the per-VM table cited in the comment at
  `:78`. Enum constants landed in examples (`examples/node/hello-world.js:19`
  `Update.DoNothing`; `examples/ruby/hello-world.rb:15` `Azul::Update::DoNothing`).
  Node/Ruby/OCaml/C# codegen fixes and hello-world rewrites all shipped (see the
  2026-07-04 review). PyO3 was **kept** for Python, contradicting the doc's
  speculative "kill the extension model" question — the note at
  `BINDING_STRATEGY:672-682` ("What does NOT change") is what actually held.
- **Superseded by:** partly by `LANGUAGE_EXPANSION_RESEARCH.md` §5 (which reprints
  the lock table) and by `CI_ONLY_LANGS_RESEARCH` for the mechanics — but this is
  the *original* and the one the code points at.
- **Still open:** `WriteBackCallback` is **not** in `HOST_INVOKER_KINDS`
  (`managed_host_invoker.rs:54-84`) — the doc's §"Phase 5 design notes" explains
  exactly why (`impl_managed_callback!` assumes arg 2 is the info-ty with
  `get_ctx()`; WriteBackCallback's arg 2 is a plain `RefAny`) and lists three
  unresolved options. No per-language ThreadCallback thunk actually calls
  `PyGILState_Ensure`/`rb_thread_call_with_gvl` yet — grep finds those symbols
  only in the comment at `managed_host_invoker.rs:78`, nowhere in an emitter. The
  2026-05-12 "user review" status table is stale and misleading; strip it.
- **Research value:** **High.** The per-VM lock-acquire matrix + the "two things
  pure FFI cannot do, and both have ~5-50 line fixes" argument is the transferable
  core: *why one C-ABI .so beats N native-extension crates*. Also the
  language-native-module-system principle and its concrete JVM payoff (64KB
  `<clinit>` cap). Candidate for `scripts/research/` — but trim §"Concurrent-agent
  note", the 2026-05-12 status tables, and the phase plan first.

---

#### scripts/CI_ONLY_LANGS_RESEARCH_2026_07_06.md

- **Verdict:** RESEARCH — the C1/C2 FFI-capability decomposition that decides archetype A vs B.
- **Was:** Companion FFI-mechanics dossier to `LANGUAGE_EXPANSION_RESEARCH.md`,
  narrowed to languages implemented blindly and validated only in CI. Decomposes
  bindability into **C1** (pass/return `repr(C)` structs by value in ordinary
  calls — the true falsifier) and **C2** (mint a C fn-ptr whose signature takes
  by-value aggregates — the archetype-A gate), notes Azul exports each
  callback-taking API as a triple (`Az<X>` / `Az<X>WithCtx` / `Az<X>Struct`), then
  gives 5-question dossiers for Fortran, PowerShell, D, Crystal, V, Julia, Swift,
  Dart, Elixir/Erlang, Guile, Chez, Racket, Tcl, Ada, Pony, Janet.
- **Landed:** D, Crystal, V, Swift, Julia all shipped as emitters
  (`doc/src/codegen/v2/lang_{d,crystal,v,swift,julia}/`, emitted at
  `generator.rs:308+`, BETA tier at `e2e_language_matrix.sh:128`); the commit that
  last touched this file is literally `f56ee544f feat(bindings): add
  D/Crystal/V/Swift/Julia emitters → 29 frontpage`. Its headline Fortran bug is
  **fixed**: `doc/src/codegen/v2/lang_fortran/layout.rs` now computes exact
  tagged-union size/alignment per variant and emits sized opaque blobs
  (`layout.rs:129-208, 269-277`), and `lang_fortran/managed.rs:22-38` emits the
  real `(id, arg0.., out_ptr)` invoker signature with dispatch bodies. Fortran is
  consequently SHIPPED-tier and CI-gating (`e2e_language_matrix.sh:118, 196`).
- **Superseded by:** n/a — it is the mechanics half of a two-doc pair.
- **Still open:** Dart, Guile, Chez, Tcl, Pony, Janet, Elixir/Erlang have **no
  emitter** — none of `lang_dart`/`lang_guile`/`lang_chez`/`lang_tcl`/`lang_pony`/
  `lang_janet`/`lang_elixir` exists. The doc's Tcl warning ("`cffi::callback`
  trampolines are call-scoped; validate before committing") is unvalidated. The
  recommendation to flip Fortran to pure archetype A (`c_funloc` of a `bind(C)`
  procedure) was **not** taken — it still emits host-invoker.
- **Research value:** **High.** C1-vs-C2 is the crisp, reusable test for "can
  language X bind a C-ABI library, and by which archetype" — sharper than the
  usual "does it have FFI". Directly comparable to what SWIG/UniFFI decide
  implicitly. The per-language dossier format (archetype / mechanism / SBV /
  showstopper / difficulty+CI action) is a reusable template.

---

#### scripts/LANGUAGE_EXPANSION_RESEARCH.md

- **Verdict:** RESEARCH — the genericity thesis, archetype taxonomy, and falsification test. Best keeper in this cluster.
- **Was:** 2026-07-06 survey of ~40 unbound languages against the thesis "Azul's
  C-ABI + host-invoker codegen can bind ANY language". Defines the two archetypes
  (**A** C-ABI-direct real fn-ptrs; **B** host-invoker: one static thunk per kind
  inside libazul, host registers one pointer-only invoker per kind plus a shared
  `AzApp_setHostHandleReleaser`, with `AzApp_setGenericInvoker` as the weakest-ask
  escape hatch). States the ONE requirement (load a .so + call an `extern "C"` fn),
  runs a **falsification test** (only genuine "no"s are languages that removed FFI
  on purpose — Starlark, Elm — or sandbox VMs: WASM, BEAM-without-NIF), gives a
  40-row master table with an effort estimate + template emitter per language, a
  4-wave roadmap, a **"future language" checklist** (3 mandatory + one-of-two
  archetype gates + 3 nice-to-haves), and a fully cited source list (~35 URLs).
- **Landed:** Wave 1 is essentially complete — odin, nim, d, crystal, v, swift all
  have emitters and are BETA-tier. Wave 2 partly: racket (`lang_racket/`) and julia
  (`lang_julia/`) shipped. The architecture claims are accurate against
  `core/src/host_invoker.rs` and `managed_host_invoker.rs:37-84`. `to_kebab_case`
  it mentions for Scheme naming does live in `managed_host_invoker.rs`.
- **Superseded by:** n/a.
- **Still open:** Objective-C, Vala, Hare, Nelua, Mojo, Chapel (Wave 1 stretch);
  Dart, Chez, Guile, Tcl (Wave 2); the entire **Wave 3 riders** — F#, VB.NET,
  Clojure, Groovy, Elixir, Erlang, Gleam — which the doc rates trivial-to-moderate
  precisely because they *reuse* the shipped C#/Java bindings, and none of which
  was attempted; Wave 4 (R, SWI-Prolog, Mojo, the assembly genericity proof,
  AssemblyScript). Carbon/Jai correctly blocked on toolchain.
- **Research value:** **Highest in this cluster.** Archetype A/B + "the one
  requirement is the C ABI floor" + the falsification test + the future-language
  checklist form a self-contained, citable design rationale for single-source-of-
  truth C-ABI binding generation, and it is the natural place to hang a
  comparison against SWIG / cbindgen / UniFFI / PyO3 / Qt moc (the doc implicitly
  argues against all of them by making the host side ~3 primitives). Move to
  `scripts/research/` as-is; the wave plan section is the only dated part.

---

#### scripts/WIRING_nim.md

- **Verdict:** DELETE — six-step wiring checklist, fully applied; Nim ships today.
- **Was:** Coordination hand-off telling the orchestrating agent exactly which
  shared files to edit to activate an already-written `lang_nim` generator:
  register the module, emit `azul.nim`, add to `deploy.rs::BINDING_FILES`, add
  `api.json` `installation.languages["nim"]` + `exampleFiles`, add the
  `lang_nim()` e2e recipe, install `jiro4989/setup-nim-action` in CI. Explicitly
  said to keep Nim ALPHA and *out* of `tabOrder`.
- **Landed:** Every step applied and then some. `doc/src/codegen/v2/lang_nim/`
  exists; emission at `generator.rs:291-295` writes `target/codegen/azul.nim`
  (file present on disk); `doc/src/dllgen/deploy.rs:827-828` ships `azul.nim` +
  `hello-world.nim`; `api.json:2125` (`installation.languages.nim`),
  `api.json:3673` (`exampleFiles`), and `api.json:29` — Nim **is** in `tabOrder`,
  contradicting the doc's instruction; `scripts/e2e_language_matrix.sh:1571-1588`
  has the `lang_nim()` recipe (with the doc's `hello_world_e2e.nim` valid-ident
  copy trick); `.github/workflows/rust.yml:2716` installs Nim;
  `.../rust.yml:2623` includes `nim` in the scripting family;
  `doc/guide/en/hello-world/nim.md` exists; `FRONTPAGE_LANGUAGES`
  (`doc/src/docgen/mod.rs:167`) lists it.
- **Superseded by:** the applied state itself; tier now BETA
  (`scripts/e2e_language_matrix.sh:128`), not ALPHA.
- **Still open:** none. (Promotion past ALPHA/`tabOrder` was a deliberate later
  decision recorded in `docgen/mod.rs:162-167`, not a leftover.)
- **Research value:** none — pure mechanical checklist. The only transferable bit
  ("a new binding costs 6 shared-file edits: module reg, emit, deploy, api.json,
  e2e recipe, CI toolchain") is already stated better in
  `CI_ONLY_LANGS_RESEARCH_2026_07_06.md` §4.

---

#### scripts/WIRING_racket.md

- **Verdict:** DELETE — wiring checklist applied; Racket ships (BETA), one cosmetic gap.
- **Was:** Same shape as WIRING_nim: activate the already-written `lang_racket`
  generator. Six edits (mod.rs registration + `generate_racket`, emit `azul.rkt`
  **and** `info.rkt`, `deploy.rs` entries for all three files, `api.json`
  installation block using `AZ_LIB_DIR`, `ALL_LANGS` + `lang_racket()` recipe,
  `Bogdanp/setup-racket` in CI). Notes Racket's `ffi/unsafe` is libffi-backed so
  `_fun` closures are real C fn-ptrs (archetype A mechanically), and that GC
  retention is handled in `lang_racket/managed.rs` via a `live-pins` list + an
  `azul-handles` hash.
- **Landed:** `doc/src/codegen/v2/lang_racket/{mod,types,functions,managed,wrappers,pkg}.rs`
  all exist; emission at `generator.rs:296-300`; `target/codegen/azul.rkt`
  present; `doc/src/dllgen/deploy.rs:829-830`; `api.json:2837` +
  `api.json:3681` + `tabOrder` at `api.json:30`;
  `scripts/e2e_language_matrix.sh:1594-1608` recipe (with `AZ_LIB_DIR=.`);
  `.github/workflows/rust.yml:2738` `Bogdanp/setup-racket@v1.11`;
  `doc/guide/en/hello-world/racket.md`; `FRONTPAGE_LANGUAGES` at
  `docgen/mod.rs:167`.
- **Superseded by:** the applied state; tier BETA, not ALPHA as the doc directed.
- **Still open:** **`info.rkt` is never generated.** `lang_racket/pkg.rs:13`
  defines `generate_info_rkt` and `lang_racket/mod.rs:80-81` documents it, but
  `generator.rs:296-300` writes only `azul.rkt`, and `deploy.rs:829-830` lists only
  `azul.rkt` + `hello-world.rkt`. The e2e recipe still tries to copy it
  (`e2e_language_matrix.sh:1600`, `|| true` so it silently no-ops) and cleans it up
  (`:836`). Dead code + a dead copy step: either wire the second `write_string` or
  drop `pkg.rs`. The doc's future `raco pkg` publish job also never happened.
- **Research value:** none.

---

#### scripts/WIRING_red.md

- **Verdict:** DELETE — checklist applied; the two generator follow-ups belong in code TODOs, not a wiring doc.
- **Was:** Fortran-shaped wiring checklist for the Red/System binding (module
  registration + `generate_red`, emit `azul.reds`, `api.json` install block +
  example map but **explicitly not** `tabOrder`, `deploy.rs` entries, ALPHA-tier
  e2e recipe using `redc`, and a CI section noting no maintained Red GitHub
  Action exists so a permanent SKIP is acceptable). Closes with two named
  generator follow-ups: exact tagged-union sizing via the shared Fortran/Pascal
  layout pass, and `i64`/`u64` mapping once a verified Red/System int64 exists.
- **Landed:** `doc/src/codegen/v2/lang_red/` exists; emission at
  `generator.rs:301-305`; `target/codegen/azul.reds` present on disk;
  `deploy.rs:831-832`; `api.json:2905` + `api.json:3682`;
  `scripts/e2e_language_matrix.sh:1612-1625` `lang_red()` recipe guarded by
  `have redc`; `.github/workflows/rust.yml:2921-2923` explicitly documents *not*
  installing `redc` — exactly the "permanent SKIP" option the doc offered.
  `doc/guide/en/hello-world/red.md` exists (marked experimental).
- **Superseded by:** the applied state — plus the doc's `tabOrder`/`FRONTPAGE`
  prohibitions were overridden: Red **is** in `api.json:31` `tabOrder` and in
  `FRONTPAGE_LANGUAGES` (`doc/src/docgen/mod.rs:167`), and is BETA not ALPHA
  (`e2e_language_matrix.sh:128`).
- **Still open:** Both generator follow-ups are live and already have in-code
  markers — `doc/src/codegen/v2/lang_red/mod.rs:221-232` `emit_union_opaque` still
  emits `opaque [byte-ptr!]` with a literal `TODO2: opaque union — needs exact
  layout size` comment, and `i64`/`u64`/`GLint64`/`GLuint64` still map to
  `byte-ptr!` at `lang_red/mod.rs:276`. **Red has never been compiled**: no
  `redc` anywhere in CI, and `rust.yml:2922` calls the binding "ALPHA/broken" —
  which contradicts its BETA tier and its frontpage/tabOrder placement. That
  inconsistency is the real open item, and it belongs in the tier table, not here.
  `lang_red/mod.rs` cites this file's path twice — fix those comments to point at
  `RED_FFI_FINDINGS.md` before removing.
- **Research value:** none beyond what `RED_FFI_FINDINGS.md` already carries.

---

#### scripts/RED_FFI_FINDINGS.md

- **Verdict:** RESEARCH — a deliberate falsification test of the "bind any language" thesis, fully cited.
- **Was:** 2026-07-06 documentation-only audit asking whether Red (red-lang.org)
  can drive an Azul GUI, run **as a falsification attempt** rather than an
  advocacy piece. Verdict: FEASIBLE via Red/System (the low-level dialect), ALPHA.
  Quotes the Red/System spec for `#import`, the `value` keyword for struct-by-value
  args *and* returns, `:functionName` for fn addresses, and the `[cdecl]` /
  `[callback]` attributes — then argues the binding should still use the
  host-invoker path (all-pointers + one out-pointer) to avoid the
  least-exercised corner of Red/System's by-value-struct-in-a-callback path.
  Section "Honest limits" enumerates 5 caveats, including "it is Red/System, not
  interpreted Red" and arm64 >16-byte aggregate rules.
- **Landed:** `doc/src/codegen/v2/lang_red/` implements exactly the described
  design (host-invoker, `[callback]`-attributed dispatcher, handle ids as
  pointer-width — see `lang_red/mod.rs:405-413`). The doc is cited from 5 live
  locations (generator, example, guide ×2, e2e recipe at
  `scripts/e2e_language_matrix.sh:1620`).
- **Superseded by:** n/a.
- **Still open:** every one of the doc's own "what Red would need to be green" —
  verified Red/System int64 (still `byte-ptr!` at `lang_red/mod.rs:276`), a `redc`
  in CI (deliberately absent, `rust.yml:2921-2923`), and arm64 >16-byte aggregate
  confirmation. Union sizing still `TODO2` (`lang_red/mod.rs:221-232`). **Red has
  never been compiled by any toolchain, anywhere.**
- **Research value:** **High, and unusual.** It is the cluster's only worked
  *negative-control* — the team picked the most awkward candidate they could find
  and documented, with citations, exactly where the thesis strains (a two-dialect
  language where only the low-level half can reach C; a 32-bit `integer!`). The
  transferable concept is "when a binding thesis claims universality, run a
  deliberate falsification test on the worst candidate and publish the honest
  limits" — plus the concrete rule that the host-invoker's all-pointers signature
  is what buys ABI safety on unproven FFIs. Keep alongside
  `LANGUAGE_EXPANSION_RESEARCH.md`.

---

#### scripts/CODEGEN_BINDINGS_PLAN.md

- **Verdict:** DELETE — six-language wave plan fully executed 15 months of commits ago.
- **Was:** 2026-05-09 plan to add six bindings in two waves — S-tier C#/Ruby/Lua,
  B/C-tier Pascal/Ada/FreeBASIC — the second wave explicitly as a "universal
  framework" showcase rather than for audience size. Specifies the
  `lang_<lang>/{mod,types,functions,wrappers}.rs` subdir template, the hard
  requirement that every binding wrap `_delete` in a language-native destructor
  (with a per-language table: `IDisposable`, `ObjectSpace.define_finalizer`,
  `__gc`, `destructor Destroy; override;`, `Ada.Finalization.Controlled`, FB
  `Destructor`), idiomatic naming per language, and — the notable coordination
  idea — a `scripts/api-json-additions/<lang>.json` sidecar per agent so parallel
  agents never conflict-edit the single shared `api.json`.
- **Landed:** All six exist and are SHIPPED- or gate-tier:
  `doc/src/codegen/v2/lang_{csharp,ruby,lua,pascal,ada,freebasic}/`; manifests
  emitted at `generator.rs:193` (`Azul.csproj`), `:204` (`azul.gemspec`), `:220`
  (`azul-<ver>-1.rockspec`, with a comment about keeping the filename in sync with
  the internal `version =`), `:256` (`azul.gpr`). `examples/{csharp,ruby,lua,
  pascal,ada,freebasic}/` all exist. The sidecar mechanism shipped too —
  `scripts/api-json-additions/` contains `ada.json`, `algol68.json`, `cobol.json`,
  `csharp.json`, `fortran.json`, so it outlived the original six.
  `doc/src/codegen/experimental/` — the "26 prior-art generators, reference
  material, not callable" the plan was reading from — **has been deleted**.
- **Superseded by:** `LANGUAGE_EXPANSION_RESEARCH.md` (a far better statement of
  the "universal framework" showcase argument) and by the tier machinery in
  `scripts/e2e_language_matrix.sh`, which replaced this plan's acceptance criteria
  with an executable gate.
- **Still open:** none. The plan's out-of-scope list (CI, package publishing) was
  subsequently done anyway — CI runs 27 langs and a luarocks publish job exists.
- **Research value:** none uniquely. The wrapper-destructor-per-language table is
  the only durable nugget and it is a 6-row table; fold it into
  `LANGUAGE_EXPANSION_RESEARCH.md` if anything.

---

#### scripts/CPP_CODEGEN_MODERNIZATION.md

- **Verdict:** ACTIVE — 8 of 10 phases landed; one feature sits behind a hard-coded `false`.
- **Was:** 2026-05-02 ten-phase, explicitly **type-driven** plan so each
  `azul<NN>.hpp` delivers its standard's features instead of inheriting the C++11
  baseline. Its central discipline: *no `if class_name == "Dom"` branches anywhere*
  — every dispatch is on `TypeCategory`, `callback_wrapper_info`,
  `EnumVariantKind`, `FieldRefKind`, or a `method_name` pattern. Opens with a
  feature-count audit per standard, records three already-fixed codegen blockers
  (predicate divergence between `type_has_wrapper`/`should_skip_class`;
  unconditional `explicit` ctors; `should_substitute_callbacks` missing
  `MethodMut`/`StaticMethod`), and an explicit "dropped from earlier drafts" list.
- **Landed:** Phase 0 — `TypeCategory::{Option, Result}` at `ir.rs:757, 762`,
  classified at `ir_builder.rs:2329-2332, 2410-2413`, consumed by
  `lang_cpp/common.rs:241-256`. Phase 2 — string_view overloads
  (`common.rs:1028-1080`, `cpp17.rs:596, 604`). Phase 3 —
  `common.rs:275 get_result_payload_types`, real `std::expected` emission at
  `cpp20.rs:693-758` (the `// TODO` the plan called out is gone). Phase 4 —
  `common.rs:577 generate_structured_binding_specs`, called from cpp14/17/20
  (`cpp14.rs:117`, `cpp17.rs:118`, `cpp20.rs:124, 471`). Phase 5 —
  `concept ReflectableModel` at `common.rs:1508`, constraining `upcast`/`downcast`
  at `:1529, 1625`. Phase 6 — `common.rs:499 generate_module_partition`, wired at
  `lang_cpp/mod.rs:257-258` and `generator.rs:139`. Phase 9 —
  `generate_reflect_macro` is now called **only** from `cpp03.rs:40`, and
  `cpp14.rs` exists + `Cpp14Generator`/`Cpp23Generator` are both wired in
  `lang_cpp/mod.rs:219-226` (no alias).
- **Superseded by:** n/a.
- **Still open:** **Phase 8 (deducing-`this`) is implemented but dead.**
  `cpp20.rs:907` and `:1017` both contain a literal `let use_deducing_this =
  false;` with a comment that `this Self&&` needs clang-18+, so the branch at
  `:908-914` never fires. Someone must decide: bump the CI toolchain floor, gate
  on a `__cpp_explicit_this_parameter` feature test, or delete the branch. Phase 8
  also called for splitting `cpp23.rs` out of `cpp20.rs` — never done; C++23 lives
  in `cpp20.rs:387+`. Phase 7 (designated-init POD audit) has no
  `is_designated_init_eligible` predicate in the tree — it was framed as a
  documentation/verification phase and appears simply skipped. The
  "wrapper-typed callbacks need a per-call-site templated `extern "C"`
  trampoline" item is still deferred indefinitely (plan §"Out of scope").
- **Research value:** Moderate. The transferable rule is the type-driven-codegen
  discipline — *never dispatch on a class name in a generator; dispatch on an IR
  category, and if you're tempted to special-case a class, the predicate is
  wrong*. Plus the negative-control observation that two predicates
  (`type_has_wrapper` vs `should_skip_class`) silently diverged. Worth extracting
  as a short note; the phase-by-phase body is not.

---

#### scripts/GETTER_MIGRATION_PLAN.md

- **Verdict:** ACTIVE — migration ~80% done; its stated motivation was achieved by a different design.
- **Was:** 2026-02-17 plan (the oldest doc in this cluster, and the only one not
  about bindings — it is CSS property access in `layout/`). Goal: route ALL CSS
  property reads through `layout/src/solver3/getters.rs` so the BTreeMap-backed
  `CssPropertyCache` could later be swapped for an FxHashMap system fronted by the
  3-tier compact cache. Inventories which properties already have a compact fast
  path (21 bitpacked Tier-1, ~30 Tier-2 `CompactNodeProps`, 6 Tier-2b
  `CompactTextProps`) and tabulates ~67 remaining direct-cache call sites across 9
  files, then names a getter per property in 6 phases. Phase 6 explicitly
  **decides** to leave `core/` (gpu.rs, styled_dom.rs) alone as cold-path.
- **Landed:** `layout/src/solver3/getters.rs` is now 313KB with 60 `pub fn get_*`.
  Direct `css_property_cache` access has collapsed in `layout/`: taffy_bridge
  9→1 (`taffy_bridge.rs:656`), fc.rs 23→5, display_list.rs 12→4, cache.rs 2→1,
  hit_test.rs 1→2. Several planned getters exist under the planned or adjacent
  names — `get_shape_inside`, `get_column_count`, `get_opacity`, `get_filter`,
  `get_box_shadow_left`, `get_text_shadow`, `get_transform`, `get_counter_reset`,
  `get_counter_increment`, plus `get_line_height_value` (`getters.rs:5470`) and
  `get_text_indent_value` (`:5485`). taffy_bridge now imports getters at
  `taffy_bridge.rs:11, 338, 686, 690, 719, 1293, 1693`.
- **Superseded by:** the compact-cache work. The plan's *reason* for existing —
  "replace the BTreeMap-based `CssPropertyCache`" — was achieved without
  FxHashMap: `core/src/prop_cache.rs:708-738` now stores `cascaded_props` /
  `css_props` as `FlatVecVec<StatefulCssProperty>` ("Replaces the per-pseudo-state
  `BTreeMap` approach", `prop_cache.rs:338`) plus
  `compact_cache: Option<CompactLayoutCache>`. See the COMPACT_CACHE_* docs.
- **Still open:** ~13 direct-cache sites remain in `layout/` — `fc.rs:4480, 4538,
  4686, 4708, 4727` (border/border-spacing resolver), `taffy_bridge.rs:656`,
  `display_list.rs:2088, 2540, 2556, 3849` (filter/backdrop-filter/box-shadow),
  `cache.rs`, `hit_test.rs`. The flex/grid getters from Phase 1
  (`get_flex_grow`, `get_flex_shrink`, `get_align_self`, `get_justify_items`,
  `get_gap`, `get_grid_template_*`, `get_grid_auto_*`) were **never created** —
  taffy_bridge reaches those another way. `core/src/styled_dom.rs` still has 46
  direct accesses and `core/src/gpu.rs` 7 — but the plan deliberately signed off
  on that ("Decision: Keep core/ accesses as-is for now"), so it is not a leftover.
- **Research value:** none transferable. The one durable idea — "funnel every read
  through a single getter module so the backing store becomes swappable" — is a
  one-line principle, and the property inventory is stale.

---

#### scripts/problems/api-validation.md

- **Verdict:** DELETE — Phase-0 audit whose two conclusions are now encoded in `capability.rs`.
- **Was:** 2026-05-29 source-of-truth audit of `dll/src/desktop/extra/*` device
  backends (camera, audio, UDP, sensors, gamepad, geolocation, keyring,
  biometric, video codec) done before fixing, referencing
  `scripts/problems/problems-{windows,linux,macos}.txt`. Two headline conclusions:
  (1) camera is **not** an NV12 problem — all four backends already emit RGBA8, so
  the macOS white screen was misattributed; (2) "feature unavailable" signalling
  is inconsistent — only keyring and biometric expose a real result enum with an
  `Unavailable` variant, everything else returns ambiguous `0`/null sentinels.
  Names `biometric::probe_availability` as the template to copy.
- **Landed:** The capability probe shipped as
  `dll/src/desktop/extra/capability.rs`, whose module doc opens "PlatformCapability
  probes … (Phase 2, item c)" and cites the bug reports' contract verbatim
  (`capability.rs:1-18`). `PlatformCapability { available, backend, reason }` at
  `:26-36` with 11 probes at `:50-268` (udp, camera, screen_capture, microphone,
  audio_output, sensors, gamepad, geolocation, keyring, biometric, video_codec) —
  every subsystem the doc listed as lacking one, plus screen_capture. It is
  exposed through the C ABI as the doc demanded: `api.json:5999-6088` declares
  `PlatformCapability` external to `azul_dll::unified::capability::PlatformCapability`
  with one `fn_body` per probe. The logging gap is closed —
  `dll/src/desktop/extra/camera/mod.rs:32, 46, 60, 76` emit
  `plog_info!("[camera] registering … → RGBA")` per backend. The gilrs double-free
  (C5) was addressed by vendoring: `Cargo.lock` now has `gilrs-azul` /
  `gilrs-core-azul`, not upstream `gilrs`.
- **Superseded by:** `dll/src/desktop/extra/capability.rs` — the per-subsystem
  availability table now lives in compilable, C-ABI-exported code instead of a
  markdown table, which is strictly better.
- **Still open:** two stubs the doc already flagged as stubs, not regressions —
  `geolocation::probe_last_fix()` still `return None` unconditionally
  (`dll/src/desktop/extra/geolocation/mod.rs:72-74`; Linux geoclue/D-Bus never
  landed), and `video_codec` is still a no-op wherever `backend() == "none"`
  (`video_codec/mod.rs:68-76, 196, 520` — i.e. Linux/Windows). Both are
  self-reporting via `PlatformCapability`, so they fail honestly. The doc's
  action item "audit the demo crates for `unwrap()` on these sentinel returns" has
  no visible evidence of completion.
- **Research value:** none transferable. The one reusable idea — *make "feature
  unavailable" a typed, probeable value rather than an ambiguous sentinel, and
  copy the one subsystem that already got it right* — is now self-documenting in
  `capability.rs`.

---

### Tallies

| Verdict | Count | Files |
|---|---|---|
| RESEARCH | 4 | LANGUAGE_EXPANSION_RESEARCH, CI_ONLY_LANGS_RESEARCH_2026_07_06, BINDING_STRATEGY_PER_LANGUAGE, RED_FFI_FINDINGS |
| DELETE | 5 | WIRING_nim, WIRING_racket, WIRING_red, CODEGEN_BINDINGS_PLAN, problems/api-validation |
| ACTIVE | 2 | CPP_CODEGEN_MODERNIZATION, GETTER_MIGRATION_PLAN |
| ARCHIVE | 1 | BINDINGS_REVIEW_2026_07_04 |

### Do Nim / Racket / Red actually ship?

**All three ship — the expansion was not dropped.** Each has a generator
(`doc/src/codegen/v2/lang_{nim,racket,red}/`), is emitted by
`generator.rs:287-305` into `target/codegen/azul.{nim,rkt,reds}` (all three files
verified present on disk), is deployed by `doc/src/dllgen/deploy.rs:827-832`, has
an `api.json` installation block + `exampleFiles` entry + a slot in `tabOrder`
(`api.json:29-31`), an example dir, a guide page, an e2e recipe
(`scripts/e2e_language_matrix.sh:1571/1594/1612`), and a `FRONTPAGE_LANGUAGES`
entry (`doc/src/docgen/mod.rs:167`). All three sit at **BETA** tier
(`e2e_language_matrix.sh:128`) — they never gate CI.

Caveat, and it is a real one: **Nim and Racket are CI-exercised; Red is not.**
`rust.yml:2716` installs Nim and `:2738` installs Racket, so their rows can go
green. `rust.yml:2921-2923` says `redc` is "intentionally NOT installed" and calls
the binding "ALPHA/broken", so `lang_red` permanently SKIPs — Red's generated
`azul.reds` has never been compiled by any toolchain. That "ALPHA/broken" label
directly contradicts Red's BETA tier, its `tabOrder` slot, and its frontpage
placement.


## Part 14 — E2E / testing / debugger docs

Audited 2026-08-01 against `master` (working tree). Every status line in every doc below was
re-derived from source; none was trusted. Note: `.claude/worktrees/` contains stale copies of the
tree — all `path:line` references below are from the real repo root.

**Global fact worth carrying into the report:** of these 12 files, exactly **one** is wired into a
build: `scripts/DEBUG_API.md` is a `design_docs` entry in `doc/autodoc-groups.toml:572` and is
read+embedded by `doc/src/reftest/autodoc.rs:512,522` (`project_root.join("scripts").join(d)`).
Moving or deleting it silently degrades the generated `doc/guide/en/debugging.md`. The other 11
have no build-time consumer (`grep design_docs doc/autodoc-groups.toml` → 12 entries, none of the
other 11).

---

#### scripts/E2E_PLAN.md

- **Verdict:** RESEARCH — the design that produced the shipped harness; Phases 2–5 still unbuilt.
- **Was:** 94 KB plan (2026-07-11) for headless redraw-correctness testing at scale. Establishes
  that the headless window *is* the engine (same `CpuBackend::render_frame` all 6 shells call), so
  headless damage testing is real testing. Defines a three-tier assertion model (Tier 1
  deterministic self-comparison invariants = the CI gate; Tier 2 advisory LLM screenshot sweep;
  Tier 3 forbidden), an explicit **no-geometry rule** (geometry belongs to `azul-doc reftest`), the
  "a test is a scripted interaction timeline, not a DOM snapshot" format, and a 3-stage
  generate/fan-out/gate pipeline.
- **Landed:** all four named engine gaps closed. `mount` op `layout/src/e2e/full.rs:2117,2121`
  (dispatch `:11885`) — note the plan *recommended* Option A (a test-host binary); the
  implementation took Option B (a real op). Virtual clock: `TickMs` `full.rs:2169`, offset field
  `:303`. `assert_resource_counts` `full.rs:3827,5645`. `assert_work_bounded` `full.rs:3826,5478`.
  Auto-baseline killed — missing reference PNG is now explicitly RED (`full.rs:4544` "a missing
  reference must be RED, never a silent…"). 23 `assert_*` ops now dispatch at `full.rs:3821-3835`
  including all six Tier-1 ops the plan designed (`assert_idle_stable`, `assert_changed`,
  `assert_damage_sound`, `assert_composition`, `assert_manager_invariants`,
  `assert_state_machines_idle`). Phase 3 CI landed: blocking `e2e_headless` job,
  `.github/workflows/rust.yml:2496,2558`, in `deploy_pages.needs` `:3862`.
  The plan is still a **live input**: `scripts/gen_e2e_cases.py:14` derives its assertion families
  from "`scripts/E2E_PLAN.md` §B (a..f, g1..g5)".
- **Superseded by:** partially by `E2E_READINESS_2026_07_25.md` for status; not for design.
- **Still open:** **Phase 2 (generation) never ran** — `e2e/` holds 38 hand-written scenarios, not
  the ~2000 the plan targets, and `e2e/gen/` does not exist. Phase 4 (coverage-gap `--fable`
  overlay over `managers/`+`cpurender/`) and Phase 5 (Tier-2 LLM sweep + native-vs-headless
  consistency) untouched. Three ops the schema (§C.2) proposes are still absent from `full.rs`:
  `snapshot_resources`… (present), but `assert_no_growth` and `capture` are **not** implemented
  (0 hits each). The doc's own header still reads "plan / not implemented", which is now false.
- **Research value:** **highest of the 12.** Transferable: (a) determinism via *self*-comparison
  (frame N vs N+1, full-repaint vs damage-driven render, run 1 vs run 2) inside one process, which
  makes font/DPI/runner variance cancel — this is the reason the harness needs almost no pinned
  environment, unlike Playwright/WPT reftests; (b) the hard scope split "behaviour over time here,
  pixel/geometry correctness in browser reftests"; (c) Tier 1 assertions carry **no expected
  values**, which is the structural defence against bug-enshrinement when cheap agents generate
  tests; (d) the F1–F10 risk table (bug-enshrinement, red-test flood triaged by failure signature
  not by test).

---

#### scripts/E2E_PROTOCOL_AUDIT.md

- **Verdict:** ACTIVE — top-3 recommendations landed; ~10 input holes verified still open.
- **Was:** Audit (2026-07-13, at `983357ebd`) testing the thesis "the op surface should be exactly
  the headless window's I/O boundary". Enumerates the union of what all six platform shells inject,
  diffs it against `DebugEvent`, and finds ~half missing (H1–H15), five zombie ops that returned
  `ok` while doing nothing, and ~30 IDE/component ops that are not window I/O at all. Headline
  finding §2.C: the texture/image-callback damage contract was broken on every platform.
- **Landed:** All five zombies now have real arms (`DebugEvent::Focus|Blur|Move|DpiChanged|GetDom`,
  1 match arm each in `layout/src/e2e/full.rs`) with corpus scenarios
  `e2e/op-focus-blur.json`, `op-move.json`, `op-dpi-changed.json`, `op-get-dom.json`.
  **H1 is FIXED and better than proposed:** `invoke_cpu_image_callbacks` + its side map are gone;
  `LayoutWindow::invoke_image_callbacks_into_overlay` (`layout/src/window.rs:3140`, called from
  `:2858`/`:2875` inside `LayoutWindow` so *every* host incl. headless gets it) funnels each
  produced frame through the content chokepoint, "damage then falls out of `ImageRef` identity in
  the backend's display-list diff — there is NO side map" (`window.rs:3130-3134`). Regression
  scenarios `e2e/op-image-swap-repaints.json`, `e2e/op-image-cache-id-repaints.json`.
  §2.D "pollution" is enforced rather than fixed — `OP_POLICY` (`doc/src/gene2e.rs:382`) denies the
  IDE ops to the generator.
- **Superseded by:** n/a (07-14 readiness re-scored it; nothing replaces the boundary map).
- **Still open — verified by grep against `layout/src/e2e/full.rs`, all zero hits:**
  H3 IME preedit (`SetPreedit`/`set_preedit`/`Preedit`), H4 file drag & drop op (`FileHover`/
  `FileDrop` exist only as `HeadlessEvent` and as `ActiveDragType::FileDrop` state, never as an op),
  H10 pen `is_eraser`/`barrel`/`tangential`/`barrel_roll`, H11 `ScrollInputSource` on the `Scroll`
  op, H14 `CursorPosition::OutOfWindow`. Also still open per the doc: H5 clipboard inject/readback,
  H8 WM frame state, H9 theme change, H13 monitor change, and output ops O3–O8. The two-mock-surface
  drift (`DebugEvent` vs `HeadlessEvent`) is unresolved.
- **Research value:** the method — derive the protocol's *required* surface from the union of what
  N platform backends actually inject, then diff, rather than from what the protocol happens to
  declare. Directly transferable to any "headless server + client driver" test protocol (this is
  what CDP/WebDriver get wrong in the other direction). Keep §0's two-surfaces-drift argument.

---

#### scripts/E2E_READINESS_2026_07_14.md

- **Verdict:** DELETE — self-declared superseded; port §4's two unbuilt specs out first.
- **Was:** 59 KB readiness audit at `fe8165f57`. Part 1 reconciles ~10 sibling plan docs against the
  tree; Part 2 lists what landed; Part 3 names four blockers to bulk generation; Part 4 (the part
  it says matters) generalises three recurring bug classes — A false-green, B derived state that
  stops being recomputed, C silent fallback — and proposes harness mutation testing.
- **Landed:** its own header (`:3-6`) marks it SUPERSEDED and names §3.D/§3.E/§3.F2 as now FALSE.
  All four blockers confirmed fixed (see 07-25 entry).
- **Superseded by:** `scripts/E2E_READINESS_2026_07_25.md` (explicit, same commit `bcb38222a`).
- **Still open:** two designs exist **only here** in detail and are **still unimplemented**:
  the `AZ_E2E_NEUTER=op1,op2` harness-mutation harness (code sketch at `:403`, job design at `:433`)
  and `assert_no_silent_fallbacks` as a default trailer on every generated test (`:653`, ordered
  item at `:365`). 07-25 references both but does not reproduce the specs. Also unique: §4.C's
  six-live-sites silent-fallback audit and Part 5's exact build/run command block. Extract those
  before deleting.
- **Research value:** the mutation-testing invariant, stated crisply: *"for every op and every
  assertion, neutering its implementation must make at least one test FAIL; if neutering changes
  nothing, it is vacuous"* — plus the observation that a single `OnceLock`-read env var gives this
  for **one** build instead of ~107, and that the op→test coverage map is free because the op name
  appears literally as a string in the JSON (`grep -l '"op": *"scroll"'` *is* the map).

---

#### scripts/E2E_READINESS_2026_07_25.md

- **Verdict:** ACTIVE — both blocking must-dos now done; bulk generation still never run.
- **Was:** Re-audit at `b5ca32e5d`. Verdict "READY to bulk-generate, with one caveat and two
  must-dos". Records that all four 07-14 blockers are fixed, measures the runner
  (21 scenarios / 11.6 s / 39.7 MB RSS single-process → ~121 min for 13,223 tests, shardable), lists
  honest headless-runner gaps, and re-states the three bug classes with an eleven-day recurrence
  ledger (Class A recurred **six** more times in eleven days).
- **Landed since:** its caveat is closed — `azul-doc e2e` **is** in CI now:
  `.github/workflows/rust.yml:2549` (`--list`), `:2558` (`timeout 900 … azul-doc e2e e2e`), job
  `e2e_headless` `:2496`, blocking via `deploy_pages.needs` `:3862`. §1.C de-dup is DONE:
  `dll/src/desktop/shell2/common/debug_server/full.rs` no longer exists (dir now holds only
  `mod.rs`, `platform.rs`, `stub.rs`), and `platform.rs:26` does `use azul_layout::e2e::{…}`.
  §3.C's two dead counters are fixed: `relayout_iterations` is really counted
  (`layout/src/e2e/runner.rs:675`, no longer `.max(1)`) and `hit_depth_cap` has a library-path
  writer (`runner.rs:680`). §3.A's unimplemented cross-invariants X1/X4/X7/X8 are implemented
  (`full.rs:6522,6554,6583,6629`).
- **Superseded by:** n/a — this is the current status doc.
- **Still open:**
  1. **The fan-out never happened.** No `e2e/gen/`; `e2e/` has 38 JSON scenarios (+4 in
     `layout/tests/e2e_fixtures/` for the four assertion ops the corpus lacks —
     `layout/tests/e2e_json.rs:27-31`). `scripts/E2E_TESTS.txt` is 9,530 lines and
     `E2E_TESTS_WAVE1.txt` 6,812 lines of *ungenerated* case descriptions.
  2. **No triage policy written** (its must-do #2) and no `--shard` flag — `doc/src/e2erun.rs`
     has `--filter`, `--list`, `--jobs`, not `--shard`.
  3. `assert_no_silent_fallbacks` (its "single highest-value addition to Part 4") does not exist —
     0 hits outside the two readiness docs.
  4. `AZ_E2E_NEUTER` mutation job does not exist — 0 hits outside the two readiness docs.
  5. `apply_user_change`'s DLL-only arms still unported (`runner.rs:1235`), so `[clipboard/*]`,
     `[menu/*]`, `[timer/*]` corpus categories remain untrustworthy on the library path.
- **Research value:** high. (a) The **liveness-precondition rule** — *"an assertion of ABSENCE
  passes for free whenever the machinery that would produce the thing never ran; every such
  assertion needs a liveness precondition"*, and its corollary that a `#[cfg]`-gated or
  `if let Some(..)`-gated check falling through to `pass` is the same bug. Implemented as
  `frames_since_reset >= 1` gating `assert_idle_stable`/`assert_work_bounded`. (b) *"'I cannot check
  this' must be red, never green."* (c) The eleven-day false-green ledger is unusually good evidence
  that harness bugs, not engine bugs, dominate early. (d) "a classified-but-constant field is the
  same hazard as an unclassified one" — the limit of exhaustive-destructure guards.

---

#### scripts/E2E_CORPUS_SEMANTIC_REVIEW.md

- **Verdict:** ACTIVE — most structural fixes landed; the wave plan is the unexecuted deliverable.
- **Was:** Independent second-opinion review (2026-07-25) of the then-13,223-line
  `scripts/E2E_TESTS.txt` *before* generating from it. Verdict: "structurally excellent and
  semantically over-committed" — ~4,300 lines (32.6 %) described an interaction the headless runner
  silently no-ops, an op the policy denies, or an invariant declared unimplemented. Gives a
  manager×corpus coverage map (40:1 over-representation of scroll/text_edit/gesture; nine managers
  with zero drive path), an assertion-fit audit, and a four-wave generation order.
- **Landed:** §7.1 (its "highest-leverage single fix") is IN — the runner no longer silently
  no-ops: `Runner.unsupported_changes: Vec<String>` with the doc "Non-empty ⇒ the scenario FAILS"
  (`layout/src/e2e/runner.rs:96-101`, refusals at `:266`, `:545`, `:1261`).
  §7.3 fixed — `scripts/gen_e2e_cases.py:64` now reads `layout/src/e2e/full.rs`, so the generator is
  re-runnable (`--check` works). §7.4 landed — `OP_POLICY`/`KNOWN_CROSS`/`UNIMPLEMENTED_CROSS`/
  `KNOWN_MANAGERS` are parsed and applied **at expansion time** (`gen_e2e_cases.py:147,189,205`,
  drop accounting at `:1097`, summary at `:2161`); corpus shrank 13,223 → 9,530 lines / 156 tags.
  §7.5 landed — the over-invalidation family exists (610 lines matching "stays None" /
  "over-invalidat"). §7.2's second zombie layer landed as the runner-refusal list
  (`gen_e2e_cases.py:2053,2105,2114`).
- **Superseded by:** n/a.
- **Still open:** §7.6 — the two undo systems are still conflated: corpus tags are `[undo/mutate]`
  and `[undo/renumber]`, no `[undo/app-state]` vs `[undo/node-text]` split. §8 waves 1–4 never
  executed; Wave 1 was *selected* (`scripts/E2E_TESTS_WAVE1.txt`, 6,812 lines, 2026-07-27) but
  never generated into JSON. The nine hostless managers (clipboard, file_drop, permission,
  geolocation, biometric, keyring, sensors, gamepad, a11y) still have no drive op — same set as
  E2E_PROTOCOL_AUDIT's H4/H5/H15, so the two docs corroborate.
- **Research value:** the review *criterion* is the keeper — for a generated corpus, the question is
  not "is this line coherent" but "does its assertion match what the manager under test can actually
  get wrong, and can the runner even drive it". Plus the finding that **coherence must be enforced
  by REQUIRES/PROVIDES capability sets at expansion time**, not hoped for, because an LLM handed an
  incoherent one-liner will silently invent the missing step (`gen_e2e_cases.py:20-46`).

---

#### scripts/e2e_language_matrix.md

- **Verdict:** ACTIVE — live companion doc, badly drifted from its own script and from CI.
- **Was:** Reference for `scripts/e2e_language_matrix.sh`: what each of 26 language bindings needs
  (toolchain probe, CI installer, codegen artifact, example entry), how "working" is detected
  (`test result: ok` + `0 failed` after ANSI stripping), a `--strict` flag, a "CI wiring suggestion",
  and an observed macOS-aarch64 baseline dated **2026-05-27**.
- **Landed:** the script exists and grew to 115 KB (2026-07-29) and IS wired into CI —
  `.github/workflows/rust.yml:2988` runs `bash scripts/e2e_language_matrix.sh --gate-shipped …`,
  gating documented at `:2326`. So the doc's "CI wiring suggestion" is done.
- **Superseded by:** partially, by the script's own header comments (`e2e_language_matrix.sh:85-200`),
  which are now the authoritative version.
- **Still open (the drift, all verifiable):** doc says 26 languages; `ALL_LANGS`
  (`e2e_language_matrix.sh:90`) has **35** (adds go, nim, odin, racket, red, crystal, d, julia,
  swift, v). Doc knows only `--strict`; the script's CI mode is `--gate-shipped` with a three-tier
  maturity model — `SHIPPED_LANGS` (16, `:113`), `BETA_LANGS` (`:128`), ALPHA — plus
  `WINDOWS_NONGATING_LANGS` (`:157`) and `REQUIRED_LANGS` (`:194`, the anti-"everything SKIPped so
  the gate passed" guard). Doc's per-language status table is a May snapshot: it does not record
  that python was **de-gated to BETA** for a Linux-only pyo3 segfault (`:125-128`), nor that zig,
  go, pascal, scala, fortran, haskell were promoted to SHIPPED on 2026-07-04. Two files link to
  this doc (`e2e_language_matrix.sh:63`, `examples/csharp/README.md:8`), so deleting breaks
  references — refresh it or redirect those two links at the script header.
- **Research value:** modest but real — the cross-language e2e matrix concept (one identical
  "click → counter" scenario driven through every FFI binding, with SKIP/FAILS/WORKS tiers and a
  per-OS non-gating exclusion list) is a reusable pattern for any project shipping N bindings. The
  sharpest lesson is in the *script*, not the doc: `REQUIRED_LANGS` exists because "SKIP never
  gates + `continue-on-error` toolchain setup" meant every row could be SKIP and the gate still
  exited 0.

---

#### scripts/DEBUG_API.md

- **Verdict:** ACTIVE — stale content, but a live autodoc build input; do not move blind.
- **Was:** User-facing reference for the `AZ_DEBUG=<port>` in-process HTTP debug server: request/
  response envelope, per-family `curl` examples (mouse, click targeting variants, keyboard, window
  events, window/DOM/scroll/selection inspection, app-state get/set with `RefAny` metadata), the CSS
  selector grammar the `selector` param supports, and a troubleshooting section.
- **Landed:** the server and its op vocabulary shipped and grew — `layout/src/e2e/full.rs` now has
  `pub enum DebugEvent` at `:1684` with ~121 variants and 23 assertion ops. The prose is now
  generated: `doc/guide/en/debugging.md` (162 lines) covers the same ground.
- **Superseded by:** partly by `doc/guide/en/debugging.md` — but **that page is generated FROM this
  file**: `doc/autodoc-groups.toml:571-575` lists `DEBUG_API.md` under `design_docs`, and
  `doc/src/reftest/autodoc.rs:512,522` resolves it as `scripts/<name>` and embeds its full text into
  the generation prompt (under the header "Design docs (INTENT — read after the source)").
- **Still open:** the file is dated 2026-05-23 and predates ~30 ops, every `assert_*` op, `mount`,
  `unmount`, `tick_ms`, `snapshot_frame`/`snapshot_resources`, `get_frame_report`,
  `capture_damage_png`, `reset_frame_counters`, and the whole `FrameReport`/damage surface.
  `E2E_READINESS_2026_07_14.md` already flagged it: "**STALE as documentation** … Rewrite from
  `parse_schema()`'s output, or delete. Do **not** hand it to a new session as truth." The clean fix
  is to regenerate it from `doc/src/gene2e.rs::parse_schema()` (which already reads `full.rs` at
  runtime), keeping the path stable so autodoc-groups.toml needs no edit.
- **Research value:** none beyond the ops themselves — but note the anti-pattern it embodies: a
  hand-maintained protocol reference that is *fed to an LLM as intent* while the machine-readable
  truth (`full.rs`) sits one directory away. Autodoc's own preamble even warns "the code is truth,
  the design docs are context", which is exactly the seam where a stale doc leaks into shipped docs.

---

#### scripts/DEBUGGER_REQUIREMENTS.md

- **Verdict:** DELETE — all eight Phase-1 items verified implemented in the debugger UI.
- **Was:** Eight UI requirements for the in-browser debugger (`AZ_DEBUG` inspector): CSS-property
  key/value `space-between` + own scroll container + transparent default border; an Accessibility
  section; a Clip-Mask/scroll-nesting section; inline base64-PNG rendering in the terminal;
  slash-command popup with named params and per-variant examples; test-explorer redesign; an
  "Add Step" icon toolbar; and one shared `app.schema.commands` config behind both the slash
  commands and the Add-Step form.
- **Landed:** `dll/src/desktop/shell2/common/debugger/debugger.js` (4,828 lines) +
  `debugger.css` (1,568) + `debugger.html` (291). Accessibility section
  `debugger.js:874-883` (tabindex/contenteditable/role/aria-label/focusable). Clip/scroll section
  `:886-898` + async loader `_loadNodeClipInfo` `:1313`, reading `data.clip_analysis` `:1320`.
  Inline PNG in terminal `:307-326` (`_extractBase64Image`, `data:image/png;base64,`). Named-param
  slash commands `_parseSlashCommand` (2 hits) with per-command `examples:` arrays, e.g. `:178`.
  Shared schema: `app.schema.commands` (7 hits) drives both the popup and the step form.
  Test explorer + editable names + pencil affordance (`edit-icon`/✏ in both `.js` and `.css`).
  Toolbar row of icon buttons `debugger.html:212-218`.
- **Superseded by:** `DEBUGGER_REQUIREMENTS_2.md` §8, which lists these same nine as completed.
- **Still open:** none found.
- **Research value:** none — product requirements, not method.

---

#### scripts/DEBUGGER_REQUIREMENTS_2.md

- **Verdict:** DELETE — all seven Phase-2 sections verified implemented.
- **Was:** Phase-2 debugger requirements: (1) app-state snapshot save/restore/alias + a
  `restore_snapshot` e2e step + a Rust-side `snapshots` map; (2) clean project export/import with
  `snapshots`/`htmlTree`/`resolvedSymbols`/`componentRegistry`; (3) response panel as a read-only
  JSON tree; (4) fix the never-called component-registry load; (5) rename runner buttons to
  "Run"/"Run headless"/"Run all headless"; (6) function-pointer resolution with
  `source_file`/`source_line`/`hint`/`approximate` + open-in-editor; (7) pencil icons on renameable
  labels. §8 records Phase-1 as already done.
- **Landed:** snapshots — 57 hits in `debugger.js`, UI host `debugger.html:112`
  (`#snapshots-container`), command `debugger.js:178`, local handling `:4597-4598`; Rust side
  `layout/src/e2e/full.rs:2248` ("Used by `restore_snapshot` steps to look up pre-saved states").
  Export/import — `exportProject` `debugger.js:1495` with `resolvedSymbols` `:1509` and `htmlTree`
  `:1514`; import restores snapshots `:1455`. Response tree — `#step-response-tree`
  `debugger.js:1163` + `app.json.render('step-response-tree', step.lastResponse, true)` `:1167`
  (third arg = read-only). Component registry — `switchView` `:581`, panel list `:588`,
  `get_component_registry` fetched at `:1740`. Button labels — `debugger.html:212,217,218`
  ("Run", "Run headless", "Run all headless"). Symbol resolution — `ResolvedSymbolInfo` with
  `source_file`/`source_line`/`approximate` at `layout/src/e2e/full.rs:333-342`, struct `:9336`,
  resolver `:9352`, crate-path heuristic `:9445-9464`; JS cache `app.state.resolvedSymbols`
  (`debugger.js:49,1356,1367,1859`); open-in-editor via `xdg-open` (`full.rs:15999`) with a
  `vscode://file/` fallback (`debugger.js:1909-1912`). Pencil icons present in `.js` and `.css`.
- **Superseded by:** n/a.
- **Still open:** none found.
- **Research value:** none.

---

#### scripts/D1_D2_SEARCH_SPEC.md

- **Verdict:** DELETE — implemented exactly as specified; the pause reason was a tooling outage.
- **Was:** A 5.6 KB implementation spec written mid-task when the Read/Bash channel degraded to
  ~1-line truncation, making blind-editing the 382-line `azul-search.js` unsafe. Goal D1: move the
  guide search from a floating top-right overlay into an in-page column. Goal D2: scope by page —
  the guide overview keeps the pagefind full-guide search; individual guide pages get the API
  search, pre-expanded, seeded from that page's frontmatter `default-search-keys`, presented as
  direct links to API docs. Includes exact seams, a 5-step plan and verification greps.
- **Landed:** the proposed `PageKind` variant exists as `GuidePage` (spec suggested
  `GuideApiLinks`): `doc/src/docgen/mod.rs:972` (arm) and `doc/src/docgen/guide.rs:312`
  (`PageKind::GuidePage(&guide.default_search_keys)`), with the overview keeping
  `PageKind::Guide(&[])` at `guide.rs:468`. Inline mount host emitted at `guide.rs:476`
  (`<div id="azul-search-mount" class="azs-mount-inline">`), honoured by
  `doc/templates/azul-search.js:549-552`. Column CSS at `doc/templates/main.css:1045-1068`
  (`.guide-layout` flex, `.guide-search-col` sticky 320px, dark-mode token, `<1100px` collapse to a
  bottom sheet).
- **Superseded by:** n/a.
- **Still open:** none. (Its "STATUS / GIT" section — unpushed commits on
  `origin/mobile-ios-android` at `5a9199c0f` — is long obsolete; master is ~7677 commits.)
- **Research value:** none (site-specific), though it is a decent example of the "write the spec
  before the channel dies" habit.

---

#### scripts/TODO_ANALYSIS_REPORT.md

- **Verdict:** DELETE — Feb-2026 triage of a stale grep; 4 of 17 findings survive, folded below.
- **Was:** A 10 KB triage of `TODO_LIST.md` bucketing each TODO into FIXABLE NOW / NEEDS
  INVESTIGATION / LEAVE AS-IS / DOCUMENTATION ONLY, with 17 numbered findings and a
  "recommended immediate actions" list of 6 items (2 marked done in-session).
- **Landed (verified by grep today):** its two claimed fixes are real —
  `TEXT_DECORATION_UNDERLINE` is live at `core/src/ua_css.rs:433` and applied to `<a>` at `:655`;
  `is_layout_affecting` exists at `css/src/dynamic_selector.rs:1458` with a test at `:3766`.
  Its four "remove/update the TODO" actions all happened: the SIMD TODO is gone from
  `core/src/transform.rs`, the confusing `is_focusable` comment is gone from `core/src/dom.rs`,
  and both `display_list.rs` TODOs are gone — `StyleBackgroundContent::Image` is now really handled
  (`layout/src/solver3/display_list.rs:1491,1551`), so its "image backgrounds not rendered" finding
  is obsolete. `prop_cache.rs`'s "No variable support" TODO is gone; the `style.rs`
  `rule_ends_with` "this is wrong but fast" TODO is gone; `core/src/dom_table.rs` no longer exists;
  OS-version and `prefers-reduced-motion` detection were the subject of the very commit that added
  this file (`2a1e9025a`, "@os CSS selector, OsVersion comparison").
- **Superseded by:** the recomputed list under `TODO_LIST.md` below.
- **Still open (its four surviving findings):** `core/src/gl.rs:864` epoch overflow (low priority);
  `core/src/gpu.rs:187` parent size for `%` transforms; `core/src/icon.rs:625` multi-node icon
  subtree splicing; `css/src/shape_parser.rs:296,308` em/rem/vh/vw + percentage need layout context.
- **Research value:** none — but one habit worth keeping: it separates "the TODO is stale, the code
  is done" from "the TODO is a real feature gap", and four of its five stale-TODO calls were right.

---

#### scripts/TODO_LIST.md

- **Verdict:** DELETE — a raw `rg TODO` dump from 2026-02-02, reproducible in one command.
- **Was:** Verbatim ripgrep output — header "258 Ergebnisse - 87-Dateien" — of every `TODO` in the
  tree with 2 lines of context, grouped by file. Not curated, not prioritised, no owners.
- **Landed:** n/a — it is a snapshot, not a plan. Today the same grep over
  `core/ css/ layout/ dll/ doc/src/` returns **201 TODOs in 84 files**, so the total barely moved
  while the *contents* churned heavily: of its 258 entries, **126 no longer exist** in their file
  and **52 more are in files that were deleted or moved** (biggest: 18 in
  `dll/src/desktop/shell2/common/event_v2.rs`, 11 in `doc/src/codegen/ir_builder.rs`, 8 in
  `layout/src/cpurender.rs`, 3 in `dll/src/desktop/shell2/common/debug_server.rs` — all gone).
- **Superseded by:** `grep -rn TODO --include=*.rs core/ css/ layout/ dll/ doc/src/`.
- **Still open — 67 of its 258 entries survive verbatim. Checklist with CURRENT line numbers:**

  *Docs/API surface (1) — trivial, currently ships a lie in the public API docs:*
  - [ ] `api.json:37921` — `is_layout_affecting` doc still reads "TODO: Implement when CssProperty
        has this method"; the method has existed since Feb (`css/src/dynamic_selector.rs:1458`).

  *Core (6):*
  - [ ] `core/src/dom.rs:5780` — `from_xml` does not parse XML, returns a placeholder text node.
  - [ ] `core/src/gl.rs:864` — epoch overflow unhandled (self-marked low priority).
  - [ ] `core/src/gl.rs:4200` — blend func hardcoded, enable/disable not plumbed.
  - [ ] `core/src/gpu.rs:187` — `%` transforms animate against `parent_size = 0.0`; needs layout ctx.
  - [ ] `core/src/icon.rs:625` — multi-node icon replacement unsupported (arena splice missing).
  - [ ] `core/src/resources.rs:1906` + 10× `:~2028` — "check that this function is SIMD optimized" /
        "autovectorization fails spectacularly" (11 sites, one cluster).
  - [ ] `core/src/xml.rs:4313` — bare `// TODO`.

  *CSS (3):*
  - [ ] `css/src/parser2.rs:590` — no test for the `+` combinator.
  - [ ] `css/src/shape_parser.rs:296` — em/rem/vh/vw unhandled (needs layout context).
  - [ ] `css/src/shape_parser.rs:308` — percentages need container size.

  *Layout (17):*
  - [ ] `layout/src/solver3/display_list.rs:4283,4284,4293` — images always painted over glyphs;
        no z-index within inline content; no text-overflow handling.
  - [ ] `layout/src/solver3/fc.rs:3558` — `clip-path` not used for render clipping.
  - [ ] `layout/src/solver3/fc.rs:5335` — column indices only marked, not resolved.
  - [ ] `layout/src/solver3/fc.rs:8273` — margin hardcoded to default in a helper.
  - [ ] `layout/src/solver3/pagination.rs:267` — named strings not looked up from document context.
  - [ ] `layout/src/solver3/taffy_bridge.rs:1280` — grid stretch detection unimplemented.
  - [ ] `layout/src/text3/cache.rs:1701` — `initial-letter` (drop caps) not implemented.
  - [ ] `layout/src/text3/cache.rs:6318` — no re-orientation across mixed writing modes.
  - [ ] `layout/src/text3/cache.rs:9226` — no punctuation trimming at line edges.
  - [ ] `layout/src/text3/cache.rs:11138` — uncached path left in a hot function.
  - [ ] `layout/src/text3/knuth_plass.rs:482` — no demerits for adjacent lines of very different
        appearance.
  - [ ] `layout/src/icu.rs:907` — waiting on `ListFormatter::try_new_unit`.
  - [ ] `layout/src/managers/scroll_into_view.rs:444` — CSS `scroll-behavior` on the container
        is ignored.
  - [ ] `layout/src/window.rs:7047,7051` — a11y actions: no tooltip-manager integration, no custom
        action handlers.
  - [ ] `layout/src/widgets/node_graph.rs:887` — `self.clone()` per frame, flagged "expensive".
  - [ ] `layout/src/widgets/node_graph.rs:3183` — callback returns `Update::DoNothing // TODO`.
  - [ ] `layout/src/xml/svg.rs:145` — `apply_line_width` missing in lyon 17; `:1065` — radii not
        respected by current lyon.
  - [ ] `layout/tests/flexbox_stretch_bugs.rs:10` — the whole file is disabled pending integration
        test infrastructure.

  *Platform shells (17) — the largest untouched cluster; overlaps the seam-audit theme:*
  - [ ] Wayland: `linux/wayland/mod.rs:1073` native popup via `xdg_popup` never shown; `:1818`
        window positioning; `:1844` app_id never extracted from `x11_wm_classes`; `:1993` "Wayland
        limitation"; `:4053` visibility control via `xdg_toplevel`.
  - [ ] X11: `linux/x11/mod.rs:1492` `monitor_id` hardcoded to 0; `:4730` GNOME native menu via
        DBus not shown.
  - [ ] GNOME menu DBus: `linux/gnome_menu/protocol_impl.rs:251,459,483,550` — menu group not
        serialised, `(bool,string,array)` tuple not serialised, action-description dict not built,
        message parameter not parsed.
  - [ ] macOS: `macos/mod.rs:3130` objc2-open-gl disabled; `:4043` initial menu state not built
        from `layout_window`; `:7351` `invalidateMarkable` never called.
  - [ ] Windows: `windows/mod.rs:513` menu bar not extracted from window state; `:517`
        `size_to_content` unimplemented; `:543` `monitor_id` never resolved to a `Monitor`.
  - [ ] `dll/src/desktop/menu_renderer.rs:177` — child menu window IDs never tracked.
  - [ ] `dll/src/desktop/shell2/common/cpu_compositor.rs:50` — "Implement actual rasterization".
  - [ ] `dll/src/desktop/logging.rs:102` — external crash handler never invoked with the log path.

  *Tooling / examples (3):*
  - [ ] `doc/src/patch/parser.rs:790` — enum-leaf replacement logic incomplete.
  - [ ] `doc/src/print.rs:455` — signature matching is naive.
  - [ ] `examples/c/browser.c:530` — external CSS not fetched/parsed.

- **Research value:** none. If a standing backlog is wanted, generate it — do not check in a grep.

---

### Cross-cutting notes for the report

1. **The e2e harness is built; the corpus is not.** Ops, assertions, runner, xfail semantics,
   one-process runner and a blocking CI gate all shipped. What never happened is the thing all four
   e2e docs exist to enable: generating the corpus. 9,530 curated case descriptions
   (`scripts/E2E_TESTS.txt`) + 6,812 selected for Wave 1 sit unexpanded next to 38 hand-written
   scenarios in `e2e/`.
2. **`layout/src/e2e/full.rs` is now 16,553 lines** and is the single source of the protocol,
   parsed at runtime by both `doc/src/gene2e.rs` and `scripts/gen_e2e_cases.py`. That is the
   architecture the docs argued for and it held.
3. **The input boundary is still ~half-covered.** IME preedit, file drop, clipboard, theme, WM
   frame state, monitors, mouse-leave, scroll source and most pen state have no op — verified, not
   inherited from the doc.
4. **Two named safety mechanisms were designed and never built:** `AZ_E2E_NEUTER` harness mutation
   testing and `assert_no_silent_fallbacks`. Both are the direct answer to the bug class the
   readiness docs say recurred six times in eleven days.


## Part 15 — performance, startup, binary size, packaging, website (13 files)

Audited 2026-08-01 against `master` @ f1c43ba60. Every "Landed" line below was
grep/read-verified in the working tree; no doc status line was trusted.

---

#### scripts/STARTUP_LATENCY.md

- **Verdict:** RESEARCH — canonical GUI-toolkit startup-latency budget; one phase still unbuilt.
- **Was:** 43 KB plan to kill the ~700 ms `FcFontCache::build()` block plus ~100–200 ms
  synchronous WebRender shader compile in `App::create()`. Design: a `FcFontRegistry` with a
  Scout thread (filesystem enumeration only, family name guessed from filename), a Builder
  worker pool driven by a 4-level priority queue (Critical/High/Medium/Low), an on-disk bincode
  manifest under `~/.cache/azul`, and a **block-at-layout** `request_fonts()` so the first frame
  is never FOUC. Ends with an Implementation Status section claiming Phase 1 complete, Phase 2
  (shader disk cache) not started.
- **Landed:** Phase 1 is real. `dll/src/desktop/app.rs:16` imports
  `rust_fontconfig::registry::FcFontRegistry`; `app.rs:186` `pub font_registry:
  Option<Arc<FcFontRegistry>>`; `app.rs:214-233` builds the registry and documents the
  `request_fonts()` + `into_fc_font_cache()` handoff; `app.rs:143` threads it into
  `shell2::run`. `dll/Cargo.toml:57` pins `rust-fontconfig = "4.4.5"` with features
  `["std","parsing","async-registry","cache"]` — the two features the plan invented.
  `dll/src/desktop/wr_translate2.rs:325` is `precache_flags: ShaderPrecacheFlags::EMPTY`
  (the doc's lazy-shader change). `layout/Cargo.toml:174` even carries the
  `single-thread-unsafe-locks` follow-on for the web lift, and `app.rs:208` documents the
  registry's multi-thread scan racing that `StLock`.
- **Superseded by:** n/a — no later startup doc exists; `gemini_perf_response.md` §4 defers
  to this file as the correct plan.
- **Still open:** Phase 2 shader disk cache. `glGetProgramBinary`/`glProgramBinary` caching
  exists **only** inside vendored `webrender/core/src/device/gl.rs` (a
  `save_shaders_to_disk`/`set_startup_shaders` program-cache trait); nothing in `dll/src`
  implements it and there is no `azul_precompile_shaders()` symbol anywhere. The doc's own
  residual budget — 206 ms macOS NSWindow/NSOpenGLContext setup and ~156 ms of first-frame
  on-demand shader compile — is unattacked. The 570 ms "total window creation" number is
  macOS-only and ~3 months stale; treat as unverified on current master.
- **Research value:** High and transferable. The reusable ideas are (a) *no FOUC as a hard
  constraint*: never render with a placeholder font, instead move the block from process
  start to layout time so background work overlaps window/GL creation — the "free parallelism
  window"; (b) *stale cache beats no cache*: trust the on-disk manifest immediately, verify
  asynchronously; (c) priority promotion driven by filename heuristics, so a CSS
  `font-family` miss can boost the exact files to Critical; (d) the explicit
  cold-vs-warm-boot budget table (900 ms → 210 ms → 35 ms). Worth `scripts/research/`.

---

#### scripts/PERF2.md

- **Verdict:** RESEARCH — f16-vs-i16×10 and cache-tier sizing rationale; most of it shipped.
- **Was:** A chat transcript on data-oriented layout-node design. Rejects `f16` for layout
  storage (10-bit mantissa ⇒ ±1 px ULP at 1920 px; no native x86-64 f16 ALU — F16C is
  conversion-only, AVX-512 FP16 is server silicon) in favour of the existing `i16 ×10`
  encoding (uniform 0.1 px across ±3276.7 px, native integer arithmetic). Then argues the real
  win is a hot/warm/cold split of the 550 B `LayoutNode` down to a 64 B cache-line-exact hot
  struct, backed by a table of L1/L2/L3 sizes for RPi Zero 2W / RPi 4 / RPi 5 / Snapdragon,
  where the absence of L3 and the in-order Cortex-A53 make locality 2–3× more valuable than on
  desktop. Ends with a scheduling decision: do **not** refactor before the 800-agent
  spec-conformance run, because the split is mechanical while agents change semantics.
- **Landed:** The split shipped. `layout/src/solver3/layout_tree.rs` now exposes
  `LayoutNodeHot` / `LayoutNodeWarm` / `LayoutNodeCold` (used across
  `layout/src/managers/gpu_state.rs:333`, `managers/a11y.rs:436`, `headless.rs:1291`,
  `widgets/split_pane.rs:823`). The i16×10 packing shipped as
  `PackedBoxProps` — `layout/src/solver3/geometry.rs:473` with `pack_edge` and round-trip
  tests at `geometry.rs:1547-1562`; the doc-string at `geometry.rs:469` says verbatim "Only
  used for storage in `LayoutNodeHot`. The layout solver unpacks to [f32]" — exactly the
  storage-vs-computation split the transcript argued for. The tier2 hot/cold split also
  shipped: `css/src/compact_cache.rs:1162 CompactNodeProps` + `:1204 CompactNodePropsCold`
  (border colors/styles moved cold, as proposed). `children_arena`/`children_offsets` exist
  (`layout/src/hit_test.rs:358`), and `layout/src/window.rs:2558` reports
  `children_arena_bytes` under `[MEM]`.
- **Superseded by:** n/a for the layout-node half. The CSS-side half is the older
  `PERFORMANCE_AND_MEMORY_REPORT.md`; PERF2 is the later, winning iteration.
- **Still open:** (1) **Hierarchy flattening was never done.** `core/src/styled_dom.rs:643`
  `NodeHierarchyItem` is still `{parent, previous_sibling, next_sibling, last_child}` = 4 ×
  `usize` = 32 B/node; the proposed `depth: u16` + DFS-preorder encoding (−26 B/node) is
  absent. (2) The bump/arena allocator for per-frame `NodeData` sub-Vecs was never built.
- **Research value:** The f16 rejection is the durable piece — a reusable argument that fixed-
  point `i16 ×10` dominates half-precision for *any* pixel-geometry storage (uniform precision,
  conversion is 1–2 cycles not 4–5, no ISA gate). Second durable piece: the embedded cache
  table + "no L3 means every L2 miss is a 100 ns DRAM stall on an in-order A53", which is why a
  64 B cache-line-exact hot struct is a correctness-of-experience requirement, not a
  micro-optimisation. Third: the *scheduling* rule — never land a mechanical data-layout
  refactor immediately before a large parallel semantic-change campaign.

---

#### scripts/PERFORMANCE_AND_MEMORY_REPORT.md

- **Verdict:** DELETE — 5 of 7 fixes shipped; its premise (1520 B `CssProperty`) no longer holds.
- **Was:** Analysis of resize/scroll cost on `scrolling.c` with 500 rows. Headline finding:
  `CssProperty` was 1520 B because one variant, `Scrollbar(ScrollbarStyleValue)`, packed
  2 × `ScrollbarInfo` × 5 × `StyleBackgroundContent`; every CSS property in the engine paid for
  it. Seven ranked fixes: remove the compound variant in favour of flat per-part variants,
  `BoxOrStatic<T>` for the remaining large payloads, delete `tier3_overflow`, add
  `CompactInlineProps` + source-hash dedup of inline styles, a font-chain dirty flag,
  incremental display lists, and a width-only-resize skip. Projected 50 k nodes from ~1.08 GB
  to ~30 MB.
- **Landed:** Fix 1 fully — `CssProperty::Scrollbar` and `ScrollbarStyleValue` no longer exist
  anywhere (0 hits across `css/ core/ layout/`); the flat replacements are
  `css/src/props/property.rs:733-737` (`ScrollbarTrack/Thumb/Button/Corner/Resizer` taking
  `StyleBackgroundContentValue`), registered as `CssPropertyType` at `property.rs:988-992`,
  parsed via `-azul-scrollbar-*` at `property.rs:226-230`, named at `property.rs:1394`.
  Fix 2 landed — `BoxOrStatic<T>` at `css/src/css.rs:297` with the exact `Boxed`/`Static`
  shape, null-checked accessors (`css.rs:320-324`), a `Drop` that only frees `Boxed`
  (`css.rs:355`), and a documented mention in the module header at `css/src/css.rs:9`.
  Fix 3 landed — `tier3_overflow` has zero occurrences repo-wide.
- **Superseded by:** `PERF2.md` for the layout-node half of the memory story;
  `RELEASE_SIZE_MEMORY_AUDIT_2026_07_04.md` for the "where does RSS actually go" question
  (which found the real floor is 21 MB of boxed `glyf` + 37 MB of WebRender atlases, not CSS
  at all — azul's own tracked caches measured **39 KiB**, i.e. this report's whole subject is
  0.1 % of a running app's RSS).
- **Still open:** Fix 4 never happened — there is no `CompactInlineProps`, `InlineStyleKey` or
  `InlineStyleTable` in the tree, and `core/src/dom.rs:1551` still stores a full
  `pub style: azul_css::css::Css` per `NodeData`, so N identical inline style strings still
  produce N parses/allocations. Fix 5 landed in a different, better shape than proposed:
  there is no `font_chains_dirty: bool`, but `layout/src/window.rs:2222-2264` skips the
  resolver when a signature-keyed `font_chain_cache` is warm
  (`set_font_chain_cache_with_sig`). Fixes 6/7 explicitly deferred and still deferred.
- **Research value:** One transferable lesson, already absorbed: a single fat enum variant
  taxes every instance of the enum — measure `size_of` per variant before assuming storage
  cost tracks usage frequency. Not worth a standalone research file.

---

#### scripts/gemini_perf_response.md

- **Verdict:** DELETE — advice for the out-of-tree `git2pdf` tool; its azul-side ideas never landed.
- **Was:** A recorded consultation on a 4.1 s `git2pdf` run (12,481 nodes from
  syntax-highlighted Rust). Ranks: parallelise the per-file loop with rayon; stop emitting
  per-`<span>` inline `style=` and use CSS classes; **coalesce adjacent same-style inline runs
  into one shaping call** before handing text to `text3`/allsorts; adopt STARTUP_LATENCY.md's
  font plan; skip reconciliation on first layout; fast-path monospace shaping by reading cmap +
  hmtx directly instead of running the OpenType state machine.
- **Landed:** Nothing verifiable in azul. `rg -i coalesc` across `layout/src` returns only
  *event* coalescing (`event_determination.rs:440`, `scroll_timer.rs:1562`) and text-edit seam
  coalescing (`document_edit.rs:226`) — no text-run coalescence in the shaper or BFC. No
  `is_fixed_pitch` / monospace shaping fast path exists. `reconcile_and_invalidate`
  (`layout/src/solver3/cache.rs:~857`) has a viewport-resize branch but no "tree is None ⇒
  skip diffing entirely" fresh path. The rayon/CSS-class advice targets `git2pdf` and
  `printpdf`, neither of which is in this repo.
- **Superseded by:** `gemini_perf_response2.md` — the follow-up round, which is where the one
  idea that *did* ship (BTreeMap→Vec) was raised.
- **Still open:** Text-run coalescence remains a genuine, unimplemented layout optimisation:
  today N adjacent same-style inline boxes cost N shaping calls. It is the only claim here
  worth carrying forward, and it belongs in a layout doc, not this transcript.
- **Research value:** Low. The coalescence concept is standard browser-engine practice and is
  better recorded as one line in the layout roadmap than as 19 KB of chat.

---

#### scripts/gemini_perf_response2.md

- **Verdict:** DELETE — the one durable recommendation shipped; the rest is `git2pdf` scaffolding.
- **Was:** Second consultation round. Three asks: split layout from display-list generation so
  the paged path stops generating a throwaway display list; add a `compute_layout_fresh` that
  bypasses `reconcile_and_invalidate` for a never-before-seen DOM; and replace
  `BTreeMap<usize, LogicalPosition>` with a dense `Vec` because node indices are contiguous
  `0..N` and 12 k `O(log n)` lookups per pass dominate. Plus a `cfg!(debug_assertions)` guard
  on `debug_messages` and rayon over commits.
- **Landed:** The BTreeMap→Vec change shipped and is now a named type:
  `layout/src/window.rs:595` declares `pub calculated_positions: solver3::PositionVec`, and
  every construction site is a `Vec` (`window.rs:1015`, `window.rs:2056`,
  `hit_test.rs:362`, `default_actions.rs:592`, `cpurender/raster.rs:3296`,
  `managers/focus_cursor.rs:583`); the pass-through signature at `window.rs:2801` takes
  `&solver3::PositionVec`.
- **Superseded by:** n/a (it is itself the successor to `gemini_perf_response.md`).
- **Still open:** No `compute_layout_fresh` / initial-layout fast path exists; the double
  display-list generation claim was about `printpdf`'s paged path, not this repo, and is
  unverifiable here.
- **Research value:** None. "Dense integer keys ⇒ use a Vec, not a BTreeMap" needs no archive.

---

#### scripts/gemini1.md

- **Verdict:** DELETE — misfiled bug consult (layout/scrollbars, not perf); its real fix landed.
- **Was:** Not a performance document at all — an AI consultation on four `effects-showcase.c`
  bugs on macOS: a scrollbar stuck at 100 %, "corrupted" grid columns, HiDPI width overrun and
  opacity artefacts. Diagnoses all four as one root cause — the injected `<html>` root from the
  CSD titlebar path was unconstrained, so `<body>` grew to content height, `container_size ==
  content_size` and nothing could scroll; the `1fr` grid then split a ~3000 px container.
  Proposes: keep the overlay-scrollbar fix in `check_scrollbar_necessity` (do not early-return
  when `scrollbar_width_px == 0`, since macOS overlay scrollbars occupy 0 layout px but must
  still register scroll nodes), make the injected root `height:100%; overflow:hidden;
  display:flex`, and fix the C demo's CSS.
- **Landed:** The overlay-scrollbar half is in the code and the reasoning survives as a
  comment. `layout/src/solver3/fc.rs:8334` `check_scrollbar_necessity` has **no**
  `scrollbar_width_px <= 0.0` early return, and `fc.rs:8349-8351` reads "scrollbar_width_px
  can be 0 for overlay scrollbars (e.g. macOS), but we still need to register scroll nodes so
  that scrolling works". Tests exist at `fc.rs:10519`.
- **Superseded by:** The CSD half is moot — `inject_software_titlebar` no longer exists in
  `dll/src/desktop/csd.rs`; that whole code path was rewritten since.
- **Still open:** none.
- **Research value:** None as a document. The one durable rule is already captured in the
  source comment: zero-width overlay scrollbars must still create scroll nodes.

---

#### scripts/bloaty-analysis.md

- **Verdict:** DELETE — Feb-2026 snapshot, both "Done" items shipped, wholly superseded.
- **Was:** A 2026-02-26 bloaty run on a **debug-symbol-bearing** `libazul.dylib` (27.2 MB;
  42.5 % `__text`, plus 3.19 MB string table + 3.04 MB symtab that a stripped release does not
  ship). Attributes the largest anonymous constants: 2.77 MB ICU segmenter dictionaries,
  917 KB `encoding_rs` tables, ~950 KB of debug-server embedded HTML/JS, 499 KB ICU locale
  blob, ~600 KB `regex_automata` DFA/NFA tables compiled for four trivial patterns used only
  by `try_wlr_randr()`, 148 KB of `ring`'s P-256 table. Also lists raw `.rlib` sizes
  (`libazul_layout` 131 MB, `libmoxcms` 23 MB) with the caveat that rlib size is not binary
  contribution. Five ranked actions; two marked Done (ICU→`icu_macos`, regex removal), three
  TODO (drop webp/moxcms/pxfm, un-default `backtrace`, feature-gate the debug server).
- **Landed:** The `icu_macos` claim holds — `RELEASE_SIZE_MEMORY_AUDIT §2.10(c)` independently
  re-measured that macOS avoids most of the ICU blob. The debug server is now a real feature
  (`libazuldbg.so` ships as a separate artifact, +1.3–1.9 MB), which is the gate this file
  asked for. `ring` is no longer in azul's build at all — `Cargo.lock` lists `ring 0.17.14`
  only as an optional dep of `rustls-webpki`; the shipped TLS path is `rustls-rustcrypto`.
- **Superseded by:** `RELEASE_SIZE_MEMORY_AUDIT_2026_07_04.md`, comprehensively. That report
  measures the actual prod-release dylib (24.0 MiB, stripped, thin-LTO, cgu=1, panic=abort),
  does per-object crate attribution, and finds the real `__const` hogs are **hyphenation
  dictionaries (2.79 MiB) and ICU4X (≈9 MiB across four crates)** — an attribution this file
  missed because a debug-symbol build hid it behind symtab noise. The 27.2 MB headline and
  every `.rlib` number here are misleading.
- **Still open:** The webp→`moxcms`/`pxfm` question is still unanswered but is now a
  sub-case of the audit's core/full feature-split lever, which supersedes it.
- **Research value:** One methodological caution, worth keeping only as a sentence: never
  size-budget from a build with debug symbols, and never treat `.rlib` size as binary
  contribution — both mistakes are visible in this file's tables.

---

#### scripts/RELEASE_SIZE_MEMORY_AUDIT_2026_07_04.md

- **Verdict:** ACTIVE — top-5 actions shipped; ~8 quantified levers still unbuilt.
- **Was:** The definitive 0.2.0 forensic audit. Downloaded all 99 live release artifacts
  (3.40 GiB) plus finding 38 dead links, dissected them with bloaty/nm/otool and custom
  COFF/Mach-O parsers, and profiled RSS with `AZ_PROFILE=memory` + macOS
  `vmmap`/`footprint`/`malloc_history`. Two headline results. **Size:** ~2.4 GB of the 3.4 GB
  release is packaging bugs, not code — embedded ThinLTO bitcode is 56–89 % of every shipped
  archive (`libazul-ios-arm64.a` is 89 % `.llvmbc`), Android ships 179 MB of DWARF, a 245 MB
  `examples.zip` is 80 % duplicated demo binaries, and every APK carries an unstripped
  158.6 MB `.so`. **Memory:** a hello-world's 94 MB footprint is a *floor*, not a leak (100
  headless resize iterations: RSS flat), decomposing as ~37 MB WebRender atlases + **21.4 MB =
  one heap copy of the system font's entire `glyf` table** + 9.6 MB IOSurfaces; azul's own
  tracked caches are 39 KiB. The 1 GB video blowup is a confirmed unbounded leak: azul never
  constructs `DeleteImage`, so every decoded frame is registered permanently. Also self-
  corrects an earlier misdiagnosis (§2.10d: dist artifacts *are* `panic=abort`; the
  `_Unwind_Resume` import came from C++ deps).
- **Landed:** Action #1 shipped as `scripts/strip_staticlib.sh` — its header cites this
  report's §2.4 by name, it handles ELF/Mach-O whole-archive plus MSVC `.lib` member-wise via
  `scripts/strip_coff_lib.py`, and it **asserts** no `.llvmbc` survived. Wired at three CI
  sites: `.github/workflows/rust.yml:660`, `:1642-1648`, `:3154`. Action #4 shipped — the
  image GC exists at `dll/src/desktop/wr_translate2.rs:1747-1832` ("Image GC: DeleteImage for
  images gone from every display list", epoch-debounced per
  `core/src/resources.rs:1323 IMAGE_GC_KEEP_EPOCHS`, emitting
  `ResourceUpdate::DeleteImage` at `wr_translate2.rs:1826`), and the recommended
  pointer-keying fix landed too: `core/src/resources.rs:1178-1192` now mints a retired-forever
  monotonic id and converts it losslessly to an `ImageKey`. A regression test exists
  (`dll/tests/image_lifecycle.rs`). Action #5 shipped as lazy/evictable `LocaGlyf`
  (`layout/src/text3/default.rs:69-122`: load "deferred until the first glyph decode", plus a
  cache-eviction pass). Action #9 shipped — `dll/src/desktop/wr_translate2.rs:252-268`
  `azul_texture_cache_config()` with a comment explaining WebRender's default is
  browser-sized. Action #13 partially: CI now hard-fails on a missing `guide.pdf`
  (`rust.yml:4289-4298`), asserts every registry channel is populated
  (`rust.yml:4664-4699`), and actually runs `gem install` against the built tree
  (`rust.yml:4820-4832`) — closing the broken `/ui/gems` bug. The missing-execute-bit bug is
  also fixed (`rust.yml:4386-4413` `chmod +x` over demos and all `.so`/`.dylib`).
- **Superseded by:** n/a — it supersedes `bloaty-analysis.md`. Its companion
  `WEB_WASM_DIET_PLAN_2026_07_04.md` (not in this batch) covers the wasm half.
- **Still open, all quantified and none applied:**
  1. **Hyphenation dictionaries are still `embed_all` and still default.**
     `layout/Cargo.toml:64` `features = ["embed_all"]` and `:159` lists
     `text_layout_hyphenation` in `default`. That is 2.79 MiB of `.rodata` on *every*
     platform artifact, for ~70 languages nobody asked for. Single cheapest remaining lever.
  2. **ICU4X data is still baked in** (§2.10c: icu_segmenter 3.99 + icu_datetime 3.68 +
     icu_collator 1.06 + icu_properties 0.42 MiB of const data). No runtime
     `icu_provider_blob` loading. −3–6 MB per Linux/Windows artifact.
  3. **No core/full feature split.** The cdylib still can't dead-strip pdf/db/http/video
     because the export table roots them (AzSvg 208 fns, AzXml 146, AzVideo 102…).
     −8–12 MB per artifact, and the biggest single size lever left.
  4. `examples.zip` still bundles demo binaries — `rust.yml:4949` runs
     `zip -gr examples.zip demos`. The audit said sources+headers only (−240 MB); it went
     245 MB → ~115 MB (`rust.yml:4979` comment), so this is half-done at best.
  5. Toolchain levers untouched: `rg` finds **zero** occurrences of `--remap-path-prefix`,
     `thumbv7neon`, `--icf=all` or `--pack-dyn-relocs=relr` anywhere in the workflows or
     Cargo.toml. That is −2.4 MB (RELR x86_64, gated on a glibc-2.36 floor), −4–6 MB (armv7
     Thumb2 — armv7 `.text` is currently *larger* than aarch64), ~−1 MB (ICF), −200 KB (path
     remap) left on the table.
  6. 156 stray turso/SQLite exports (`_uuid`, `_time_parse`, `_register_*VTabModule`) still
     sit in the global dynamic namespace — a symbol-clash hazard for host apps, not just
     bytes.
  7. Cosmetic-but-shipping: deb `Maintainer: Unset Maintainer <unset@localhost>`;
     `guide.pdf` at 27 MB with 78 unsubsetted embedded fonts.
  8. The ~55 KB/iteration headless heap creep outside instrumented caches was never chased.
- **Research value:** Very high, and the best size document in the repo. Durable concepts:
  (a) **a cdylib cannot dead-strip what its export table roots** — for a C-API product, the
  only size lever is *feature composition*, not `--gc-sections`; the same audit measured a
  statically-linked app at 7.0 MiB `.text` vs 15.8 MiB for the dylib, quantifying that
  "API breadth tax" at ~9 MB. (b) **Post-build bitcode stripping preserves ThinLTO codegen**,
  whereas `embed-bitcode=no` would change it — a distinction most projects get wrong.
  (c) The whole audit is run under a self-imposed constraint — *no opt-level or codegen
  changes; every lever must be metadata, packaging, feature-gating or data placement* —
  which is a reusable discipline for size work on a performance-sensitive library.
  (d) The RSS decomposition method (footprint vs `ps` RSS vs the app's own accounting, with
  `malloc_history` backtraces) and its punchline that the framework's own caches were 0.1 %
  of the number users complain about. (e) §2.10d is a worked example of the repo's
  negative-control habit: an earlier confident diagnosis (panic=unwind) was overturned by
  looking for the *throw primitive* rather than `_Unwind_Resume`, and CI now asserts the
  corrected fact.

---

#### scripts/HTTPS_TLS_ANALYSIS.md

- **Verdict:** RESEARCH — the TLS-stack-choice rationale for a no-C-code GUI toolkit.
- **Was:** Started as a bug hunt: `examples/c/browser.c` failed on `https://example.com` with
  the pure-Rust `rustls-rustcrypto` provider. The document opens with the resolution — it is
  **not** a crypto bug. `example.com` is Cloudflare-served behind a cross-signed chain rooted
  at "AAA Certificate Services", which `webpki-roots 1.0.6` dropped when Mozilla removed it, so
  any provider yields `InvalidCertificate(UnknownIssuer)`; google.com, github.com and
  crates.io all succeed. The rest is the durable material: the full call chain from
  `AzHttpRequestConfig_httpGetDefault` down to the rustls handshake, the exact feature
  resolution (`rustls-no-provider` enables ureq's `_rustls` without `_ring`, so the provider
  *must* be set explicitly or ureq's fallback panics), the complete cipher-suite / key-exchange
  / signature-algorithm inventory of `rustls-rustcrypto 0.0.2-alpha`, and an honest
  capability gap table versus `ring` (missing P-521 and Ed448; no AES-NI assembly; alpha, not
  battle-tested). Four fix options are laid out ranging from patching the provider to
  per-platform native TLS.
- **Landed:** The stack the doc analysed is still exactly what ships.
  `layout/Cargo.toml:99` carries the intent as a comment — "HTTP client — uses pure-Rust TLS
  (no ring/aws-lc-rs C code)" — over `:100-103` `ureq 3.3` with
  `["rustls-no-provider","rustls-webpki-roots"]`, `rustls 0.23` with `["std","tls12","logging"]`,
  `rustls-rustcrypto 0.0.2-alpha`, `webpki-roots 1.0`; the feature is composed at
  `layout/Cargo.toml:288`. `make_agent` at `layout/src/http.rs:418-441` still calls
  `.unversioned_rustls_crypto_provider(Arc::new(rustls_rustcrypto::provider()))` exactly as
  documented — plus a `disable_tls_cert_verification` escape hatch added since (`http.rs:427`)
  that the doc did not have. `ring 0.17.14` appears in `Cargo.lock` only as an optional dep of
  `rustls-webpki`; it is not in azul's build, confirming the doc's `cargo tree` claim.
- **Superseded by:** n/a.
- **Still open:** None blocking. The unclosed question is a *policy* one the doc surfaces but
  does not decide: azul ships a **0.0.2-alpha** crypto provider as the only TLS path for a
  published GUI library. The named gaps (no P-521, no Ed448, no AES-NI) and the "not
  battle-tested" line are still true, and nothing in the tree revisits Option B (per-platform
  native TLS) or Option D (vendor + patch). Worth a conscious re-affirm at the next release.
- **Research value:** High and squarely in the brief's named category. Transferable content:
  (1) the reasoning for choosing pure-Rust crypto over `ring`/`aws-lc-rs` in a
  **cross-compiled, C-free** toolkit — no C toolchain per target, which is what makes the
  7-architecture Linux matrix and the wasm lift possible at all; (2) the exact price of that
  choice, itemised rather than hand-waved; (3) the generalisable debugging lesson —
  a TLS failure against one host is far more often a **trust-store** change than a
  crypto-provider bug, and the differential test (google/github/crates.io vs the failing host)
  separates the two in one minute; (4) the ureq/rustls feature-gate trap where
  `rustls-no-provider` produces a config that compiles and then panics at runtime unless the
  provider is installed explicitly.

---

#### scripts/PACKAGE_DISTRIBUTION_PLAN.md

- **Verdict:** RESEARCH — the channel-vs-artifact distribution model; largely built, `/dl` still absent.
- **Was:** 2026-05-30 plan for how azul ships native packages. Its organising idea is the
  separation of **channel** (the stable, version-*free* URL a user configures once, whose
  metadata the updater re-reads — this is how "an update is available" is discovered) from
  **artifact** (the versioned file, reached *through* that metadata). Consequence: multi-product
  shipping needs no new infrastructure, just more packages inside one channel, exactly as
  Debian and Homebrew do. Includes a per-manager table of whether metadata may point at an
  absolute off-repo URL (brew/choco/PyPI/npm yes; apt/pacman/apk no — relative to the
  configured base; dnf yes-ish via `xml:base`), and the key realisation that this is not a
  conflict: package-managed files are a few MB and Pages-servable, while the genuinely huge
  artifacts (200 MB+ `.a`) are never package-managed anyway. Also stages an azul→azlin brand
  rename that keeps the `Az` C-symbol prefix untouched, and a P0 warning that the shipped
  0.2.0 metadata used version-pinned `azul.rs/release/0.2.0/...` URLs that vanish on the next
  deploy.
- **Landed:** Most of it. `scripts/build_registry_mirrors.sh` now implements the full channel
  set under a `/ui` product namespace (header, lines 5–15): maven, PyPI PEP-503, npm, NuGet,
  RubyGems, dnf/yum/zypper, **pacman** and **Alpine apk** — the two P1 managers this plan
  added. `build_pacman()` at `:632-643` runs `repo-add azlin.db.tar.gz`; `build_apk()` at
  `:656-661` runs `apk index -o APKINDEX.tar.gz`; both document the exact
  `/etc/pacman.conf` and `/etc/apk/repositories` lines the plan asked for. The **azlin**
  package naming shipped in those docs (`pacman -Sy azlin-ui` at `:628`,
  `apk add azlin-ui` at `:650`). The script's UPDATE MODEL header (lines 17–27) restates the
  plan's channel/artifact invariant verbatim as the contract. The brand staging reached the
  website: `doc/templates/azlin-index.template.html`, `azlin-ws.html`, `azlin-os.html` exist
  (the plan's §D product cards), and the `/ui` route move happened.
- **Superseded by:** n/a — no later packaging plan.
- **Still open:**
  1. **The `/dl` indirection was never built.** `rg '/dl/|dl_base|--dl-base'` over
     `build_registry_mirrors.sh` and `rust.yml` returns nothing. The choco install script
     still emits `-Url64bit 'https://azul.rs/ui/release/{V}/azul.dll'`
     (`build_registry_mirrors.sh:581`) and `RELDIR` is `$SITE/ui/release/$V` (`:53`) — i.e.
     the exact version-pinned form the plan's P0 flagged as fragile is still what ships,
     three months on. Neither the `--dl-base` flag nor the Cloudflare redirect exists.
  2. Multi-product parameterisation (P2) is unbuilt: the script hardcodes `ui`; there is no
     `<product>` argument and no `ws` metapackage `.deb`/formula. `azlin-ws.html` /
     `azlin-os.html` are marketing pages for products that do not yet have packages.
  3. The rename (P3) is half-staged: package ids inside pacman/apk say `azlin-ui` while the
     artifacts are still `libazul.so`/`azul.dll` and the domain is still azul.rs. No
     `libazlin.so` transition symlinks, no `ghcr.io/.../azlin` alias.
  4. §6's four maintainer questions (one tap vs tap-per-product, CDN availability, rename
     timing, crates.io) are unanswered in-tree.
- **Research value:** High, and the brief names this category. The durable idea is the
  **channel/artifact separation** and its corollary — *the version must never appear in the
  channel URL* — which is what makes `apt upgrade` / `brew upgrade` work at all and is the
  single most common thing homegrown distribution schemes get wrong. Second durable artefact:
  the per-manager absolute-URL capability matrix (which metadata formats can reference an
  off-repo artifact and which are base-relative), which determines whether you can host bytes
  on a release service or must serve them from the repo root. Third: the "rename the brand,
  freeze the ABI symbols" staging discipline — keep the `Az` prefix and the `libazul.*`
  filenames through a brand change, ship aliases, and flip the domain last.

---

#### scripts/WEBSITE_REDESIGN_PLAN.md

- **Verdict:** DELETE — superseded twice over; the shipped design is not the one planned here.
- **Was:** 2026-05-30 plan to port the design system of a separate `erp-site` project into
  azul's Rust-generated docs site, recoloured to azul blue/gold. Specifies the fonts to adopt
  (Playfair Display for display, Rubik for body), a token recolour map (`--color-accent`
  `#8b3a62` → `#004e92`, gold `#facb26`, the `135deg #000428 → #004e92 → #0084ff` hero
  gradient), a component list to port, a new marketing landing page with three product cards
  (UI real, Workspace and OS "coming soon"), the route move of the current docs home to
  `/ui`, the exact `doc/src/docgen` + `deploy.rs copy_static_assets()` seams to touch, and — in
  §G — a proposal to clean up the ~68 dated session-log markdown files then cluttering
  `scripts/`. Ends with a §H note explaining it is a plan rather than a commit because the
  tool channel degraded mid-task.
- **Landed:** The *structure* landed, the *design* did not. `/ui` route move: done
  (`doc/src/main.rs:1926-1945` emits `ui-landing.css` and reads
  `azlin-index.template.html`; `doc/src/docgen/mod.rs:830-847` documents the landing/docs CSS
  layering). Product cards: `doc/templates/azlin-ws.html` and `azlin-os.html` exist. But the
  font plan was reversed — `doc/fonts/` contains Imbue, Instrument Serif, Red Hat Display, Red
  Hat Mono and Source Serif Pro, and **no Playfair Display and no Rubik**; the shipped design
  system is `n.css` + `azul-docs.css` + `ui-landing.css`, not a port of erp-site's
  `styles.css`.
- **Superseded by:** `WEBSITE_ROUND2.md`, then `WEBSITE_ROUND4.md` (which explicitly drops
  Playfair "entirely"), and finally by the `n.css`/azlin design system actually in
  `doc/templates/`.
- **Still open:** Only §G — and it is *this audit*. `scripts/*.md` has grown from the ~68
  files it complained about to **169 files / 5.3 MB**. Its load-bearing list is still
  accurate and worth honouring: `analyze_coverage.py`, `build_registry_mirrors.sh`,
  `build-android.sh`, `build-ios.sh`, `check_dep_justifications.py`, `coverage.sh`,
  `dependency-justifications.toml`, the `docs_pdf_*`/`docs_to_pdf.sh` set,
  `e2e_language_matrix.sh`, `screenshot_single.sh` are all referenced by `rust.yml`.
- **Research value:** None. One operational rule worth keeping elsewhere, from §F: moving
  files under `doc/templates/` is the riskiest edit in the docs build because
  `include_str!` and `copy_static_assets()` paths are hardcoded — move one at a time with a
  `cargo check` between. That belongs in a docgen README, not an archived plan.

---

#### scripts/WEBSITE_ROUND2.md

- **Verdict:** DELETE — a one-shot review checklist; executed then partly reversed by Round 4.
- **Was:** A numbered list of user review feedback on a local build (2026-05-30), deliberately
  exhaustive ("make a list and don't drop any requirements"). Fonts F1–F5: Playfair isn't
  actually loading, restore Instrument Serif for subtitles, adopt Red Hat Mono for all
  monospace. Dark mode D1–D3: move the floating "Search guide" overlay into the page as a
  column, and scope guide search to the guide *overview* while individual guide pages get an
  API search expanded on load from frontmatter keys. Index I1–I7: right-align the version
  block, redesign the search bar (no magnifier, no shadow, `/` hint on the left), drop the
  "more languages" expander in favour of all 11 tabs, rename the node `displayName` to "JS".
  Includes a verified seams section naming the exact functions and line ranges.
- **Landed:** Fonts: Red Hat Mono and Instrument Serif are both in `doc/fonts/`; Playfair is
  gone (per Round 4). D1/D2: the search column exists —
  `doc/templates/main.css:1047-1050` `.guide-search-col` is a sticky flex column with a
  `--fade-bg` token and a dark-mode override. I6: `api.json:2206` node `displayName` is `"JS"`.
- **Superseded by:** `WEBSITE_ROUND4.md`, which explicitly reverses F1/F3 (Playfair dropped
  for Imbue) and flags I6 as having broken JS syntax highlighting.
- **Still open:** I5 was not done as written — `PRIMARY_LANGUAGES` still exists
  (`doc/src/docgen/mod.rs:130`) and the `.lang-more` overflow group is still styled
  (`doc/templates/ui-landing.css:264-283`). This is arguably moot rather than open: the
  language set has since grown well past 11 (`mod.rs:128` names pascal, scala, fortran,
  haskell in the overflow), so "list all of them as tabs" no longer makes sense.
- **Research value:** None — pure per-build review feedback.

---

#### scripts/WEBSITE_ROUND4.md

- **Verdict:** DELETE — the last review round; its items are in the shipped CSS.
- **Was:** Six items captured after the Round 2 font + D1/D2 work landed, explicitly held from
  blind execution because the tool channel had stalled. R4-1: JS code blocks lost Prism
  highlighting after the `displayName` "JavaScript"→"JS" rename, because the Prism class was
  derived from the display name rather than the canonical grammar id. R4-2: replace Playfair
  with Imbue for big headings and drop Playfair entirely. R4-3: bolder default Imbue weight.
  R4-4: bold search placeholder, flexbox the release box, confirm guide pages say "Search API".
  R4-5: square-cornered, higher-contrast, thicker-bordered search bar. R4-6: the guide search
  column must not push the h1 down, and must become a bottom-anchored **overlay** with a
  gradient fade and an upward-opening results panel when the viewport is too narrow.
- **Landed:** R4-2 confirmed — `doc/fonts/` contains `Imbue-VariableFont_opsz,wght.ttf`
  and no `PlayfairDisplay-*`. R4-5 confirmed verbatim —
  `doc/templates/azul-search.css:404-405` sets `border-radius: 0; border-width: 2px` on both
  `.azs-inline-row` and `.azs-panel-inline`. R4-6 confirmed and is the most elaborate: a
  narrow-viewport media query at `doc/templates/main.css:1054-1068` repositions
  `.guide-search-col`, adds the `::before` fade element (`:1063`) and flips the panel upward
  with `top: auto; bottom: calc(100% + 6px)` (`:1068`) — exactly as specified.
- **Superseded by:** n/a — it is the last round; superseded only by the shipped CSS.
- **Still open:** R4-1 is unverifiable and possibly moot. Prism is loaded via the CDN
  autoloader (`doc/src/docgen/mod.rs:674-676`), API doc pages emit no `class="language-*"` at
  all, and guide fences take the bare markdown language name with a documented comma-strip
  (`doc/src/docgen/guide.rs:36-43`) — so no `displayName`-derived Prism class appears to exist
  any more. Worth one eyeball on a built `/api` page before assuming it is fixed.
- **Research value:** None — per-build review feedback. The only reusable note is the
  operational one in its "Notes / state": for large blind edits on a degraded tool channel,
  splice via a script with `assert count == 1` per edit and write only if **all** matches
  succeed (atomic-or-nothing).

---

### Tally

| Verdict | Count | Files |
|---|---|---|
| DELETE | 8 | bloaty-analysis, PERFORMANCE_AND_MEMORY_REPORT, gemini_perf_response, gemini_perf_response2, gemini1, WEBSITE_REDESIGN_PLAN, WEBSITE_ROUND2, WEBSITE_ROUND4 |
| RESEARCH | 4 | STARTUP_LATENCY, PERF2, HTTPS_TLS_ANALYSIS, PACKAGE_DISTRIBUTION_PLAN |
| ACTIVE | 1 | RELEASE_SIZE_MEMORY_AUDIT_2026_07_04 |
| ARCHIVE | 0 | — |

### Cross-cutting notes

- **Supersession chains resolved.** Perf: `PERFORMANCE_AND_MEMORY_REPORT` (CSS enum bloat) →
  `PERF2` (layout-node cache tiers) is the winning line; both shipped, `PERF2`'s framing won
  because it produced the `LayoutNodeHot`/`PackedBoxProps` architecture still in the tree. The
  two `gemini_perf_response*` files sit outside that chain — they advise an out-of-tree
  `git2pdf`, and only their BTreeMap→Vec item reached azul. Size: `bloaty-analysis` (Feb,
  debug-symbol build, wrong culprits) → `RELEASE_SIZE_MEMORY_AUDIT` (Jul, stripped prod build,
  correct culprits) — the July report wins outright. Website:
  `WEBSITE_REDESIGN_PLAN` → `ROUND2` → `ROUND4`, with Round 4 reversing Round 2's central
  font decision; none of the three describes the design that actually shipped (`n.css` +
  azlin), so all three are historical.
- **On the `gemini*.md` files.** `gemini_perf_response2` earned its keep (one landed change);
  `gemini_perf_response` and `gemini1` are now noise — `gemini1` is not even a perf document,
  it is a scrollbar/grid bug consult whose one surviving conclusion is already a source
  comment at `layout/src/solver3/fc.rs:8349`. Judged on content, all three are consultation
  transcripts whose durable output has been absorbed into code or into the two real perf docs.
- **Coverage of the known 0.2.0 incidents.** The missing execute bit and the broken `/ui/gems`
  page are *fixed and gated* (`rust.yml:4386-4413`, `:4664-4832`) — but note that neither
  packaging doc in this batch predicted them; `RELEASE_SIZE_MEMORY_AUDIT §1.2` caught the
  adjacent dead-link class. The 1 GB overrun is addressed only partially (bitcode stripping
  landed; `examples.zip` is still ~115 MB with demo binaries). The **sparse-checkout that
  omitted `scripts/`** post-dates every document here — `rust.yml:3899-3916` now carries a
  long comment and an explicit `scripts/build_registry_mirrors.sh` entry in the
  sparse-checkout list, which is the fix, not a doc.


## Part 16 — managers, overlay/journal, research briefs

Verified against the working tree at `master` (post `f1c43ba60`), 2026-08-01.
Every "Landed" bullet was re-derived with `rg` against current sources; no doc
status line was trusted.

---

#### scripts/MANAGER_FIX_PROGRESS.md

- **Verdict:** ARCHIVE — completed fix-arc control file; every item shipped, log has no forward value.
- **Was:** The 164-line control/journal file for the 2026-07-03 "manager wiring fix arc" (branch `fix/manager-wiring`). Contains the binding tick rules (no compilation in phases A–C, one commit per item, lock-file heartbeat, "NO NEW THREADS"), 10 confirmed user DECISIONS (D1–D10), the A/B/C/D phase checklists with a paragraph of implementation detail per item, Phase D compile-gate results, an API-STAGED list, a FOLLOW-UPS list and a per-tick session log. STATUS line claims DONE (40/40 items).
- **Landed:** Verified by symbol, not by line: `capability_pump` (5 files), `CAPABILITY_PUMP_TIMER_ID`, `LONG_PRESS_TIMER_ID`, `primary_down`, `wl_data_source`, `csd_resize_edge_at`, `_MOTIF_WM_HINTS`, `begin_net_wm_moveresize`, `build_edit_submenu`, `performWindowDragWithEvent`, `map_action_to_accesskit`, `ILatLongReport`, `collect_tab_order`, `apply_preedit_to_text_cache` (6 files), `refresh_scrollbar_gpu_cache_for_cpu_frame`, `set_root_window_bounds`, `has_queued_requests`, `handle_file_drag_moved`, `set_reason_override`, `reinstate_undo` — all present. `layout/src/managers/permission.rs:192` (`PermissionManager`), `core/src/dom.rs:649` (`NodeType::GeolocationProbe`). The STATUS: DONE line is, unusually, accurate.
- **Superseded by:** n/a (its FOLLOW-UPS list is the only live remnant).
- **Still open (all from its own FOLLOW-UPS, re-verified today):**
  - Wacom tablet-pad: `dll/src/desktop/extra/wacom_pad/` still does not exist; `update_pad_state` has only its definition + one unit test as callers (`layout/src/managers/gesture.rs:959`, `:3112`) — `get_pad_state` returns `None` on every OS.
  - OS drag-SOURCE: zero hits for `NSDraggingSource` / `DoDragDrop` / `beginDraggingSession` repo-wide.
  - Gamepad rumble: no `set_rumble` / force-feedback anywhere.
  - WinRT `Geolocator.PositionChanged`: only mentioned in doc comments (`dll/src/desktop/extra/geolocation/mod.rs:10-12`); the Windows provider is still the classic-COM 1 s poll.
  - iOS `GCController` gamepad: `dll/src/desktop/extra/gamepad/apple.rs:3` still future-tense ("Will enumerate `GCController.controllers()`").
  - ListView/TableView virtualization: `ListView::visible_row_range` (`layout/src/widgets/list_view.rs:1559`) still has only test callers.
  - X11 XI2 smooth-scroll valuators: zero hits for `XIScrollClass`.
  - Android sensors: `AzulSensors.java` still does not exist anywhere in the tree.
  - The whole `NEEDS-RUNTIME-VERIFY` per-OS smoke list (Phase D §5) was handed off and never ticked — it is the same real-hardware gap the 07-31 seam audit inherited.
- **Research value:** none as content. One transferable *process* idea worth keeping in a methodology note, not here: the "no compilation in phases A–C, single compile gate at the end" arc produced exactly ONE compile error across 40 items (its own Phase D §1) — evidence that symbol-located, audit-driven editing scales without a compiler in the loop.

---

#### scripts/MANAGER_WIRING_AUDIT_2026_07_03.md

- **Verdict:** ARCHIVE — the checklist its companion consumed; superseded by the code it produced.
- **Was:** The 611-line source audit behind the fix arc: 21 parallel auditors, one per manager (`gesture … csd`), each producing a capability × backend matrix (macOS/Windows/X11/Wayland/Headless) plus ranked gaps, then an adversarial verification pass (36 CONFIRMED / 1 ADJUSTED / 1 REFUTED). §2 scoreboard, §3 the five user-named scenarios, §4 top-15 repo-wide gaps, §5 twenty-one per-manager sections with `file:line` citations, §6 a REFUTED appendix.
- **Landed:** All 15 top-ranked gaps are fixed — sampled and confirmed above under MANAGER_FIX_PROGRESS (primary-modifier shortcuts, Windows generic-VK mapping, UIA `raise()`, AT-SPI focus state, incremental `:hover` restyle, horizontal wheel, nested-scroll handoff, native Wayland clipboard, pinch/rotate bridge, `WindowFocusIn/Out`, gamepad pump, `_MOTIF_WM_HINTS`, Wayland decoration negotiation, real permission backends, Windows geolocation).
- **Superseded by:** `scripts/MANAGER_FIX_PROGRESS.md` (which is itself superseded by the code).
- **Still open:** Two audit-flagged fragilities the fix arc explicitly did NOT close:
  1. §6 REFUTED-appendix caveat — macOS CoreLocation is linked only *transitively* through `objc2-core-motion`; no explicit `cargo:rustc-link-lib=framework=CoreLocation` and no dlopen exists (`rg CoreLocation dll/Cargo.toml dll/build.rs` → nothing). Drop the motion dep and macOS geolocation silently dies.
  2. §5.20 note — the Android sensor JNI Rust half is complete but `AzulSensors.java` still does not exist, i.e. a documented dead end that ships.
  Everything else is on the FOLLOW-UPS list above.
- **Research value:** low as content, HIGH as method. The durable artifact is the audit *shape*: a capability × backend matrix per subsystem with WIRED / PARTIAL / MISSING cells and a `file:line` for every cell, plus a separate adversarial pass that must try to REFUTE each critical claim (it refuted one and rescoped another — a ~5% false-positive rate that is only visible because the pass existed). Worth extracting the two-page method description into `scripts/research/` if any audit template is kept at all; the 611 lines of 2026-07-03 matrices are not.

---

#### scripts/CORE_AUDIT_2026_07_08.md

- **Verdict:** ARCHIVE — implemented by its own commit; ~1 finding demonstrably still open.
- **Was:** A 77-line read-only bug audit of `azul-core` by 5 parallel scans (unsafe/FFI/refcount, resources/caching, dom/diff/id, parsers, events/hit-test/geom): ~70 findings severity-ranked 🔴/🟠/🟡/⚪ with `file:line`, trigger and fix sketch. Also carries a "Verified OK (do not fix)" section — the negative results, which is the rarer half.
- **Landed:** Committed as `6ae9cd233 fix(core): implement core-crate audit`. Spot-verified today: `core/src/id.rs:299-304` now early-returns on an empty hierarchy with an `// AUDIT:` comment; `core/src/refany.rs:1131` uses `compare_exchange` in `downcast_mut` (and `:664` documents the fix); `core/src/gl.rs:983` gates the `Box` free on `run_destructor`, `:3350` narrates the double-`delete_textures` fix; `core/src/gl.rs:828` replaced `static mut … as_mut()` with a `&raw mut` accessor; `core/src/resources.rs:1865-1867` `normalize_u16` is the correct `(i/u16::MAX)*u8::MAX` with two regression tests (`:3378`, `:4117`); `core/src/transform.rs:858` documents the `&__m128` alignment fix.
- **Superseded by:** n/a.
- **Still open (verified):**
  - The 🟠 **font/font-instance GC leak** is NOT fixed. `remove_font_families_with_zero_references` (`core/src/resources.rs:1472`) still has only two *test* callers (`:4932`, `:4943`), and `core/src/resources.rs:1448-1462` now carries an explicit comment admitting "helper itself has no callers. No `DeleteFont`/`DeleteFontInstance` …" plus a 3-step sketch for a `garbage_collect_fonts`. Font cycling still leaks WebRender font memory unboundedly. This is the single concrete leftover.
- **Research value:** none transferable (findings are azul-specific). The methodology bullet — "publish the *Verified OK, do not fix* list alongside the findings" — is worth one line in a methodology note; it is what stops the next audit from re-litigating `ImageRef`'s refcount unsafe.

---

#### scripts/OVERLAY_JOURNAL_REFACTOR_PLAN.md

- **Verdict:** RESEARCH — the single-authority content model is the design rationale for shipped code.
- **Was:** The 250-line design for the content-overlay refactor (2026-07-31, + a 2026-07-31 amendment). §1 states the refactor as the bug classes it deletes (9 shipped bugs, each traced to "many overlays × many write paths × many read paths"); §3 separates `ContentJournal` (frame-scoped, renderer-facing, retention bounded by swapchain depth) from `UndoRedoManager` (user-intent, unbounded by frames) — *"journal = what the RENDERER may still need; undo = what the USER may still want"*; §4 the single write chokepoint `apply_content_change`; §5 the single read resolver `ResolvedContent`; §6 the "fake structural edit" split overlay (`NodeId → {Gen2a, Gen2b}`); §7 image retention ≤ N frames; §8 the O1–O4 staging table; §9 four enforcement mechanisms; §10 four open questions. The amendment replaces the text-shaped structural vocabulary with tree ops (`NodePosition`, `Dom`-fragment payloads, `SplitNode/MergeNodes/InsertChildren/RemoveChildren/ReplaceChildren`).
- **Landed (O1–O3 essentially complete):**
  - `layout/src/overlay.rs` (42 KB) exists: `ContentChange` (`:92`), `ContentDirtyTier` + `to_process_event_result` (`:140`+), `OverlayPartId::mint` (`:187`), `ContentOverlay` with the three private arms `images` / `text` / `pending_structure` (`:262-276`), `ResolvedContent` (`:500`) with `children_for_node` (`:555`) and `text_for_node` (`:656`), `JournalEntry`/`AppliedChange` (`:674-705`), `ContentJournal` (`:714`) with `begin_frame` retirement and `image_as_of` (`:774`) plus its own retention unit test (`:847`).
  - Chokepoint: `LayoutWindow::apply_content_change` at `layout/src/window.rs:2913` with per-arm helpers (`:2989` NodeCss, `:3050` node-image, `:3216` callback-result). Shell delegations at `dll/src/desktop/shell2/common/event.rs:2157, 2197, 2214, 2231, 2320, 2333`.
  - Deletions confirmed: `cpu_image_callback_results` and `with_image_callback_results` survive only as prose in `layout/src/window.rs:3132` and `layout/src/overlay.rs:8`; the `ChangeNodeImage` `set_node_type` mutation is gone.
  - Enforcement §9.1 SHIPPED as a real CI job: `.github/workflows/rust.yml:88-116` "Content-state architecture lint" greps `dll/src/desktop/shell2` (excluding `/common/`) for `set_node_type|image_cache|dirty_text_nodes|cpu_image_callback_results` and fails the build. It even cites this plan file by path at `:96` — so deleting the doc breaks a comment reference.
  - Enforcement §9.4 partially: `e2e/op-image-swap-repaints.json` exists.
  - O3: `layout/src/document_edit.rs` (1077 lines), `StructuralPreview` referenced from `layout/src/{window,overlay,document_edit,managers/changeset,managers/undo_redo,solver3/layout_tree}.rs` + `layout/tests/contenteditable_e2e.rs`; `LayoutWindow::record_structural_default_action` (`window.rs:1408`), `undo_structural_edit` (`:1744`), `redo_structural_edit` (`:1767`).
  - MS-Word page model: `layout/src/solver3/pagination.rs` (2029 lines, `PageSequence` at `:694`), `page_breaks.rs` (1478), `paged_layout.rs` (1944).
- **Superseded by:** its own amendment for §6 (text ops → tree ops); otherwise n/a.
- **Still open (concrete):**
  1. **§10 GPU epoch** — undecided and unimplemented: `dll/src/desktop/wr_translate2.rs:1909, 1921, 2696, 3095, 3107` all still build `webrender::api::Epoch(layout_window.epoch.into_u32())`; no journal entry carries an epoch.
  2. **§4/§5 no `Text` or `Structural` arm on `ContentChange`** — the enum (`overlay.rs:92-138`) is `Image / ImageCallbackResult / ImageById / NodeCss / ImageMask`. Text still enters via `update_text_cache_after_edit`, structure via `record_structural_default_action`. Two of the five planned chokepoint arms bypass the chokepoint; `AppliedChange` (`:681`) correspondingly journals only `Image / ImageById / ManagerState` — **text and structural edits are not journaled at all**.
  3. **In-place `StyledDom` mutation is NOT fully deleted** — `dll/src/desktop/shell2/common/event.rs:2095` (`ChangeNodeText` → `set_node_type(NodeType::Text(...))`) and `:2960` (`DeleteNode` tombstone → `set_node_type(NodeType::Div)`) still mutate the immutable DOM in place, exactly the §1 row the plan called a rule violation. Mirrored in the e2e runner (`layout/src/e2e/runner.rs:1373`, `:1593`).
  4. **§9.4 enforcement incomplete** — `manager_fingerprints` in `layout/src/e2e/full.rs` has rows for scroll/hover/focus/gesture/text_edit/text_input/undo_redo/virtual_view/gpu_state/permission/clipboard/file_drop/gamepad/geolocation/biometric/keyring/sensors — **no `overlay` and no `journal` row**, so `assert_only_managers_changed` cannot see overlay drift. No `noninterference-overlay-*` scenario exists in `e2e/`.
  5. **O4 partially** — the `NodeCss` arm landed, but journal-driven damage did not: `dll/src/desktop/shell2/headless/mod.rs:196, 414, 545, 790` still retain and diff `previous_display_list`.
  6. §10 `ImageById` per-DOM scoping still deliberately global; §10 `OverlayPartId` vs `az_children` a11y ordering never prototyped.
- **Research value:** HIGH — this is the keeper. Transferable concepts, none azul-specific: (a) **immutable tree + typed overlay + one write chokepoint + one read resolver**, with the chokepoint returning a *dirty tier* as the only thing platform code learns; (b) **two histories, two clocks** — a frame-scoped journal bounded by swapchain depth for the renderer vs an unbounded user-intent undo stack, fed by the same chokepoint with an `Undoability` flag decided by the change constructor; (c) **architectural enforcement as a grep CI job** — the plan predicted the lint and the lint shipped, which is the rare case of an enforcement clause surviving into infrastructure; (d) **structural preview as a recorded delta** (`Existing | ExistingTextSlice | Pending` child resolution) — the immutable-DOM equivalent of `insertChild()` being visible before the app re-renders. Move to `scripts/research/` **with a status header** recording items 1–6 above, and note that `.github/workflows/rust.yml:96` references its path.

---

#### scripts/AZMEET_TRANSPORT_DESIGN.md

- **Verdict:** RESEARCH — durable transport comparison + an original codec/layout insight; unimplemented.
- **Was:** A 358-line design report (2026-07-07, self-labelled "research + recommendation") answering "can iroh be AzMeet's transport, including in browsers?". §1 executive summary (yes, but a browser peer can never hole-punch — it is always relayed); §2 WebRTC vs WebTransport vs iroh with version-dated crate findings (`str0m` 0.21, `webrtc-rs` 0.17, `wtransport`, `web-transport-quinn`, iroh 1.0/noq fork, `iroh-live`); §3 the no-UDP question; §4 comparison table; §5 four architecture options (A WebRTC-everywhere / B iroh + WebTransport bridge / C MoQ / D pluggable transport trait — D is the recommended posture); §6 the media plane ("transport moves bytes; you still need codecs + AEC"); §6.5 the original section; §7 recommendation; §8 sources.
- **Landed:** Almost nothing beyond the API shape. `dll/src/desktop/extra/webtransport/mod.rs` (429 lines) defines the C-ABI handle, `WtReliability { ReliableOrdered, ReliableUnordered, Datagram }` and `WtEvent`, but its own header (`:12`) says *"v1 ships a stub/loopback engine (echoes the caller's own sends back as a synthetic peer, id 999)"*, and `:387` is the loopback engine. `examples/azul-meet/Cargo.toml` description states verbatim: *"Sending media to remote peers over the wire is the WebTransport follow-up."* Repo-wide `rg iroh|str0m|moq|webrtc` over all `*.toml`/`*.rs` → **zero hits**. The `web-transport-quinn` engine behind a `webtransport-native` feature does not exist; neither does the referenced `doc/webtransport-plan.md`.
- **Superseded by:** n/a.
- **Still open:** the entire media plane. No real transport engine; no codec integration; no AEC; no relay/server; §5's transport trait (option D) is not expressed in the code (the handle is concrete, not pluggable); azul-meet remains local-tiles-only.
- **Research value:** HIGH, and the value is concentrated in **§6.5 "Layout-driven resolution + source-specific codec"** — an insight that is *specific to a retained-layout GUI toolkit* and does not appear in the conferencing literature: because every video tile is a DOM node with a computed `LogicalRect`, **the layout engine is the rate controller**. Received bandwidth becomes bounded by local screen pixels rather than participant count (a 10×10 grid requests 160×90 per tile), and it re-adapts for free on window resize / join because that is just a relayout. Plus the supporting rules: codec mode forks by *source* (screen-share = P-frames at native resolution, low fps; camera = short-GOP/all-intra following tile size; audio protected last), LTR (long-term reference) frames for loss resilience without per-frame keyframes with the honest caveat that quality sags toward the end of each LTR interval, and QoS priority audio > screenshare > camera as a transport rule not an encoder setting. Keep in `scripts/research/`; add a status header noting the engine is still a loopback stub.

---

#### scripts/research/01_camera_screen_capture.md

- **Verdict:** RESEARCH — keep in place; largely consumed, but the per-OS API inventory is still the map for the missing backends.
- **Was:** A 467-line research-only inventory (2026-05-19) of camera capture and screen sharing across iOS/Android/macOS/Linux/Windows: §A camera per platform (AVFoundation, CameraX/Camera2, PipeWire/V4L2, WinRT MediaCapture) with entry points, permission strings and pixel formats; §B screen sharing (ReplayKit, MediaProjection, ScreenCaptureKit, xdg-desktop-portal ScreenCast/PipeWire, Windows.Graphics.Capture); §C a 10-part azul integration sketch (new `NodeType`, new `camera.rs`/`screen_capture.rs` managers, CallbackInfo accessors, EventFilter variants, permission API, injection points, W3C equivalents, risks, api.json impact, per-platform cost); §D recommended order.
- **Landed:** Shipped as *widgets*, not managers: `layout/src/widgets/camera.rs`, `screencap.rs`, `video.rs`, `microphone.rs`, `capture_common.rs`. Backends: `dll/src/desktop/extra/camera/{v4l2.rs, avfoundation.rs, avf_auth.rs, windows.rs, android.rs}` (platform arms at `camera/mod.rs:10-20`) and `dll/src/desktop/extra/screencap/{linux.rs, macos.rs, dmabuf.rs}` (`screencap/mod.rs:12-16`). Permissions landed as `layout/src/managers/permission.rs` with the TCC/portal/ConsentStore backends. Demo apps `examples/azul-camera`, `examples/azul-screenshare`, `examples/azul-meet` exist.
- **Superseded by:** its own §C.1/§C.2 — no `NodeType::Video`/`CameraPreview` was added and no `managers/camera.rs`/`screen_capture.rs` exists. The frame path instead runs through `ImageRef` + the overlay chokepoint (`layout/src/widgets/capture_common.rs:79` documents `apply_content_change` as the writer), i.e. `scripts/OVERLAY_JOURNAL_REFACTOR_PLAN.md` superseded the manager design.
- **Still open (concrete):** §B.1 iOS ReplayKit, §B.2 Android MediaProjection and §B.5 Windows `Windows.Graphics.Capture` screen-capture backends — `ls dll/src/desktop/extra/screencap/` is `{dmabuf, linux, macos, mod}.rs` only, so screen sharing is Linux + macOS. §C's NV12/YUV420 `RawImageFormat` addition never happened (`rg NV12|Yuv420 core/src/resources.rs` → only comments at `:845`, `:2811`); conversion still happens at capture time. Per the resource-architecture memory, the PipeWire screenshare tile is interactive-only-verified.
- **Research value:** Moderate and still live — §B is the only place the three missing screen-capture APIs are inventoried with entry points and constraints (ReplayKit's severe limits, MediaProjection's foreground-service requirement). §C.7's W3C-equivalent table (`getUserMedia`/`getDisplayMedia`) remains the target shape for a WASM backend. Keep; add a status header saying camera is done on 5 platforms, screencap on 2, and that the manager design was replaced by widgets + the overlay chokepoint.

---

#### scripts/research/04_system_integration.md

- **Verdict:** RESEARCH — keep; consumed on desktop, still the reference for the mobile halves.
- **Was:** 1442 lines (2026-05-19) covering three OS-integration features on five platforms. §1 file pickers (existing `tfd`-backed `FileDialog`/`MsgBox`/`ColorPickerDialog`; iOS `UIDocumentPickerViewController`, Android SAF, XDG portal, sync-vs-async integration sketch); §2 text input / IME / soft keyboard (iOS `UIKeyInput` vs `UITextInput`, Android `BaseInputConnection` over JNI, macOS `NSTextInputClient` already wired, XIM + `zwp_text_input_v3`, Windows TSF vs `WM_IME_*`, and a composition-event surface sketch); §3 geolocation (`CLLocationManager`, `LocationManager` vs FusedLocation, GeoClue D-Bus, `Windows.Devices.Geolocation.Geolocator`, plus a `GeolocationManager` + event-surface sketch); §4 cross-cutting (permission UX flow, async-result handles, manager registration, api.json/codegen, build artifacts).
- **Landed:** File pickers: `dll/src/desktop/extra/file_picker/{ios.rs, android.rs, mod.rs}` (desktop stays on `tfd`). IME: fully wired per the manager arc — `apply_preedit_to_text_cache` across Windows/X11/Wayland, `ImeManager::set_ic_focused` (X11 XIC gating) and `ImmAssociateContext` (Windows), macOS already native. Geolocation: `layout/src/managers/geolocation.rs` + `dll/src/desktop/extra/geolocation/` with real macOS/Linux(GeoClue)/Windows(classic-COM `ILocation`/`ILatLongReport`) providers; `EventType::GeolocationFix`/`GeolocationError` exist and are produced by the capability pump.
- **Superseded by:** n/a — its §3 design is what the manager arc implemented.
- **Still open:** §3.5's recommended **WinRT `Geolocator.PositionChanged`** push provider (the shipped Windows path is the classic-COM 1 s poll — `Geolocator` appears only in doc comments at `dll/src/desktop/extra/geolocation/mod.rs:10-12`). §2.2/§2.3's iOS/Android soft-keyboard + `InputConnection` detail is only partially exercised (mobile backends were D10 follow-ups). §1's async file-picker result handle on mobile.
- **Research value:** Moderate. The durable parts are §2.6 (TSF vs `WM_IME_*` trade-off, with the reasons TSF was not chosen) and §3's four-way geolocation comparison including the GeoClue accuracy→`Granted{Reduced}` mapping that the shipped permission backend uses. Keep in place; add a status header marking desktop DONE / mobile PARTIAL and naming the WinRT push provider as the one unimplemented recommendation.

---

#### scripts/research/05_assets_fonts_perms.md

- **Verdict:** RESEARCH — keep; the permission half shipped, the mobile-font half is still unbuilt and still correct.
- **Was:** 596 lines (2026-05-19). §0 TL;DR of *verified* gaps: `rust-fontconfig::FcFontCache::build()` has only linux/windows/macos arms so it finds **zero** fonts on iOS/Android, `OperatingSystem::current()` catch-alls to `Linux`, neither mobile backend amends `fc_cache`, and text renders on mobile only thanks to `AppConfig.bundled_fonts` + the embedded material-icons TTF. §1 fonts on mobile (CoreText, `/system/fonts` + `fonts.xml`, survey of alternative discovery crates, recommendation: **patch `rust-fontconfig`, don't swap**). §2 images (PhotoKit, MediaStore/SAF, "pickers before raw library access"). §3 permissions incl. a cross-platform state machine and W3C compat. §4 azul integration (new `PermissionManager`, `App::request_permission`, photo-picker API, font-registration API, Info.plist/manifest generation). §5 risks, §6 order, §7 open questions.
- **Landed:** The permission half, in full: `layout/src/managers/permission.rs:192` `PermissionManager` with the `Capability` enum (including `PhotoLibrary` read / add-only at `:60-62`), real macOS TCC / Linux xdg-desktop-portal / Windows ConsentStore backends, `PermissionChanged` event variants, and `permission`-as-provider in the capability pump. §1.5's recommendation was followed: `rust-fontconfig` is still the dependency, pinned `>=4.4.7, <4.5` at `layout/Cargo.toml:46` (the printpdf constraint from the resource memory).
- **Superseded by:** n/a.
- **Still open (verified):** §1 mobile fonts — no `/system/fonts`, `fonts.xml` or `CTFontManager` handling anywhere in this repo (`rg` → 0 hits); the fix lives in the external `rust-fontconfig` crate and there is no sign it shipped. §2/§4.4 photo picker — `dll/src/desktop/extra/file_picker/` has no PhotoKit/`PHPickerViewController`/MediaStore image path (`rg PHPicker|pick_photo` → 0 hits); `Capability::PhotoLibrary` exists as a permission with no picker behind it. §4.5 font-registration API and §4.6 Info.plist/AndroidManifest generation not verified as shipped.
- **Research value:** Moderate-high for the **§3.3 cross-platform permission state machine** — the collapse of five OS vocabularies (TCC / Android runtime / portal / ConsentStore / W3C Permissions API) into one `NotDetermined → Requested → Granted{Full|Reduced} | Denied | Restricted` lattice, which is exactly what `managers/permission.rs` implements and what makes the "Ask Every Time" and MDM-Restricted cases representable. Keep in place; add a status header: permissions SHIPPED, mobile fonts + photo picker STILL OPEN.

---

#### scripts/research/06_mvt_pdf.md

- **Verdict:** RESEARCH — keep; both halves shipped, but via a different node model than proposed.
- **Was:** 744 lines (2026-05-19). Part 1: MVT vector tiles + `<MapWidget>` — the Mapbox Vector Tile 2.1 protobuf format, the live-verified openfreemap endpoint, a Rust crate survey, a style-spec subset, an integration sketch and risks. Part 2: printpdf in both directions — A "render PDF inline" as `NodeType::Pdf(PdfRef)`, B "export the display list to PDF". Its sharpest finding is in the anchors: `DisplayListItem::TextLayout { layout: Arc<dyn Any…>, bounds, font_hash, … }` already existed and was already tagged "for PDF, accessibility, etc.", so **half of Direction B was already wired** before the work started. Also settles the WebRender question: the `DisplayItem` enum is closed with no custom-draw extension point, so new primitives must be composites of existing items (path (b)), matching the SVG precedent.
- **Landed:** MVT: `dll/src/desktop/extra/map/{mvt.rs (681), svg.rs, mod.rs}` + `layout/src/widgets/map.rs` (3992 lines) which uses `Dom::create_virtual_view` at `:402` — the tile surface is a VirtualView, exactly path (b). PDF: `dll/src/desktop/extra/pdf/mod.rs:109` `impl Pdf` with `from_styled_dom_with_resources` at `:181`, re-exported at `dll/src/unified/pdf.rs:68`, and referenced from `layout/src/callbacks.rs:3045, 3090` for off-thread export; `printpdf` 0.12.x from crates.io with its azul deps repointed to the workspace copy (`Cargo.toml:48-57`). `DisplayListItem::TextLayout` still present at `layout/src/solver3/display_list.rs:705`.
- **Superseded by:** its own node-model proposal — **neither `NodeType::MapTile(MapTileSource)` nor `NodeType::Pdf(BoxOrStatic<PdfRef>)` was added** (`rg MapTile|NodeType::Pdf core/src/dom.rs` → 0 hits). Both features are widgets over `ImageRef`/VirtualView, consistent with the same "no new NodeType" outcome as research/01. The typed-resource export API (`get_styled_dom_clone` / `FontCacheSnapshot` / `Pdf::from_styled_dom_with_resources`) from the `azul-pdf-typed-resources` memory is the shipped shape of Direction B.
- **Still open:** Direction A (render an existing PDF *inline* as a node) — `dll/src/desktop/extra/pdf/mod.rs:213` calls `announce_pdf_stub("Pdf::from_styled_dom_with_resources")` on the feature-off path, and no `NodeType::Pdf` consumer exists; treat inline PDF rendering as unbuilt. §1.4's style-spec subset coverage vs. what `map.rs` actually honors was not audited here.
- **Research value:** Moderate-high, and it is the *negative* findings that carry: (a) **WebRender's `DisplayItem` enum is closed** — every new visual primitive must be a composite of existing items or a pre-rasterised `ImageRef`, which is the constraint that decided map, SVG, PDF and video the same way; (b) the **positioned-glyph pipeline (`GlyphInstance` + `TextLayout`) is the PDF interchange format** — export fidelity comes from reusing the layout the screen already computed rather than re-shaping. Keep in place; add a status header saying MVT + PDF-export SHIPPED as widgets (no new NodeTypes), PDF-inline still open.

---

#### scripts/research/07_libsql_sqlite.md

- **Verdict:** RESEARCH — keep, but flag prominently that its top recommendation was inverted in implementation.
- **Was:** 514 lines (2026-05-19), self-labelled "research brief — superseded by next-session implementation". A crate-landscape survey (`libsql` 0.9.30 "production-ready" ← recommended; `turso`/Limbo "the new direction, **not yet ready**"; `rusqlite`; `sqlx`; `sea-orm`/`diesel`; no standalone SQLCipher crate) with a decision matrix; a three-mode connection string (`:memory:` / `file:` / `libsql://`); mobile path rules (iOS Application Support not Documents, Android `Context.getDatabasePath`); SQLCipher vs libsql native AES-256-CBC + keychain-derived keys; and an integration sketch (`DbHandle` in `RefAny`, one tokio runtime per `App`, sync/async/live-query patterns, `NodeType::Database(DbHandle)`, new EventFilter variants, never-log-auth-tokens).
- **Landed:** A `Db` API shipped — `dll/src/desktop/extra/sqlite/mod.rs` with `Db` as an always-present `repr(C)` FFI handle, engine behind a `db-sqlite` feature, surfaced in `api.json:105264`. Engine-agnostic surface (`SQL` strings + `azul_core::db::{DbValue, DbRows}`).
- **Superseded by:** implementation reversed the crate choice. The shipped engine is **`turso`** — the very crate this brief rated "not yet ready" — chosen because it is pure Rust with no C dependency so it cross-compiles to mobile without a C toolchain (`dll/src/desktop/extra/sqlite/mod.rs:5-12`, `dll/Cargo.toml:147-150`). It is additionally vendored from a fork (`Cargo.toml:80-85`, `github.com/fschutt/turso` branch `no-io-uring-optional`) because upstream turso pulls io-uring on Linux. §5.2's "one tokio runtime per App" was also rejected: turso's async futures are driven by a minimal in-crate `engine::block_on` with no reactor. So §1's decision matrix, §5.2 and the `libsql://` remote mode are all dead as recommendations.
- **Still open:** the remote/Turso mode (`libsql://host?authToken=`) and embedded replicas — turso has no equivalent, so the three-mode connection string collapsed to two. Encryption-at-rest (§4) has no shipped counterpart. `NodeType::Database(DbHandle)` (§5.4) and the new EventFilter variants (§5.5) were not added — same "no new NodeType" outcome as 01/06. Live queries (§5.7) unimplemented.
- **Research value:** Moderate, mostly as a **worked example of a research recommendation being correctly overridden by a constraint the brief under-weighted** (cross-compilation without a C toolchain beat crate maturity). §3's mobile data-directory rules (iOS Application Support vs Documents — the iCloud-backup trap; Android `getDatabasePath` vs external storage) and §4.3's keychain-derived key policy are durable and platform-factual regardless of engine. Keep in place; its existing status header is stale and MUST be rewritten to say "recommendation superseded: shipped engine is `turso` (fork), not `libsql`".

---

#### scripts/research/08_permission_dom_nodes.md

- **Verdict:** RESEARCH — keep; this is the permission-as-DOM-node concept doc and it shipped as designed.
- **Was:** 602 lines (2026-05-19), the architectural design turning "permissions as DOM nodes" into an implementable model. §1 weighs three semantic models — A imperative `App::request_camera_permission() -> Future`, **B DOM-node + lifecycle event (user's preference, recommended)**, C hybrid — landing on B with C-as-fallback for verbs. §2 the per-capability state machine + `Capability` enum + platform mappings that "collapse on the way in". §3 the **permission diff pass**: DOM presence of a capability node is the subscription, with reference counting and `LifecycleEvent::Mount/Unmount` interaction. §4 new `EventFilter` variants. §5 the invisible-probe `NodeType` question, weighing Alt 1 dedicated NodeType variants (recommended) vs Alt 2 `Div` + attribute vs Alt 3 lifecycle callback on any node. §6 `CallbackInfo` read/write surface. §7 W3C mapping. §8 four privacy risks including auto-prompting on first DOM appearance and "Ask Every Time" ephemeral grants.
- **Landed:** The core idea shipped. Alt 1 was implemented: `NodeType::GeolocationProbe(GeolocationProbeConfig)` at `core/src/dom.rs:649`, threaded through the exhaustive `NodeType` match arms (`:660`, `:827`, `:832`, `:1029`) with the constructor `Dom::create_geolocation_probe` at `:3923-3931`. `layout/src/managers/permission.rs:192` implements §2's state machine and `Capability` enum. §4's event variants shipped as `EventType`/`Hover`/`Window` `PermissionChanged` (+ `BiometricResult`, `KeyringResult`). §3's diff pass is the Subscribe scan in the shared layout path; the manager is an `EventProvider` targeting the capability's last-subscriber node.
- **Superseded by:** n/a — its recommendation is the shipped architecture.
- **Still open (verified):** **`GeolocationProbe` is the ONLY probe NodeType.** `rg` over `core/src/dom.rs` finds no camera/microphone/sensor probe variants, and the Subscribe scan matches only `GeolocationProbe` — this is exactly the "Camera/Sensor probe NodeTypes (GeolocationProbe is the template; blocked on media-widget NodeType design)" follow-up carried in MANAGER_FIX_PROGRESS. So permission-as-DOM-node exists for one capability out of ~eight; camera/mic permissions are driven imperatively from widget `AfterMount` instead — i.e. §1's Option C fallback in practice. §8.2's "Ask Every Time"/ephemeral-grant handling and §8.4's manifest-declaration generation are unverified.
- **Research value:** HIGH and durable — **permission-as-DOM-node** is a genuinely unusual framework idea: making the *presence of a node in the tree* the subscription, so grant/revoke is a reconciliation problem rather than an imperative lifecycle the app must hand-manage, with reference counting falling out of the diff. §1's three-option comparison (and why B beats A for a `f(State) -> Dom` framework) and §5's Alt-1-vs-Alt-2-vs-Alt-3 weighing are the transferable parts. Keep in place; its status header ("research / architecture — no code yet") is now FALSE and must be corrected to "shipped for Geolocation only; other capabilities still imperative".

---

#### scripts/audits/QUICK_PASS_HACKS_2026_07_28.md

- **Verdict:** ACTIVE — CI tier closed, but ~20 engine/test-tier hacks are still live and unticked.
- **Was:** A 1706-line read-only audit (2026-07-28, against `0a5c2ceba`) of "gates that cannot fail / tests that assert nothing / dead state". Organised as a severity index of four parts — Part A CI/release pipeline (`§1–§18` + `C1–C4`), Part B e2e scenario runner (`B1–B6`), Part D Rust test suite (`D1–D11`), Part E engine silent-fallbacks (`E1–E12`) — each row CRITICAL/HIGH/MEDIUM/LOW with a `file:line`, then a numbered prose section per finding carrying a **"scenario in which it passes while broken"** and a proposed fix. It also records negative results ("Verified SOLID / CLEAN / SUSPECTED / Not covered"), which is what makes it re-runnable.
- **Status header it already carries (quoted):** *"**Moving target.** `master` advanced three times during this audit (`9d1e62ffc` → `f7ee53088` → `0a5c2ceba`) … Every finding below was re-verified against the tree at the time it was written … **Nothing in this repo was modified.** The only file written is this report."* It has exactly one per-item status marker in 1706 lines (`text_edit_manager.display_list_dirty — FIXED`, line 1673), so there is no stale status line to distrust — but equally no way to tell what has since been fixed without re-grepping, which is why it must not be filed as ARCHIVE.
- **Landed (verified fixed — the CI/e2e tiers are essentially closed):** all four CRITICALs plus most HIGHs. §1 reftest-can-never-fail → `doc/src/reftest/mod.rs:170` (`ReftestOutcome.failed`) + hard exit at `doc/src/main.rs:1623-1631`; §2 integration tests now run outside `coverage` (`.github/workflows/rust.yml:921`); §3 `--gate-shipped` REQUIRED_LANGS (`scripts/e2e_language_matrix.sh:194-217`); §4 dep-justification empty-list → `return 1` (`scripts/check_dep_justifications.py:146-178`); §5 cargo-deny gating step (`rust.yml:5431`); §6 ASan `|| [ $? -eq 124 ]` removed (`rust.yml:1180-1193`); §12 `TSAN_OPTIONS=halt_on_error=1` (`:5782`); §14 `leak_regression` now runs with a zero-test guard (`:1157-1162`); §17 examples checked with all `required-features` (`:1758`); §18 `css_double_drop` actually executed (`:248`); C1/C2 (`docker/dockery.yml:61,126`; `scripts/strip_staticlib.sh:119,137,143`); D1 azul-doc tested (`:223`); B1–B6 all hardened (`layout/src/e2e/full.rs:11729`, `:13588`, `:5495-5503`, `:6110`, `:4473`, `:5988-6000`); E1 lifecycle-dispatch return now consumed centrally (`dll/src/desktop/shell2/common/event.rs:1490,1500`) with a regression test (`dll/tests/headless_lifecycle.rs:407-436`); E9 macOS Cmd+Z now redraws (`dll/src/desktop/shell2/macos/mod.rs:6973,6993`); E10 tautological `frame_needs_regeneration` deleted. Several fixes quote the old defect verbatim as a tombstone comment — a good pattern.
- **Superseded by:** n/a.
- **STILL PRESENT — the laziness / WIP list (grep-verified today; I independently re-checked E7, E3, D2 and D11):**

  | Sev | Hack | Evidence |
  |---|---|---|
  | HIGH | 11 of 20 `deploy_pages.needs` are `continue-on-error: true` — sequencing, not gating. Now *documented* but structurally unchanged | `.github/workflows/rust.yml:3814-3828` |
  | HIGH | e2e `run_keyboard_default_action` catch-all silently drops `ActivateFocusedElement` / `ScrollFocusedContainer` / `SubmitForm` / `CloseModal` / `SelectAllText` | `layout/src/e2e/runner.rs:2567` (`_ => (ProcessEventResult::DoNothing, false)`) |
  | HIGH | `pending_focus_request` is entirely dead — `request_focus_change`/`take_focus_request` have only `#[cfg(test)]` callers, and the e2e assertion on it is an absence check that can never fail | `layout/src/managers/focus_cursor.rs:57,99,104` (callers `:722-728` in tests); unfailable reader `layout/src/e2e/full.rs:5931` |
  | HIGH | 3 `DISABLED_*` phantom features still gate whole test files → 0-test binaries that print `ok` | `layout/Cargo.toml:321-323` |
  | MEDIUM | **macOS text input still swallows relayout**: `apply_text_changeset`'s `ShouldIncrementalRelayout` → `RegenerateLayoutIncremental` → `_ => {}`, so an edit needing relayout gets neither relayout nor redraw | `dll/src/desktop/shell2/macos/events.rs:733-746` (catch-all at `:746`) |
  | MEDIUM | `determine_events_from_managers` has zero production callers while its architecture-diagram comment still claims it *is* the pipeline; its only integration test is a `DISABLED_*` file | `layout/src/event_determination.rs:42,66`; `layout/tests/event_determination.rs:68` |
  | MEDIUM | `check_properties_changed` / `check_layout_properties_changed` zero production callers **and the false doc comment claiming the property cache consults them is still there** | `core/src/prop_cache.rs:2404,2421`; comment `layout/src/callbacks.rs:605` |
  | MEDIUM | `gesture.pad_state` writer is test-only → `get_wacom_pad()` returns `None` forever (same defect as the manager-arc Wacom follow-up) | `layout/src/managers/gesture.rs:959` / sole caller `:3112` in tests; reader `layout/src/callbacks.rs:3536` |
  | MEDIUM | `tests/src/layout.rs` (~37 tests) orphaned — `lib.rs` declares `mod layout_test;`, never `mod layout;` | `tests/src/lib.rs:14` |
  | MEDIUM | `kitchen_sink_integration.rs` tautology: the error branch itself emits `fn main()`, so the `\|\|` can never be false — in a file that re-implements the function under test | `dll/tests/kitchen_sink_integration.rs:103` (re-impl at `:15-30`) |
  | MEDIUM | X11 still swallows one `regenerate_layout()` (resize/DPI sites were fixed, `apply_size_to_content` was not) | `dll/src/desktop/shell2/linux/x11/mod.rs:3341` `let _ =` vs `:3655` `if let Err(e)` |
  | MEDIUM | `css/tests/test_parser_robustness.rs` still assertion-thin: 44 `#[test]` fns, 27 lines containing `assert` | `css/tests/test_parser_robustness.rs` |
  | LOW | `build_binaries` cache key omits root `Cargo.toml`, `dll/Cargo.toml`, `api.json`, and keeps a prefix `restore-keys` | `.github/workflows/rust.yml:1489,1491` |
  | LOW | `build_binaries` symbol gates are `[ -f ] \|\| continue` loops — vacuous if a path moves | `.github/workflows/rust.yml:1675,1704` (also `:1584,1609,1922,1962,3138`) |
  | LOW | Miri gate is a bare name filter with **no zero-match guard** (the `leak_regression` gate got one; this did not) | `.github/workflows/rust.yml:971` |
  | LOW | `SubmitForm` / `CloseModal` / `SelectAllText` are now explicit arms whose body is a comment | `dll/src/desktop/shell2/common/event.rs:5704-5707` |
  | LOW | `validate_class_definition` / `ensure_chrome_references` zero-caller | `doc/src/print.rs:429`; `doc/src/reftest/regression.rs:553` |
  | LOW | Phantom cargo features gating real code: `table_layout` declared in **no** `Cargo.toml` in the workspace; `xml` declared in `layout`/`dll` but not in `azul-core`, which `#[cfg]`s on it | `core/src/styled_dom.rs:1181`; `core/src/dom.rs:5778,5789` |
  | LOW | `catch_unwind` `Err` arms that only `eprintln!` | `layout/src/widgets/screencap.rs:1293,1304`; `capture_common.rs:943`; `video.rs:2130` |
  | LOW | `unwrap_or_default()` masking a poisoned mutex | `layout/src/widgets/drop_down.rs:579-583` |
  | LOW | Dockerfile prelift diagnostic `\|\| echo` with no `pipefail` | `docker/Dockerfile:107,136` |

- **Global marker baseline (current tree, `*.rs`, gitignore-respecting):** first-party `TODO\|FIXME\|HACK\|XXX\|unimplemented!(\|todo!(` = **217 lines / 91 files** (layout 106, dll 37, core 34, doc 24, css 16, examples+tests 0); vendored `webrender/` adds 280 → workspace 497. By marker: `TODO` 463, `FIXME` 24, `HACK` 7, `XXX` 5, `unimplemented!(` 7, `todo!(` 3. `dll/`: 164 `.unwrap()`, 210 `.expect(` — note the audit did *not* set a target for these, so this is a fresh baseline, not a delta.
- **Still open:** everything in the table above; the concentration is **Part E (engine dead state / silent fallbacks)** and **Part D (test suite)**. Most actionable single item: the macOS `RegenerateLayoutIncremental` swallow (`macos/events.rs:746`) — a real user-visible bug, not just dead code.
- **Research value:** low as content; the transferable idea is the audit's per-finding **"scenario in which it passes while broken"** field, which is what converts "this looks sloppy" into a falsifiable claim, plus its habit of recording *negative* results (Verified SOLID / CLEAN / Not covered) so the next pass does not re-litigate them. Same family as the `azul-gates-with-wrong-premises` methodology note.

---

### Cross-cutting notes for the report

- **Is `OVERLAY_JOURNAL_REFACTOR_PLAN.md` fully consumed?** No — ~80%. O1/O2/O3 shipped (overlay, journal, chokepoint, resolver, structural preview, pagination/`PageSequence`) and the §9.1 CI lint is live at `.github/workflows/rust.yml:88-116`. Not consumed: the `Text` and `Structural` chokepoint arms (text/structural edits are neither routed through `apply_content_change` nor journaled), the two surviving in-place `set_node_type` mutations at `dll/src/desktop/shell2/common/event.rs:2095` and `:2960`, journal-driven damage / `previous_display_list` deletion (O4), the `overlay`/`journal` non-interference rows, and all four §10 open questions (GPU epoch chief among them).
- **A recurring architectural outcome worth naming in the report:** four independent research briefs (01 camera, 06 map/PDF, 07 database, 08 permissions) each proposed a new `NodeType`; only ONE was added (`GeolocationProbe`). Everything else became a widget over `ImageRef`/VirtualView. The reason is in 06: WebRender's `DisplayItem` enum is closed. That is a real, load-bearing constraint that shaped four features and belongs in whatever architecture doc survives.
- **Do not delete `scripts/OVERLAY_JOURNAL_REFACTOR_PLAN.md` in place** without updating `.github/workflows/rust.yml:96`, which cites its path in the lint's rationale comment.


---
---

# Section B — Codebase marker sweep (TODO / FIXME / HACK / WIP)

## Part 17 — Unfinished / lazy / WIP work-marker inventory (`/home/fs/Development/azul`)

Snapshot: `master` @ `f1c43ba60`, 2026-08-01. **No repo file was modified.**

---

### Raw marker counts by crate

#### Method (reproducible)

`rg` on this box is a shell **function** wrapper (`exec -a rg $CLAUDE_CODE_EXECPATH`), so it cannot
be reached through `xargs`. All sweeps therefore pass an explicit file array built from
`git ls-files`.

```bash
cd /home/fs/Development/azul
S=<scratchdir>

# 1. scope: the INCLUDE dirs + root files, tracked files only
git ls-files > $S/all_files.txt
grep -E '^(css|core|layout|dll|doc|examples|e2e|tests|tools|packaging|\.github|scripts)/|^[^/]+$' \
     $S/all_files.txt \
 | grep -vE '^(doc/xhtml1/|doc/fonts/|doc/target/|scripts/E2E_TESTS)' \
 | grep -vE '^(Cargo\.lock|api\.json|hello-world\.obj)$' > $S/scoped_raw.txt
# 2. drop files > 1 MiB
while IFS= read -r f; do [ -f "$f" ] || continue
  [ "$(stat -c%s "$f")" -le 1048576 ] && printf '%s\n' "$f" >> $S/scoped.txt
done < $S/scoped_raw.txt          # -> 1910 files

# 3. split code vs prose
grep -E '\.(rs|c|h|cpp|hpp|py|js|sh|ps1|toml|yml|yaml|json|java|zig|go|rb|php|pas|scala|css|html|xhtml|xml)$' \
     $S/scoped.txt > $S/code.txt   # 1316 files (882 .rs)
grep -vE '<same regex>' $S/scoped.txt > $S/prose.txt   # 594 files

# 4. vendored, counted separately
grep -E '^(webrender|third_party)/' $S/all_files.txt   # -> 235 files <= 1 MiB

# 5. per-marker, per-crate counts
readarray -t CODE < $S/code.txt
rg -c [-i] -e '<pattern>' "${CODE[@]}" \
  | awk -F: '{c=$NF; f=$0; sub(/:[0-9]+$/,"",f); split(f,a,"/");
              b=(f ~ /\//)?a[1]:"ROOT"; s[b]+=c} END{for(k in s) print k, s[k]}'
```

Exact patterns used (`i` = `rg -i`):

| marker | pattern | mode |
|---|---|---|
| TODO / FIXME / XXX / HACK / WIP | `\bTODO\b` etc. | case |
| todo! / unimplemented! / unreachable! | `todo!\(` / `unimplemented!\(` / `unreachable!\(` | case |
| panic-not | `panic!\("[Nn]ot` | case |
| ignore-attr / allow-dead-code / cfg-FALSE | `#\[ignore` / `#\[allow\(dead_code\)\]` / `#\[cfg\(FALSE\)\]` | case |
| for-now / follow-up / temporar / workaround | `for now` / `follow.?up` / `temporar` / `work.?around` | i |
| not-implemented / no-op-for-now / currently-ignore | `not implemented` / `no.?op for now` / `currently ignore` | i |
| placeholder / stub / simplified / naive | `placeholder` / `\bstubs?\b` / `simplif` / `naive` | i |
| in-the-future / should-be / revisit / left-as | `in the future` / `should be` / `revisit` / `left as` | i |
| hardcoded / assume / best-effort / approximat | `hard.?cod` / `assum` / `best.?effort` / `approximat` | i |
| does-not-handle / unsupported | `does ?n.t handle` / `unsupported` | i |

Excluded per scope rules: `target/`, `.git/`, `Cargo.lock`, `api.json` (4.9 MB), `hello-world.obj`,
`scripts/E2E_TESTS*.txt` (5 MB generated corpora), `doc/xhtml1/` (10 037 vendored W3C spec files),
`doc/fonts/`, `doc/target/`, and anything > 1 MiB.
`scripts/` is **not** in the stated INCLUDE list; it is counted but flagged — its markers are
almost entirely planning/session-log markdown, not code debt.

#### A. Code files (1316 files, 882 of them `.rs`)

| marker | .github | ROOT | core | css | dll | doc | e2e | examples | layout | scripts | tests | **TOTAL** |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `TODO` | 1 | 0 | 32 | 13 | 37 | 10 | 0 | 4 | 66 | 0 | 0 | **163** |
| `FIXME` | 0 | 0 | 2 | 0 | 0 | 4 | 0 | 0 | 2 | 0 | 0 | **8** |
| `XXX` | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | **1** |
| `HACK` | 0 | 0 | 0 | 2 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | **6** |
| `WIP` | 2 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | **5** |
| `todo!(` | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | **3** |
| `unimplemented!(` | 0 | 0 | 0 | 0 | 0 | 6 | 0 | 0 | 0 | 0 | 0 | **6** |
| `unreachable!(` | 0 | 0 | 9 | 6 | 6 | 7 | 0 | 0 | 11 | 0 | 0 | **39** |
| `panic!("not` | 0 | 0 | 0 | 7 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | **8** |
| `#[ignore` | 0 | 0 | 0 | 0 | 13 | 0 | 0 | 0 | 0 | 0 | 0 | **13** |
| `#[allow(dead_code)]` | 0 | 0 | 11 | 1 | 15 | 5 | 0 | 0 | 6 | 0 | 0 | **38** |
| `#[cfg(FALSE)]` | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | **0** |
| `for now` | 0 | 0 | 11 | 7 | 29 | 24 | 0 | 5 | 33 | 2 | 1 | **112** |
| `follow.up` | 2 | 2 | 4 | 0 | 21 | 35 | 0 | 11 | 12 | 0 | 0 | **87** |
| `temporar` | 0 | 0 | 11 | 8 | 8 | 25 | 0 | 1 | 22 | 2 | 1 | **78** |
| `workaround` | 0 | 1 | 1 | 1 | 6 | 11 | 0 | 1 | 9 | 0 | 1 | **31** |
| `not implemented` | 0 | 0 | 1 | 2 | 6 | 4 | 0 | 0 | 20 | 0 | 9 | **42** |
| `no-op for now` | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | **1** |
| `placeholder` | 11 | 0 | 49 | 14 | 164 | 73 | 1 | 43 | 428 | 6 | 0 | **789** |
| `stub` | 35 | 0 | 10 | 3 | 283 | 62 | 0 | 1 | 95 | 27 | 0 | **516** |
| `simplif` | 0 | 0 | 1 | 6 | 9 | 10 | 0 | 1 | 20 | 0 | 0 | **47** |
| `naive` | 0 | 0 | 20 | 2 | 1 | 4 | 0 | 0 | 34 | 2 | 0 | **63** |
| `in the future` | 0 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | **3** |
| `should be` | 1 | 0 | 212 | 88 | 52 | 144 | 0 | 2 | 398 | 9 | 45 | **951** |
| `revisit` | 0 | 1 | 2 | 0 | 3 | 4 | 0 | 0 | 2 | 1 | 0 | **13** |
| `left as` | 0 | 0 | 7 | 1 | 8 | 3 | 0 | 0 | 9 | 0 | 0 | **28** |
| `hard.?coded` | 2 | 0 | 5 | 7 | 26 | 14 | 1 | 3 | 29 | 5 | 0 | **92** |
| `assum` | 4 | 0 | 19 | 13 | 31 | 36 | 1 | 5 | 65 | 6 | 0 | **180** |
| `best.effort` | 20 | 0 | 2 | 1 | 19 | 8 | 0 | 3 | 8 | 3 | 0 | **64** |
| `approximat` | 0 | 0 | 16 | 9 | 12 | 7 | 0 | 1 | 63 | 2 | 2 | **112** |
| `does n't handle` | 0 | 0 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | **3** |
| `unsupported` | 1 | 0 | 17 | 34 | 31 | 18 | 2 | 0 | 115 | 13 | 0 | **231** |
| `currently ignore` | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 1 | 0 | 0 | **3** |

**Grand total, code files: 3736 matches.**

#### B. Prose files in scope (594 `.md`/`.txt`/…)

| marker | dll | doc | examples | layout | scripts | tests | **TOTAL** |
|---|---|---|---|---|---|---|---|
| `TODO` | 2 | 8 | 12 | 0 | 483 | 1 | **506** |
| `WIP` | 0 | 50 | 1 | 0 | 3 | 0 | **54** |
| `stub` | 1 | 81 | 4 | 0 | 371 | 0 | **457** |
| `placeholder` | 0 | 52 | 13 | 0 | 99 | 5 | **169** |
| `should be` | 0 | 18 | 0 | 0 | 138 | 0 | **156** |
| `follow.up` | 0 | 5 | 2 | 0 | 139 | 0 | **146** |
| `workaround` | 0 | 7 | 2 | 0 | 122 | 0 | **131** |
| `hard.?coded` | 0 | 14 | 0 | 0 | 112 | 0 | **126** |
| `not implemented` | 2 | 8 | 12 | 0 | 62 | 0 | **84** |
| `assum` | 0 | 9 | 3 | 1 | 67 | 0 | **80** |
| `for now` | 0 | 3 | 1 | 0 | 70 | 0 | **74** |
| `unsupported` | 0 | 4 | 1 | 0 | 48 | 0 | **53** |
| (all remaining markers) | 0 | 33 | 8 | 0 | 168 | 5 | **214** |

**Grand total, prose files: 2192 matches — but 1826 of them (83 %) are in `scripts/`**, which is
a corpus of planning docs, session logs and audit reports (`SPEC_CONFORMANCE_REVIEW.md`,
`MOBILE_SESSION_LOG.md`, `BINDINGS_REVIEW_2026_07_04.md`, `problems/`, `research/`). These are
*records of debt already triaged elsewhere*, not new debt, and are excluded from all triage below.

#### Noise concentration in the code table (why the big numbers are not debt)

| marker | raw | dominant noise source | signal-bearing residue |
|---|---|---|---|
| `should be` | 951 | assertion messages (`assert_eq!(…, "x should be y")`) in `*/tests/*` and `#[cfg(test)]` mods — top files are `core/tests/css_inheritance.rs` (32), `core/tests/compact_cache.rs` (28), `layout/tests/inline_block_text.rs` (27) | ~15 |
| `placeholder` | 789 | the **CSS/HTML `placeholder` attribute** — `layout/src/widgets/text_input.rs` (110), `text_area.rs` (90), `combobox.rs` (28) are the input-placeholder API | ~30 |
| `stub` | 516 | domain vocabulary of the **WASM lifter** (`dll/src/web/symbol_table.rs` 75, `transpiler_remill.rs` 40, `web/mod.rs` 13) + CI job names (`.github/workflows/rust.yml` 28) + `probe`/`file`/`http` platform-shim modules that legitimately name their fallback "stub" | ~40 |
| `assum` | 180 | "assumes"/"assumption" in explanatory doc comments and in `debug_assert` rationale | ~20 |
| `unsupported` | 231 | `enum …::Unsupported` variants + CSS parser rejection messages (the *correct* behaviour) | ~25 |
| `hard.?coded` | 92 | **mostly historical**: ~55 read "*was* hardcoded — fixed" (MWA-C-* / seam-audit commit annotations). Only ~12 describe a live hard-code | ~12 |
| `naive` | 63 | test names (`naive_baseline`) + prose contrasting with the implemented algorithm | ~5 |
| `approximat` | 112 | mostly `assert_approx_eq` / float-tolerance helper names | ~10 |

**Bottom line:** of 3736 code-file matches, roughly **250–300 carry any signal**, and after reading
them **~60 are real functional gaps**. The 40 highest-impact are listed below.

---

### Latent panics in shipped code

#### `todo!()` / `unimplemented!()` written directly in shipped code: **ZERO**

Every hit is either (a) prompt text inside the doc/review tooling, or (b) a string the code
generator *emits* into generated bindings.

| path:line | what it is | reachable? |
|---|---|---|
| `doc/src/reftest/autoreview.rs:217,250` | prompt text listing `todo!()` as a smell to look for | no |
| `doc/src/doc_coverage/mod.rs:250-251` | same, prompt text | no |
| `doc/src/codegen/v2/rust/static_binding.rs:489` | `builder.line("unimplemented!()")` — emitted as the body of any `api.json` function with **no `fn_body`** | **conditional** |
| `doc/src/codegen/v2/lang_rust.rs:3919` | `"{{ /* ERROR: No fn_body for {} */ unimplemented!() }}"` | **conditional** |
| `doc/src/codegen/v2/lang_python.rs:1328-1330` | `// TODO: callback type conversion` + `unimplemented!("Option<{}> not yet supported in Python")` for any `Option<Callback…>` enum variant | **conditional** |

Verified: `rg 'unimplemented!\(\)' dll/src examples/` returns **nothing** — no currently-generated
binding contains one. These are latent *generator* bugs: adding an `Option<Callback>` variant or a
body-less function to `api.json` silently ships a panicking binding instead of failing the build.
**Recommendation: make all three sites `panic!` at codegen time, not at runtime.**

#### Real latent panics found by other patterns

| path:line | marker | why it can fire |
|---|---|---|
| `core/src/task.rs:494` | `Self::Tick(_) => unreachable!()` | `Instant::into_std_instant()` **panics for every `Tick` instant**. Tick instants are the FFI/test clock; any host that builds an `Instant::Tick` and calls a std-clock path aborts. Documented at `core/src/task.rs:1826` but not returned as an error. |
| `css/src/props/style/transform.rs:906` | `_ => unreachable!()` | terminal arm of `parse_style_transform`; sound only while `parse_parentheses` can never return an unlisted stopword. Guarded by a negative-control test (`transform.rs:1254`) — **accepted, but fragile**: adding a stopword to the tokenizer without a match arm is a panic on user CSS. |
| `css/src/props/style/filter.rs:752`, `background.rs:1132`, `basic/color.rs:1137` | `_ => unreachable!()` | same shape, same guard-by-test. |
| `layout/src/window_state.rs:221` | `unreachable!("WindowCreateOptions::create must not invoke the layout callback")` | genuine invariant guard, fine. |
| `dll/src/desktop/shell2/run.rs:825,1536,1616` | `unreachable!()` in the app-termination state machine | bare, no message — a new `AppTerminationBehavior` variant aborts the app loop. |

The remaining 30 `unreachable!` sites are exhaustive-match tails over closed local enums (noise).

`panic!("not …")` — all 8 hits are inside `#[cfg(test)]` assertion helpers
(`css/src/props/style/filter.rs:1318-2211`, `layout/src/font.rs:3598`). No shipped-code hits.

---

### Ignored / disabled tests

**11** `#[ignore]` attributes, all in `dll/`, all with a stated hardware reason. No `#[cfg(FALSE)]`,
no `#[cfg(any())]`, no commented-out `#[test]` blocks found.

| path:line | stated reason |
|---|---|
| `dll/src/desktop/display.rs:1353` | "Requires main thread and real display hardware" (`test_get_displays`) |
| `dll/src/desktop/display.rs:1371` | same (`test_get_primary_display`) |
| `dll/src/desktop/display.rs:1384` | same (`test_get_display_at_point`) |
| `dll/src/desktop/menu.rs:498` | same (`test_menu_position_auto_cursor_default`) |
| `dll/src/desktop/menu.rs:518` | same (`test_menu_position_auto_hit_rect_default`) |
| `dll/src/desktop/menu.rs:541` | same (`test_menu_position_overflow_right`) |
| `dll/src/desktop/menu.rs:563` | same (`test_menu_position_overflow_bottom`) |
| `dll/src/desktop/menu.rs:585` | same (`test_submenu_positioning_right`) |
| `dll/src/desktop/extra/screencap/dmabuf.rs:658` | "requires a GPU / libEGL; run explicitly" (`egl_init_and_query`) |
| `dll/src/desktop/extra/video_codec/provision.rs:1850` | "needs a real Linux desktop: inspects /lib/modules + /boot … Run explicitly." |
| `dll/src/desktop/extra/video_codec/provision.rs:1888` | "needs a real Linux desktop: depends on apt package metadata + kernel module trees … Run explicitly." |

**Assessment:** every reason is legitimate and specific — this is a healthy `#[ignore]` population,
not hidden debt. The real risk is that **all 11 live in `dll/`, and per MEMORY.md
`cargo test -p azul-dll --lib` is not in CI**, so these tests are doubly unrun: ignored *and* in a
crate CI never test-builds. The five `menu.rs` positioning tests are pure geometry
(`LogicalRect` math) and look like they do **not** actually need display hardware — worth
re-checking whether the `#[ignore]` is a stale copy-paste from `display.rs`.

---

### Top real functional gaps

Ranked within theme; **H/M/L = user-visible impact**.

#### Theme 1 — Print / paged media (PDF output)

| # | path:line | marker | what is actually missing | impact |
|---|---|---|---|---|
| 1 | `layout/src/solver3/display_list.rs:5120` | `"State management items - skip for now (would need proper per-page tracking)"` | The paged/PDF display-list filter drops **`PushClip`/`PopClip`, `PushScrollFrame`/`PopScrollFrame`, `PushStackingContext`/`PopStackingContext`** entirely (`=> None`). Printed pages therefore lose all clipping and stacking context — `overflow:hidden` content bleeds, z-order flattens. | **H** |
| 2 | `layout/src/solver3/display_list.rs:5193` | `"Filter effects - skip for now (would need proper per-page tracking)"` | Same arm drops **`PushFilter`, `PushBackdropFilter`, `PushOpacity`, `PushReferenceFrame`**. Any `opacity`, `filter:` or transform-reference-frame in a printed document renders at full opacity / unfiltered. | **H** |
| 3 | `layout/src/solver3/paged_layout.rs:10` | `"Full CSS @page rule parsing is not yet implemented. The FakePageConfig provides … a temporary solution."` | `@page { size; margin; @top-center …}` is unparsed; page geometry is only settable programmatically via `FakePageConfig`. CSS-authored print stylesheets are silently ignored. | **H** |
| 4 | `layout/src/solver3/pagination.rs:760` | `"Re-wrapping text per fragmentainer is not implemented yet — content lays out at the DEFAULT width on every page"` | A `PageSequence` that varies content width (landscape override / different margins) lays out text at the *first* page's width on every page → overflow or short lines. Announced once at runtime (honest), but wrong output. | **H** |
| 5 | `layout/src/solver3/pagination.rs:267` | `"TODO: Look up named string from document context"` | CSS named strings (`string-set` / `content: string(chapter)`) in running headers resolve to nothing. | **M** |

#### Theme 2 — Text / bidi / typography

| # | path:line | marker | what is actually missing | impact |
|---|---|---|---|---|
| 6 | `layout/src/text3/cache.rs:7155` | `"TODO(text3-review): RTL glyph-level visual reversal is NOT applied."` | Runs are ordered visually but each run's glyphs are emitted in **logical** order. **All Hebrew/Arabic text renders reversed.** Also breaks `get_selection_rects` (one rect for a bidi selection instead of one per directional segment). Three named failing tests are cited. This is the single largest correctness gap in the tree. | **H** |
| 7 | `layout/src/text3/cache.rs:7839` | `"TODO: use actual font's space_width … For now, approximate space advance as 0.5 * font_size"` | Tab stops are computed from a fake 0.5-em space rather than the resolved font's real space advance → `tab-size` misaligns in every non-Latin/monospace font. | **M** |
| 8 | `layout/src/text3/cache.rs:1671-1672,8696` | `"NOT IMPLEMENTED: text-box-trim property / text-box-edge property"` (+ `solver3/fc.rs:2604`) | `text-box-trim`/`text-box-edge` parsed but never applied; half-leading is never trimmed. | **M** |
| 9 | `layout/src/text3/cache.rs:1701` | `"[TODO] initial-letter (drop caps) not implemented"` | `initial-letter` silently no-ops. | **M** |
| 10 | `layout/src/text3/cache.rs:9225-9226` | `"hanging-punctuation is declared in UnifiedConstraints but not used here"` | Declared, parsed, threaded into constraints, **never read**. | **M** |
| 11 | `layout/src/text3/cache.rs:6318` | `"❌ TODO: Should re-orient if fragments have different writing modes"` | Text-orientation uses the **first** fragment's constraints only; a flow crossing a writing-mode boundary orients wrongly. | **M** |
| 12 | `layout/src/text3/knuth_plass.rs:113` | `let is_vertical = false; // Knuth-Plass is horizontal-only for now` | Vertical writing modes fall off the Knuth–Plass path entirely (greedy fallback only). | **M** |
| 13 | `layout/src/text3/knuth_plass.rs:201` | `"cross-direction hyphenation suppression (LTR in RTL / RTL in LTR) not yet implemented"` | Hyphenation fires inside opposite-direction runs. | **L** |
| 14 | `layout/src/solver3/fc.rs:4173` | `"TODO(superplan): use the resolved primary font's real OS/2 metrics"` | Line-box metrics use synthesized values instead of the font's OS/2 table → line heights differ from browsers. | **M** |

#### Theme 3 — Layout / CSS engine

| # | path:line | marker | what is actually missing | impact |
|---|---|---|---|---|
| 15 | `layout/src/solver3/display_list.rs:2605` + `:3170` | `"CSS Overflow 3 says overflow clips should NOT apply to abs-pos descendants"` | Absolutely-positioned descendants whose containing block is *outside* the scroller are wrongly clipped by it — a classic dropdown/popover-gets-cut bug. | **H** |
| 16 | `layout/src/solver3/display_list.rs:4286` | `"TODO: Text shadows not yet implemented"` | `text-shadow` parses and is stored, never painted. | **H** |
| 17 | `layout/src/solver3/taffy_bridge.rs:1280` | `"TODO: Implement grid stretch detection"` → returns `(false, false)` | Grid items never get intrinsic-cross-size suppression, so `align-items: stretch` on grid mis-sizes. Flex has the logic; Grid returns the do-nothing tuple. | **H** |
| 18 | `layout/src/solver3/taffy_bridge.rs:315` | `"TODO: visibility, z_index still missing"` | `visibility` and `z-index` are not carried across the Taffy bridge at all. | **M** |
| 19 | `layout/src/solver3/display_list.rs:4290` | `"text-overflow ellipsis side depends on direction (RTL clips left, LTR clips right); not yet implemented"` | RTL ellipsis truncates the wrong end. | **M** |
| 20 | `layout/src/solver3/fc.rs:3323` | `"align-content != normal should also establish BFC per CSS-DISPLAY-3, but align-content is not yet implemented for block containers"` | `align-content` on block containers is a no-op *and* skips BFC establishment (float/margin-collapse fallout). | **M** |
| 21 | `layout/src/solver3/fc.rs:4867` | `"For now, we use a simple heuristic: if there are children, assume not empty"` | `:empty` / empty-cell determination is structural-only — a subtree of only-whitespace or `display:none` children counts as non-empty. | **M** |
| 22 | `layout/src/solver3/fc.rs:5221` | `"a rowspan cell that *starts* in row 0 but whose content baseline sits in a later row is approximated by row_baselines[0]"` | `inline-table` baseline misalignment with rowspans. | **L** |
| 23 | `layout/src/solver3/sizing.rs:817` + `:1513` + `:1550` | `"cyclic percentage contributions … (not yet implemented)"`, `"orthogonal flows would require child block size as input (not yet implemented)"` | Percentage min/max-height on children and orthogonal-flow sizing both fall back to viewport/ICB defaults. | **M** |
| 24 | `layout/src/solver3/getters.rs:2658` | `"viewport units (vw/vh/...) in a vertical-align <length>"` | `vertical-align: 2vh` resolves to 0. | **L** |
| 25 | `layout/src/solver3/layout_tree.rs:3381-3386` | `"run-in falls back to block; reparenting not implemented"` (5 spec sites) | `display: run-in` never merges into the following block. Deliberate + matches most browsers; recorded for completeness. | **L** |
| 26 | `css/src/shape.rs:207` + `layout/src/solver3/display_list.rs:6716,8937` + `css/src/shape_parser.rs:296,308` | `"path parsing is not yet implemented — data is stored but not interpreted"` | `clip-path: path(...)` / `shape-outside: path(...)` store the string and **never clip**. `shape_parser` also cannot resolve `em/rem/vh/vw` or `%` in shape coordinates. | **M** |
| 27 | `css/src/shape_parser.rs:152` | `"fill-rule … currently ignored — the scanline rasterizer always uses even-odd fill"` | `nonzero` fill-rule silently becomes even-odd. | **L** |
| 28 | `layout/src/managers/scroll_into_view.rs:444` | `"TODO: Check CSS scroll-behavior property on the scroll container"` → always `Instant` | `scroll-behavior: smooth` never smooth-scrolls; `ScrollIntoViewBehavior::Auto` is hard-wired to instant. | **M** |

#### Theme 4 — Rendering / compositing

| # | path:line | marker | what is actually missing | impact |
|---|---|---|---|---|
| 29 | `dll/src/desktop/shell2/common/cpu_compositor.rs:50` | `"TODO: Implement actual rasterization / For now, just clear to white"` | `CpuCompositor::rasterize()` **ignores the display list and clears the framebuffer to white**. It is re-exported from `shell2/mod.rs` as the documented "CPU-only fallback compositor". Any platform that selects it renders a blank white window. (The real CPU path is `layout/src/cpurender/` — this type is a parallel, empty implementation that is nonetheless public API.) | **H** |
| 30 | `layout/src/cpurender/compositor.rs:1532` | `"Blend, Flood, ColorMatrix, DropShadow, ComponentTransfer, Offset, Composite not yet implemented"` — a bare `_ => {}` | 7 of the SVG/CSS filter primitives silently do nothing in the CPU renderer (hue-rotate and a few others are implemented). | **M** |
| 31 | `layout/src/cpurender/raster.rs:1991` | `"TODO(superplan g4): backdrop-filter is unimplemented in the CPU renderer"` | `backdrop-filter` renders as no filter on the CPU backend. | **M** |
| 32 | `layout/src/solver3/display_list.rs:4283-4284` | `"This will always paint images over the glyphs"` / `"Handle z-index within inline content"` | Inline replaced content is always painted above text regardless of z-order. | **M** |
| 33 | `core/src/gl.rs:864` | `"TODO: Handle overflow of Epochs correctly (low priority)"` | GL texture GC compares epochs without wrap handling; a long-lived session that wraps `Epoch` leaks or frees live textures. Low probability, high severity. | **L** |
| 34 | `layout/src/xml/svg.rs:145` + `:1065` | `"e.apply_line_width - not present in lyon 17!"`, `"radii not respected on latest version of lyon"` | SVG stroke `line-width` and rounded-rect radii are dropped by the tessellator. Pinned by a test at `svg.rs:3349` (documented, still wrong output). | **M** |
| 35 | `layout/src/xml/svg.rs:2340` | `"Decode PNG back to raw RGBA (TODO: render_svg_to_rgba to avoid PNG round-trip)"` | Every SVG rasterization does an encode→decode PNG round trip. Performance, not correctness. | **L** |

#### Theme 5 — Platform shells (the largest cluster)

| # | path:line | marker | what is actually missing | impact |
|---|---|---|---|---|
| 36 | `dll/src/desktop/shell2/linux/gnome_menu/protocol_impl.rs:251,459,483,550` | `"TODO: Serialize menu group to DBus format … For now, we add empty groups"`, `"TODO: Properly serialize (bool, string, array) tuple"`, `"TODO: Build dictionary of action descriptions"`, `callback(None); // TODO: Parse parameter from message` | The GNOME global-menu D-Bus server **answers every request with empty groups / empty dicts and returns `DBUS_HANDLER_RESULT_HANDLED`**. GNOME shows an empty app menu and the app reports success. Action activation drops its parameter. | **H** |
| 37 | `dll/src/desktop/shell2/linux/wayland/mod.rs:1073` + `linux/x11/mod.rs:4730` | `"TODO: Show native Wayland popup via xdg_popup protocol"` / `"TODO: Show GNOME native menu via DBus"` | `use_native_context_menus` is accepted then ignored on both Linux backends — always the DOM fallback menu. At least it logs and falls back (honest). | **M** |
| 38 | `dll/src/desktop/shell2/windows/mod.rs:513,517,543` | `"Menu bar needs to be extracted from window state"` (`let menu_bar = None;`), `"size_to_content needs to be implemented with new layout API"` (whole block commented out), `"Use monitor_id to look up actual Monitor"` (`Monitor::default()`) | On Win32: **no menu bar is ever created**, `size_to_content` is silently ignored, and the requested `monitor_id` is discarded so windows always open on the default monitor. Mirror gap at `linux/x11/mod.rs:1491` (`let monitor_id = 0; // For now, we default to monitor 0`). | **H** |
| 39 | `dll/src/desktop/extra/permission/ios.rs:30` | `"TODO(P1.2+): issue the matching request<X>Access / native release."` — `handle_event` body is `let _ = event;` | On iOS, **`PermissionDiffEvent` is discarded**: the app can *read* camera/mic/photo status but can never *prompt* for it. Combined with `probe_status`'s `_ => NotDetermined` for Motion/Contacts/Calendars/Notifications/Bluetooth/Biometric/ScreenCapture, most iOS capabilities are permanently unreachable. | **H** |
| 40 | `dll/src/desktop/shell2/android/mod.rs:808` | `"unicode_char mapping (KeyCharacterMap) still TODO"` | Android hardware-keyboard text input produces virtual keycodes but **no characters** — typing into a text field with a physical keyboard does nothing. | **H** |
| 41 | `dll/src/desktop/shell2/macos/mod.rs:3130` | `"TODO: Re-enable once objc2-open-gl feature is properly configured"` | `configure_vsync` computes `swap_interval` then never applies it (msg_send type-encoding issue). macOS relies wholly on CVDisplayLink; `Vsync::Disabled` is unhonoured. | **M** |
| 42 | `dll/src/desktop/shell2/macos/mod.rs:4043` + `:3178` | `menu_state: menu::MenuState::new(), // TODO: build initial menu state from layout_window`, `"For now, use display_id as index (not perfect but reasonable)"` | macOS window opens with an empty menu state (populated only on later updates); display-id-as-index breaks multi-monitor lookup. | **M** |
| 43 | `dll/src/desktop/shell2/windows/mod.rs:4643` | `"For now, return None - will be implemented in phase 1.2"` | `Win32Window::poll_event` collapses every event to `Win32Event::Other` — no typed event is ever surfaced through this API. | **M** |
| 44 | `dll/src/desktop/shell2/linux/wayland/mod.rs:1818,4053` + `:6928` | `"TODO: Window positioning on Wayland"`, `"TODO: Wayland visibility control via xdg_toplevel methods"`, `"set_is_top_level not supported"` | Positioning is a genuine protocol limitation (documented at length — accepted). **Visibility control is not**: `xdg_toplevel` set_minimized/unset_fullscreen exist and are not wired, so show/hide is a no-op on Wayland. | **M** |
| 45 | `dll/src/desktop/shell2/linux/wayland/menu.rs:128` | `"TODO: Implement proper size calculation using system_style font metrics"` | Fallback menu sizing ignores system font metrics → clipped/oversized menu items under non-default DPI or font scale. | **M** |
| 46 | `dll/src/desktop/native_screenshot.rs:89,92,639` | `"Native screenshot not supported on Wayland"`, `"XCB screenshot not yet implemented - please use X11/Xlib backend"` | `take_screenshot` fails on Wayland (the default session on modern Linux) and on the XCB backend. | **M** |
| 47 | `dll/src/desktop/logging.rs:102` | `"TODO: invoke external app crash handler with the location to the log file"` | The registered external crash handler is never called from the panic hook — apps that register one get nothing on crash. | **M** |
| 48 | `dll/src/desktop/shell2/ios/mod.rs:179` + `:1077` | `"applicationWillTerminate: (TODO once we wire lifecycle methods)"` | iOS app lifecycle (terminate/background/foreground) is not wired; no clean shutdown, no pause-on-background. | **M** |
| 49 | `dll/src/desktop/extra/capability.rs:103,105,206,208` | `cap(false, "DXGI duplication", "not yet implemented (stub)")`, `"ReplayKit / MediaProjection"`, `"GCController"`, `"InputDevice (JNI)"` | Screen capture on Windows/iOS/Android and gamepad on macOS/Android have **no backend at all**. The capability report is honest about it (good practice) — the features simply don't exist. | **M** |
| 50 | `dll/src/desktop/extra/video_codec/mod.rs:108,219` | `"the MediaCodec backend is not implemented yet"` (+ `mod.rs:95`: encode is "a counting stub in EVERY build" outside macOS/iOS) | Hardware video **encode** exists only on macOS/iOS+libloading; **decode** on Android is absent. Announced at `open()` rather than yielding nothing (good practice). | **M** |

#### Theme 6 — Core / FFI / ownership

| # | path:line | marker | what is actually missing | impact |
|---|---|---|---|---|
| 51 | `core/src/refany.rs:1291` + `:1311` | `"FIXME: &mut self is exclusive to this clone only, not to the shared RefCountInner — concurrent calls via different clones are a data race."` | `RefAny::set_serialize_fn` / `set_deserialize_fn` write a plain `usize` in shared `RefCountInner` through a `*mut` derived from `&mut self` on a *clone*. Two threads holding clones = **UB data race**. Fix is mechanical (`AtomicUsize`). | **H** |
| 52 | `core/src/transform.rs:293` | `"AUDIT-TODO: USE_AVX/USE_SSE are populated in gpu.rs from a raw CPUID leaf-1 feature bit … On a kernel that didn't XSETBV-enable AVX, using these intrinsics faults with SIGILL."` | The SIMD transform fast path gates on CPUID only, not XGETBV/XCR0. **SIGILL on affected hosts.** Fix named in the comment: `is_x86_feature_detected!`. The `SAFETY:` comments below explicitly lean on the broken gate. | **H** |
| 53 | `core/src/resources.rs:1443-1471` | `"AUDIT-TODO (font GC, resources.rs font leak): Fonts and font instances are currently NEVER garbage-collected … WebRender font memory grows unbounded"` | No `DeleteFont`/`DeleteFontInstance` is ever emitted; the pruning helper `remove_font_families_with_zero_references` is `#[allow(dead_code)]` **with no callers**. Font pickers / editors / live CSS leak. A complete 4-step fix plan is written in the comment. | **H** |
| 54 | `core/src/icon.rs:621` + `:1555` | `"For now, just apply the root node (same as single-node)"` / `"TODO: Full subtree splicing requires inserting nodes into arrays"` | `apply_multi_node_replacement` **discards every node but the root** of an icon replacement. Any icon whose provider returns a multi-node DOM (i.e. essentially every real SVG icon set) renders as a bare root element. Only a `debug_assertions` `eprintln!` warns. | **H** |
| 55 | `core/src/dom.rs:5776-5781` | `"TODO: Implement full XML parsing / For now, just create a text node showing that XML was loaded"` | `Dom::from_xml()` — a public API, even under the `xml` feature — returns `Dom::create_text("XML content loaded (N bytes)")`. It is a fully-formed lie: it does not parse anything. (The real parser is `layout::xml::DomXmlExt::from_xml_string`.) Should be deleted or delegated. | **H** |
| 56 | `core/src/xml.rs:5797` + `:5966` | `"AUDIT-TODO: a worklist-based iterative builder would preserve deep subtrees"` | At `MAX_XML_NESTING_DEPTH` the recursive builder **silently truncates the subtree** (returns the node without children) rather than erroring. Deep markup loses content with no diagnostic. | **M** |
| 57 | `core/src/task.rs:494` | `Self::Tick(_) => unreachable!()` | See "Latent panics" — `into_std_instant()` aborts on tick clocks. | **M** |
| 58 | `core/src/gpu.rs:187` | `"TODO: look up the parent nodes size properly to resolve animation of ..."` | Percentage-valued animated properties can't resolve against the parent size during GPU animation. | **M** |
| 59 | `core/src/diff.rs:1484` | `"For now, we use SizingOnly as a conservative default"` | Diff classification over-invalidates (perf, not correctness). | **L** |
| 60 | `core/src/prop_cache.rs:934` | `"TODO: re-enable css_props pruning once recompute ..."` | Property-cache pruning is disabled → memory growth on long sessions. | **M** |

#### Theme 7 — Clipboard / editing / events

| # | path:line | marker | what is actually missing | impact |
|---|---|---|---|---|
| 61 | `layout/src/window.rs:9154` + `dll/src/desktop/shell2/common/event.rs:3252` | `"styled_runs left empty"` (copy) / `"styled_runs empty — the OS clipboard read only returns plain text"` (paste) | Rich-text copy/paste round-trips as plain text in **both** directions. Honestly documented and plain text is fully wired; the HTML/RTF clipboard format is the missing piece. | **M** |
| 62 | `layout/src/event_determination.rs:373` | `"proper click synthesis requires tracking mousedown target across frames. For now, if left mouse was released and the hover node hasn't changed, emit Click."` | Click is synthesized from *hover equality*, not the mousedown target. Press-on-A, drag-to-B, release-on-A still fires Click on A (correct by luck); press-on-A, release-on-B fires nothing (correct); but a hover change *and back* within one frame misfires. Genuine event-model shortcut. | **M** |
| 63 | `layout/src/window.rs:7047` + `:7051` | `"TODO: Integrate with tooltip manager when implemented"` / `"TODO: Allow custom action handlers"` | `AccessibilityAction::ShowTooltip`/`HideTooltip`/`CustomAction` are accepted from the a11y layer and **silently dropped**. Screen-reader tooltip requests and every custom a11y action do nothing. | **M** |
| 64 | `layout/src/widgets/node_graph.rs:3183` | `Update::DoNothing // TODO` | `nodegraph_duplicate_node` downcasts its dataset then returns `DoNothing`. The node-graph widget's **Duplicate command is a dead button**. | **M** |
| 65 | `layout/src/managers/undo_redo.rs:57` | `"For now, we store the logical position, not the TextCursor"` | Undo/redo restores a logical offset, not the full cursor (affinity, multi-cursor) → cursor jumps after undo in bidi/wrapped text. | **M** |
| 66 | `layout/src/default_actions.rs:134` | `"For now, no action (form handling could be added later)"` | Form submit default action is a no-op. | **L** |

#### Theme 8 — Widgets (the `TODO2` convention)

`layout/src/widgets/` uses a **`TODO2` marker** for "documented, deliberate widget limitation":
**38 occurrences across 15 files**. This is a *good* convention — each one states what is missing and
why — but the aggregate is a real feature gap in the widget set:

| widget | file | limitation |
|---|---|---|
| Toast | `layout/src/widgets/toast.rs:14` | `"auto-dismiss is intentionally NOT implemented (be honest, don't fake it)"` — needs a host timer |
| Spinner | `layout/src/widgets/spinner.rs:8` | `"## PARTIAL — STATIC ONLY (no spin animation)"` |
| Combobox | `layout/src/widgets/combobox.rs:23,35` | type-to-filter NOT implemented; list at a fixed offset, no collision-aware placement |
| Modal | `layout/src/widgets/modal.rs:16` | esc-to-close, backdrop-click-close, focus trap, real z-order not reachable from a widget |
| Date picker | `layout/src/widgets/date_picker.rs` (4) | month nav fires `on_change` but **cannot rebuild the grid in-widget** |
| Text area | `layout/src/widgets/text_area.rs:15,89` | core multi-line only; the cursor is a static child that does not track the caret |
| Tooltip / Popover | `tooltip.rs:16`, `popover.rs:16` | CSS simplification of a floating popover — no collision detection, no portal |
| Accordion | `accordion.rs:13` | no animated disclosure |
| Split pane | `split_pane.rs:30` | continuous drag not verifiable headlessly |
| Avatar | `avatar.rs:10` | circular crop depends on `overflow:hidden` + `border-radius` behaving |
| Alert / Chip | `alert.rs:424`, `chip.rs:448` | dismissal is `display:none` toggling, node stays in the tree |

Impact: **M** overall — the widget set advertises components that are visually right but
behaviourally partial. Ranked below the engine gaps because each is individually documented.

#### Theme 9 — Codegen / bindings / web

| # | path:line | marker | what is actually missing | impact |
|---|---|---|---|---|
| 67 | `css/src/codegen/cpp.rs:21` + `css/src/codegen/python.rs:22` | `"// TODO: C++ codegen backend not yet implemented."` / `"# TODO: Python codegen backend not yet implemented."` | `backend_for()` dispatches to emitters that return a one-line TODO comment as their entire output. A caller asking for C++/Python CSS codegen gets a file containing a comment, **not an error**. | **M** |
| 68 | `doc/src/codegen/v2/lang_python.rs:1328` | `"// TODO: callback type conversion"` + emitted `unimplemented!(…)` | `Option<Callback>` enum variants cannot be constructed from Python; the generated static method panics. See "Latent panics". | **M** |
| 69 | `doc/src/patch/parser.rs:790` | `"TODO: This logic for replacing the enum leaf is incomplete and might not"` | The api.json patch parser can mis-apply enum-leaf replacements — silently produces a wrong `api.json`, which feeds every language binding. | **M** |
| 70 | `dll/src/web/transpiler.rs:127,137,147` | `"transpiler not yet implemented (Phase 0 stub — callbacks run server-side)"` | The `StubTranspiler` `is_available() -> false` and every lift returns `Err`. The web backend therefore runs **all callbacks server-side** (a round trip per event). Honest and explicit; the real lifter is behind `transpiler_remill`. | **M** |
| 71 | `dll/src/web/server.rs:353` + `:393` | `"The JSON {x, y, button, key} payload is currently ignored"`, `"For now, serve the template HTML (Phase 0 limitation)"` | The web event endpoint discards the actual event coordinates/key. | **M** |
| 72 | `dll/src/web/transpiler_remill.rs:6601,7909` | `"aarch64-ONLY: the trace block below hardcodes the X1 slot (560) and ring band"`, `"the aarch64-hardcoded 0x4000000 never matched on …"` | The lifter's trace/relocation constants are hardcoded per-arch; x86 self-lift is `WIP` (matching `.github/workflows/dockery.yml:14`: `"self-lift is still WIP, so the amd64 image is best-effort"`). | **M** |
| 73 | `examples/cpp/cpp{11,14,17}/opengl.cpp:23`, `examples/python/opengl.py:16` | `Dom placeholder = Dom::create_text("OpenGL texture would render here")` | The OpenGL example in **C++ (×3) and Python** draws a grey box instead of a texture. The OpenGL FFI surface is therefore unexercised outside Rust — a binding regression there would ship undetected. | **M** |
| 74 | `examples/c/browser.c:530` | `"TODO: Fetch and parse external CSS"` | The C browser demo ignores `<link rel=stylesheet>`. | **L** |

---

### Accepted simplifications (noted, not debt)

These matched the marker list but are **deliberate, reasoned, and correctly announced**. Several are
exemplary and worth preserving as house style:

1. **`layout/src/e2e/runner.rs:1964-2331` — "NOT SUPPORTED HEADLESSLY".** Every
   `CallbackChange` the headless runner cannot apply is listed **explicitly, with no `_` arm**, so a
   new variant is a compile error, and `Runner::unsupported()` **fails the scenario by name** rather
   than logging. The comment states exactly why: *"a change that is silently ignored produces a test
   that executes nothing and PASSES, which in a generated corpus is indistinguishable from a real
   pass."* This is the single best pattern in the tree; gaps #1/#2 (paged display-list `=> None`)
   are the same shape done wrong.
2. **`layout/src/e2e/full.rs:6191` — `UNIMPLEMENTED_CROSS` invariants** fail loudly
   (`"invariant {c} is NOT IMPLEMENTED and will not be silently passed"`) instead of vacuously
   passing. Same philosophy.
3. **`dll/src/desktop/extra/video_codec/mod.rs:102` — `decode_engine_missing_reason()`.** A decode
   handle that can never produce output says so at `open()`: *"A handle that opens fine and then
   yields nothing forever is indistinguishable from 'no data yet'."* Same for
   `dll/src/desktop/extra/capability.rs` which reports `available=false` with a reason string.
4. **`layout/src/solver3/pagination.rs:760`** announces the varying-content-width limitation once at
   runtime. The gap is real (#4) but the *disclosure* is right.
5. **`dll/src/desktop/shell2/linux/wayland/mod.rs:1818`** — window positioning. Wayland genuinely
   has no protocol for it; the 9-line comment explains the protocol rationale and the
   `wl_surface enter/leave` mitigation. Not debt.
6. **`layout/src/window.rs:3908` `tick_timers`** — "For now, we'll just collect all timer IDs" looks
   like a gap but is by design: readiness is decided by `Timer::invoke`, documented at
   `e2e/runner.rs:557` and **pinned by a negative-control test**
   (`window.rs:11148 tick_timers_reports_every_registered_timer_regardless_of_the_clock`).
7. **`css/src/props/style/{transform,filter,background}.rs` `_ => unreachable!()`** — each has a
   dedicated test proving the arm is unreachable for near-miss stopwords
   (`transform.rs:1254`, `filter.rs:1071`). Accepted, though fragile (see Latent panics).
8. **`css/src/props/style/content.rs:13`** — *"Intentionally simplified: stores the unparsed CSS
   value"*, with the rationale stated.
9. **`core/src/xml.rs:5795`** — the recursion depth cap itself is a correct hardening
   (stack-overflow prevention); only the *silent* truncation (#56) is the gap.
10. **The `#[allow(dead_code)]` population (38)** is largely legitimate: test-only structs
    (`core/src/refany.rs:1631,2537,2806`), feature-gated platform shims, and ABI-padding fields
    (`core/src/gl.rs:5602`: *"the field only exists to give the vertex a realistic size/layout"*).
    The one that **is** debt is `core/src/resources.rs:1471` — the uncalled font-GC pruner (#53).
11. **`layout/src/solver3/layout_tree.rs:3381-3386`** — `display: run-in` falling back to block
    explicitly *"matches browser behavior"*. Correct call.
12. **The `hard.?coded` marker is mostly a changelog.** ~55 of 92 code hits read *"was hardcoded —
    fixed"* (the `MWA-C-*` / seam-audit annotations, e.g. `dll/src/desktop/extra/geolocation/*.rs`
    `"was hardcoded 0"`, `core/src/events.rs:3020` `"hardcoding Ctrl here made Cmd+C dead on
    macOS"`). Treat this marker as *evidence of past remediation*, not present debt.

---

### Vendored `webrender/` + `third_party/` (excluded from counts)

235 tracked files ≤ 1 MiB. **937 marker matches total** — upstream Mozilla WebRender/swgl debt, not
azul's.

| marker | count | | marker | count |
|---|---|---|---|---|
| `TODO` | 311 | | `should be` | 111 |
| `unreachable!(` | 74 | | `assum` | 64 |
| `for now` | 63 | | `temporar` | 52 |
| `placeholder` | 52 | | `workaround` | 45 |
| `allow(dead_code)` | 30 | | `approximat` | 27 |
| `simplif` | 22 | | `in the future` | 20 |
| `FIXME` | 16 | | `follow.up` | 13 |
| `unsupported` | 10 | | `stub` | 8 |
| `left as` | 5 | | `not implemented` | 4 |
| `panic!("not` | 2 | | `revisit` / `hardcoded` | 2 / 2 |
| `unimplemented!(` | 1 | | `XXX` / `naive` / `does n't handle` | 1 each |
| `#[ignore]` / `#[cfg(FALSE)]` / `HACK` / `WIP` / `todo!(` / `best-effort` | **0** | | | |

Concentration is entirely upstream-shaped: `webrender/core/src/picture.rs` (42 TODO/FIXME),
`webrender/core/res/cs_svg_filter_node.glsl` (33), `renderer/mod.rs` (22), `prepare.rs` (20),
`scene_building.rs` (17). No azul-authored TODO was found inside the fork
(`rg 'TODO.*azul|azul.*TODO' webrender` → 0 hits), so the fork's delta is not marker-annotated —
worth noting separately: **the azul-specific changes to the vendored WebRender are undocumented in
comments**, which makes rebasing onto upstream a blind diff.

`third_party/` contains exactly one tracked file ≤ 1 MiB (`WEB_LIFTER_INSTALL.md`) with a single
`assum` match. Nothing to report.

---

### Summary of counts

| | code files | prose files | vendored |
|---|---|---|---|
| files swept | 1316 | 594 | 235 |
| total marker matches | **3736** | **2192** (1826 of them in `scripts/` planning docs) | **937** |
| estimated signal-bearing | ~250–300 | ~40 | n/a (upstream) |
| **real functional gaps triaged** | **~60** (74 numbered entries above incl. grouped duplicates) | — | — |
| `todo!()`/`unimplemented!()` in shipped code | **0** (3 latent codegen emitters) | — | 1 (upstream) |
| `#[ignore]`d tests | **11** (all `dll/`, all with hardware reasons) | — | 0 |
| `#[cfg(FALSE)]` / commented-out tests | **0** | — | 0 |

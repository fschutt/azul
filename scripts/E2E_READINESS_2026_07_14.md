# E2E Readiness — 2026-07-14

Audited against `master` = **`fe8165f57`**, working tree as committed. Every claim below was
re-derived from source; where the brief's framing was wrong, it is corrected in place and the
correction is marked **[CORRECTION]**.

Two questions are answered:

1. **What blocks running `gen-e2e` at scale (§3).**
2. **How we structurally prevent the bug classes we keep finding (§4)** — the part that matters.

TL;DR of §3: **nothing blocks generation itself.** The generator, the schema parse, the op
denylist, the zombie gate and the incremental/content-addressed expander all work. What blocks a
*useful* 13k run is four things, in order: **(1) the generator is blind to the two new assertion
surfaces and to the mock fonts; (2) `assert_state_machines_idle` — which 22 corpus lines ask for —
does not exist; (3) there is no xfail marker, so the blocking CI gate is red on master right now
and cannot absorb a single known bug; (4) the runner is one OS process per JSON in a serial bash
loop, i.e. ~7 h for the corpus against a 30-min job cap.** All four are cheap. The *expensive*
work is §4.

---

# PART 1 — Reconciliation of the existing plans

165 `.md` files live in `scripts/`. These are the ones that still bear on e2e, and their real
status at `fe8165f57`:

| Doc | Status | What is left |
|---|---|---|
| `E2E_PLAN.md` | **PARTLY DONE — Phase 0/1 are essentially complete.** Its own status line ("plan / not implemented") is now **STALE**. | Of its four "must be added to azul": `mount` op ✅, `FrameReport`/queryable damage ✅ (`layout/src/window.rs:421`), frame-work counters ✅ (`assert_work_bounded`), resource counters ✅ (`assert_resource_counts`), auto-baseline killed ✅, virtual clock ✅ (`tick_ms`). **STILL OPEN:** `assert_state_machines_idle`, `assert_manager_invariants`, `assert_composition`, `assert_damage_sound` — four assertions the plan designed, the corpus references, and nobody implemented (§3.F2). Its §D sharding advice ("one process, many files") is **unimplemented and is now the #1 runtime blocker** (§3.D). |
| `E2E_PROTOCOL_AUDIT.md` | **PARTLY STALE — it audits `983357ebd`, and its top three recommendations have since landed.** | Fixed since: the 5 zombie ops (§2.A) are implemented (`full.rs:7748/7766/7780/7798/8484`); `FrameReport`/`capture_damage_png`/`tick_ms`/`mount` landed (its §3-O2). **STILL OPEN, and still correct:** H1 (`UpdateImageCallback` damages nothing — §3.E2 below, *verified still broken*), H3 IME preedit, H4 file drop, H5 clipboard inject/readback, H8 WM frame state, H9 theme, H10 pen eraser/barrel, H11 scroll source, H13 monitors, H14 mouse-leave; outputs O3–O8 (cursor icon, IME rect, clipboard, window requests, `ProcessEventResult`, monitors). Its §2.D "pollution" point is now *enforced* rather than fixed — the 24 IDE ops are still in the enum but `OP_POLICY` (`doc/src/gene2e.rs:258-388`) denies them to the generator. |
| `DAMAGE_REGION_PLAN.md` | **PARTLY DONE.** Its own 2026-07-02 status header is accurate. | The CPU half (§12-P2/P7) shipped on all four backends. **STILL OPEN:** §4 layout-level damage source (damage is still derived post-hoc by display-list diff — this is what makes §3.B possible), §5 `css_property_damage`, GPU partial present / buffer-age (§12-P5), Wayland fractional scale. The e2e assertions assert *outcomes*, not mechanism, so none of this blocks generation (the plan says so itself, F8). |
| `DEBUG_API.md` | **STALE as documentation** (2026-06-02; predates ~30 ops, all the new assertions, `mount`, `tick_ms`, `FrameReport`). Not load-bearing — the generator reads `full.rs` directly, not this doc. | Rewrite from `parse_schema()`'s output, or delete. Do **not** hand it to a new session as truth. |
| `SPEC_CONFORMANCE_REVIEW.md` | **DONE as an artifact** (2026-07-13, `d11181c67`). Not an e2e input. | It is a *reftest*/CSS-correctness backlog, explicitly out of e2e scope (`E2E_PLAN.md` §0.1). Keep it out of the generator's way. One incidental finding worth carrying: `azul-doc spec show` does not exist and every `+spec:` hash has drifted (0 known / 1237 unknown). |
| `MANAGER_WIRING_AUDIT_2026_07_03.md` | **PARTLY STALE — but it is still the best map of behavioural holes, and §3.G below depends on it.** It audits a *different tree* (`azul-mobile` @ `30162fa9f`) and predates the damage-wiring merge. Spot-check: its "hover: `:hover` restyle never applied on ANY backend / DEAD" is **now false** — `apply_hover_restyle` exists (`dll/src/desktop/shell2/common/event.rs:520`, called at `:4389`). | Its remaining DEAD rows (gesture pinch/rotate, clipboard, a11y, gamepad, drag-source, changeset, gpu_state drain) predict where generated tests will go red *for real reasons*. See §3.G — the good news is those corpus categories are tiny. **Needs a re-audit against `fe8165f57` before anyone trusts a red-test triage against it.** |
| `DEFERRED_CASCADE_DESIGN.md` | **STILL OPEN — and it is the design that fixes §3.E1.** | The inserted-node-no-author-cascade bug (the RED test blocking the release) is exactly the problem this doc frames. It is not a prerequisite for generation, but it is the correct long-term fix (§4.B). |
| `FONT_INVALIDATION_AND_MEMORY_LAYOUT_ANALYSIS.md` | **STALE in a dangerous way.** It documents `font_stacks_hash` as the mechanism that skips font re-resolution. | **`LayoutWindow::font_stacks_hash` (`layout/src/window.rs:753`) is a DEAD FIELD**: it exists in exactly three places — the declaration, `: 0` in the constructor (`window.rs:881`), and `font_stacks_hash: _` in the exhaustive destructure (`window.rs:8164`). It is **never written and never read.** So the optimisation this doc describes is off; worse, if someone wires the *read* without the *write* it will report "fonts unchanged" forever. This is a §4.B specimen: the destructure *classified* it as exempt, and "exempt" is precisely where a should-recompute field goes to hide. |
| `INCREMENTAL_LAYOUT_ARCHITECTURE.md` / `DOM_CHANGE_REPORT_ARCHITECTURE.md` | **STILL OPEN.** | Both describe the granular invalidation that would make `is_layout_equivalent` unnecessary. Not a Friday blocker. They are the strategic answer to §4.B; §4.B proposes the *tactical* compile-time guard that works without them. |
| `CSS_STYLESHEET_COLLAPSE_PLAN.md` | **STILL OPEN**, and orthogonal. | No e2e impact. |
| `PLATFORM_INTEGRATION_AUDIT.md` | **PARTLY STALE**, and out of scope. | Headless-only e2e does not touch camera/sensors/video. Ignore for Friday. |

---

# PART 2 — What actually landed (verified against `git log`, not the brief)

All confirmed present at `fe8165f57`:

| Commit | What it really did |
|---|---|
| `3c5cad6fe` | **`NodeIdRemap` trait** (`layout/src/managers/mod.rs:148-151`) + `LayoutWindow::remap_node_ids` (`layout/src/window.rs:8101`), whose `let Self { … } = self;` (`:8104-8175`) has **no `..`** — a new field is an `E0027` compile error until it is classified. Doc at `:8080-8100`. 10 of 22 managers implement it. |
| `0488d6a23` | The mechanism paying for itself within hours: `frame_report` was a new field and did not compile until classified exempt. |
| `3a0350ac1` | `mount` op, `FrameReport` (`layout/src/window.rs:421`), `tick_ms`, damage-PNG capture, **`assert_screenshot` auto-baseline removed**. |
| `ac4fc38f2` / `338f096bd` / `2c6de2068` / `3821af0c5` | `azul-doc gen-e2e` (`doc/src/gene2e.rs`, 1966 lines): schema parsed **at runtime from `full.rs`** (`parse_schema`, `:431`), content-addressed incremental generation (`--prune`), explicit `OP_POLICY` denylist (`:258-388`, unclassified ⇒ denied **loudly**, `:393-397`), and the **zombie gate** (`is_zombie`, `:203-207` — declared in `DebugEvent`, no `DebugEvent::X` match arm ⇒ refuse to generate). |
| `fe981ccd5` | 13,223-line corpus (`scripts/E2E_TESTS.txt`) from `scripts/gen_e2e_cases.py`, 161 categories. |
| `85145ba62` | api.json 198 → 254 fns. |
| `6499f0c47` / `f1b18069a` / `87b260540` | Three engine bugfixes + four RED-first tests in `e2e/`. **Three of the four are now green** (fixed by `6499f0c47`/`2bb6909ba`); `bug-inserted-node-no-author-css.json` is the one still red — see §3.E1. |
| `d4f772f29` | **BLOCKING** `e2e_headless` CI gate (`.github/workflows/rust.yml:1987-2074`), in `deploy_pages.needs` (`:3105`). |
| `2bb6909ba` | Failed font-family match is no longer silent (`ResolvedFontChains.unresolved_families` / `.last_resort_chains`, `layout/src/solver3/getters.rs:3445/3449`) + **mock fonts, auto-registered** (`layout/src/text3/mock_fonts.rs`, registered from *every* `FontManager` ctor — `layout/src/text3/cache.rs:760/833/969`). |
| `f53048778` / `da5a65317` | The five ex-zombie ops implemented (`focus`/`blur`/`move`/`dpi_changed`/`get_dom`) + `assert_response`. All five are now non-zombie **and** allowed by `OP_POLICY` (`gene2e.rs:280-286`), so they auto-appear in the prompt. |

---

# PART 3 — The readiness checklist

Ordered by *what blocks a useful run*, not by size. Effort is one engineer, focused.

## A. The generator is blind to the mock fonts — **TRUE, but for a different reason than stated** [CORRECTION]

**Confirmed:** `doc/src/gene2e.rs` (all 1966 lines) contains **zero** occurrences of
`font-family`, `Azul Mock`, `Arial`, `serif`, `@font-face` — the prompt (`build_prompt`,
`gene2e.rs:694-780`) never mentions fonts, and for a font corpus line it just says *"invent
plausible, minimal HTML+CSS for it"* (`:740`). The corpus (`scripts/E2E_TESTS.txt`) names **no
concrete family either**: 322 lines say *"a text node whose font-family is unique to it"*
(from `gen_e2e_cases.py:142`), 0 lines mention Arial/Helvetica/…, 0 mention `Azul Mock`. So the
model free-invents family names. That much of the brief is right.

**Two parts of the brief are wrong:**

1. **Unmatched families do NOT "resolve to nothing".** `ensure_chains_nonempty`
   (`layout/src/solver3/getters.rs:4086-4124`) takes `fc_cache.list().first()` and gives that
   **same arbitrary `FontId`** to every chain that matched nothing. Text renders — in an
   arbitrary font, with every unmatched family collapsed onto one id. That collapse is the
   *actual* hazard: it makes the **322 `unique-font` lines and the `leak/font` cases
   (`gen_e2e_cases.py:749-750`) vacuously green** — the exact failure mode `2bb6909ba` was
   written to kill. (Generic families — `sans-serif`/`monospace`/`serif` — resolve properly;
   only *named* families miss.)
2. **The "HELLO@20px = exactly 50px" metric is irrelevant to `gen-e2e`.** Generated tests
   **cannot assert geometry at all**: `assert_layout` is on the denylist
   (`gene2e.rs:383` — *"geometry — azul-doc reftest owns layout correctness"*), restated in the
   prompt (`:722-726`) and enforced by `validate()` (`:1607-1608`). The exact metrics are for
   *hand-written* tests (`e2e/mock-font-exact-metrics.json`) and for reftest — not for this fleet.

**Also wrong in the brief:** the mock fonts need **no** loading step. `register_builtin_mock_fonts`
(`layout/src/text3/cache.rs:924-932`) runs in every `FontManager` constructor. Naming
`Azul Mock Mono` / `Azul Mock Wide` in CSS is sufficient — no op, no `@font-face`, no path.

**Fix (do it):** add ~6 lines to `build_prompt` (`gene2e.rs:694-780`):

> *"For any test involving text: use only `Azul Mock Mono` (0.5em advance) or `Azul Mock Wide`
> (1.0em advance) — they are built in and always resolve. When a case needs N **distinct**
> families, invent N distinct names but ALWAYS list a mock font as the last fallback, e.g.
> `font-family: MyFakeFamilyA, "Azul Mock Mono";`. Never name a system font (Arial, Helvetica,
> Times, Courier, Verdana) — on the CI box they match nothing and collapse onto one shared
> FontId, which makes font-identity and leak assertions vacuous."*

Optionally also steer `gen_e2e_cases.py:142` to say *"…whose font-family is unique to it (use a
distinct invented name with `Azul Mock Mono` as fallback)"* and regenerate the corpus — but the
prompt change alone is enough, and the corpus is content-addressed so a regeneration is not free.

**Effort: 1 h** (prompt only; regenerate 0 tests, the change only affects *future* generations —
which is all of them, since `e2e/gen` is empty today).

## B. `resize` reports `FrameDamage::None` — **REFUTED at HEAD. Do not "fix" this.** [CORRECTION]

*(I initially confirmed this claim by reading the skip-arm in isolation, and I was wrong. The
correct reading, traced end-to-end:)*

`dll/src/desktop/shell2/headless/mod.rs`, `CpuBackend::render_frame`:

1. `needs_resize` is computed at **`:314`** (`old_pw != pixel_w || old_ph != pixel_h`).
2. **`:376-386` — the display-list diff is short-circuited by `needs_resize`:**
   ```rust
   let dl_damage = match &self.previous_display_list {
       Some(old_dl) if !needs_resize && !gpu_damage.needs_full => { compute_display_list_damage(..) }
       _ => None,   // :385 — "first frame, resize or ref-frame transform → full repaint"
   };
   ```
   So on **any** resize, `dl_damage` is **`None`**.
3. The "Nothing changed — skip rendering entirely" early-out at **`:489-506`** is guarded by
   `Some(rects) if rects.is_empty() && …`. **`None` cannot match it.** It is **unreachable on a
   resize.**
4. Control therefore falls to the `_` arm (`:513`) ⇒ `is_incremental = false` ⇒
   **`last_present_damage = FrameDamage::Full`** (`:745`, `:754`).

`dpi_changed` shares the path exactly (`full.rs:7798-7821` → `event.rs:1425-1466`:
`size_changed || dpi_changed → RelayoutReason::Resize + mark_frame_needs_regeneration()`), so it
inherits the *correct* behaviour, not a bug.

**Where the claim came from:** prose in the `description` field of `e2e/op-dpi-changed.json`
("a surface-SIZE change currently reports FrameDamage::None … it needs its own bug-*.json
scenario"). **There is no such `bug-*.json`, no red test, no unit test, and no TODO in the code.**
It is an unverified assertion that has been repeated until it sounded settled. There is also no
resize-damage assertion anywhere: `simulate_resize` (`headless/mod.rs:1108`) has none.

**So the 387 `[resize/*]` lines and the 96 `[dpi/*]` lines are NOT pre-doomed.** Do not spend the
day before a 13k run rewriting a damage path that reads correct.

**What to do instead — settle it empirically, cheaply.** Write **one** scenario,
`e2e/resize-damage.json`: mount a fixed-size box, `reset_frame_counters`, `resize` (grow), assert
damage is non-empty; `resize` (**shrink** — the one path where `resize_damage` genuinely stays empty
at `:337-340` because the compositor is recreated wholesale, so it is the only place worth
suspecting), assert damage is non-empty. If both are green, the category is clear and you have a
permanent regression test for free. If the shrink is red, *then* fix it (populate `resize_damage`
with the full new viewport rect in the recreate branch, `:337-340`).

**Effort: 1 h to write the test. 0 h of engine work unless it goes red.** This entry is a
reminder of the discipline the rest of this document argues for: **an unverified claim in a
description field is exactly the kind of thing an executable assertion exists to kill.**

## C. "A debug-linked host cannot run" — **REFUTED as stated. Local iteration is not blocked.** [CORRECTION]

`cabi_export` is a **Cargo feature**, not a profile or a `cfg`:

- `dll/Cargo.toml:510` — `cabi_export = ["cabi_internal"]`.
- `dll/Cargo.toml:528-529` — **`build-dll = ["cabi_export", …]`**. Also `link-static` (`:576-577`),
  which is in `default`.
- The `#[no_mangle]` is emitted as `#[cfg_attr(feature = "cabi_export", no_mangle)]`
  (`doc/src/codegen/v2/config.rs:459-471`), 13,918× in `target/codegen/dll_api_internal.rs`.
- **There is no `debug_assertions` check and no `PROFILE` check anywhere on the export path**
  (grepped `dll/build.rs`, `dll/src/lib.rs`, `doc/src/codegen/v2/lang_rust.rs`,
  `target/codegen/dll_api_internal.rs` — zero hits). `restrict_cdylib_exports()`
  (`dll/build.rs:75-79, 123-185`) is profile-agnostic and is a **deliberate no-op on Linux**
  (`:160-184`).
- `AZ_LINK_PATH` is read only by `dll/build.rs:645` (`configure_dynamic_linking`), and only for
  the **`link-dynamic`** consumer; it returns immediately if `CARGO_FEATURE_CABI_INTERNAL` is set
  (`:625-627`).

So `cargo build -p azul-dll --features build-dll` in the **debug** profile exports the same ~13.9k
`Az*` symbols as release. (Empirically: `target/release/libazul.so` has 12,759 defined dynsyms;
there is **no `target/debug/libazul.so` in this tree at all** — nobody has actually built one, which
is consistent with the symptom having come from somewhere else.)

**The real cause of `undefined symbol: AzU8Vec_delete` with 4 dynsyms** is almost certainly a
**feature-selection** mistake: building the *dll* with `--features link-dynamic` (or
`--no-default-features`), which selects `cabi_external` (`dll/Cargo.toml:583-590`) — the
**extern "C" declaration** side. That crate emits *no* `#[no_mangle]` bodies by design; it expects
to link against a `libazul.so` built elsewhere. Two crates, two feature sets:

```bash
# the LIBRARY (exports symbols) — any profile works:
cargo build -p azul-dll --features build-dll                 # → target/debug/libazul.so
# the HOST (imports symbols):
AZ_LINK_PATH=$PWD/target/debug \
cargo build -p azul-examples --example hello-world --no-default-features --features link-dynamic
```

**Verify this in 10 minutes before you plan around it** (`nm -D --defined-only
target/debug/libazul.so | wc -l` must be ~13k). If it *does* come back as 4, the mechanism is
something I did not find and this becomes a real blocker — but the static evidence is
unambiguous, and the fastest local loop is anyway to keep using `--release` (`[profile.release]`
already has `debug = 1, strip = false`, `Cargo.toml:84-88`, so you get symbols and a debugger).

**Effort: 0.5 h to verify; 0 h to fix if my reading is right. Document the two commands in the
handoff (§5) so nobody re-derives this.**

## D. No sharded runner — **TRUE. This is the hard runtime blocker.**

Today (`.github/workflows/rust.yml:2044-2066`): **one OS process per JSON file, in a serial bash
`for` loop, on one `ubuntu-22.04` runner, `timeout-minutes: 30`.** No matrix, no parallelism. 10
files today. At 13,223 files × (process spawn + headless boot + font-cache init + several explicit
`{"op":"wait","ms":150}` sleeps) ≈ 1–5 s each ⇒ **~7 hours serial.** Against a 30-minute cap.

**The lever needs no engine change.** `AZ_E2E` already accepts a **JSON array of tests** in one
file (`dll/src/desktop/shell2/run.rs:82-92`), and the runner loops them. So:

**Proposed shape**

1. **Batch.** A build step concatenates each shard's JSONs into one array file ⇒ **one process, one
   warmup, N tests.** Batch size 200. (`scripts/e2e_batch.py`, ~40 lines.)
2. **Two lanes, enforced in the schema.** Add `"isolated": true` to the test object. The e2e host
   has **no per-test sandbox** — tests in one process share window, manager and resource state, so
   **every leak/resource test must be its own process.** `[leak/*]`, `[callback/leak]`,
   `[*/leak]`, `[focus/leak]`, `[clipboard/leak]`, `[virtualview/leak]` ≈ **250 lines** ⇒ 250
   processes, one lane. The other ~13k batch at 200/process ⇒ **~65 processes**.
3. **Shard.** GitHub matrix `shard: [0..7]`, `shard_index % 8`. Reuse the existing
   `build_dll_e2e` artifact (the `e2e_headless` job already downloads it, `rust.yml:2024-2038`) —
   no extra DLL build.
4. **Budget.** 65 batched processes + 250 isolated ⇒ ~315 process spawns; across 8 shards ≈ 40
   spawns/shard. At ~2 s spawn + ~5 ms/step ⇒ **well under 15 min/shard.** That fits, with room.
5. **Do NOT put this in `e2e_native`** (`rust.yml:2080`) — that is the 3-OS × 27-toolchain binding
   matrix, already at a 60-min cap.

**Effort: 1 day** (batcher + `"isolated"` flag + a `shard` matrix on `e2e_headless`).

**Blocker within the blocker (§4.C.2 preview):** the font-diagnostic dedupe
(`report_unresolved_families`, `layout/src/solver3/getters.rs:4361`) uses a **process-global
`static OnceLock<Mutex<BTreeSet>>`**. In a 200-tests-per-process batch, **only the first test in
the process ever sees the warning.** Any diagnostic you add must be **per-frame/per-test state on
`LayoutWindow`**, not a process-global. Fix this in the same PR or batching silently blinds the
diagnostics.

## E. Known-red engine bugs — both confirmed at HEAD. **Decision: fix E1, pin E2.**

**E1 — inserted node gets no author cascade. CONFIRMED, and it is currently failing CI.**
Root cause is not subtle: `StyledDom::create` cascades the stylesheet once and then **throws the
rules away** — `core/src/styled_dom.rs:1084`, literally `drop(css);`, with a comment justifying it
as a ~500 KiB saving. The XML/`mount` path routes all of its CSS through exactly that channel
(`layout/src/xml/mod.rs:218-227`), so after the parse **nothing in the process holds the rules** and
no one can re-run the cascade for a node created later. An inserted node gets UA defaults +
inheritance only. Test: `e2e/bug-inserted-node-no-author-css.json`.
*It is not a structural impossibility* — the `Dom`-API `.with_css()` path retains rules per node
(`core/src/dom.rs:1551`). The fix is to keep the cascaded rules (or a compact form) on the
`StyledDom` and re-run restyle for inserted nodes. This is a §4.B specimen, and
`DEFERRED_CASCADE_DESIGN.md` is its long-form design.

**E2 — `UpdateImageCallback` marks nothing dirty. CONFIRMED at HEAD** (the line moved; it is now
`dll/src/desktop/shell2/common/event.rs:1712-1718`):

```rust
CallbackChange::UpdateImageCallback { dom_id: _, node_id: _ } => {
    ProcessEventResult::ShouldReRenderCurrentWindow
}
CallbackChange::UpdateAllImageCallbacks => {
    ProcessEventResult::ShouldReRenderCurrentWindow
}
```

It ignores `dom_id`/`node_id`, marks no node dirty, adds no damage and does not regenerate the
display list. A callback image's display-list item is **byte-identical** across re-invocations (that
is the design — it is keyed by `ImageRefHash`), so `compute_display_list_damage` yields empty and
`render_frame` takes the skip branch (`headless/mod.rs:489-506`). Its sibling `ChangeNodeImage` had
exactly this bug and was fixed by calling `lw.regenerate_display_list_for_dom(*dom_id)`, with a
comment (`event.rs:1692-1710`) that spells out the failure mode verbatim. `UpdateImageCallback`
never got that fix.

**[CORRECTION] "frozen on all six platforms" is overstated, and the precision matters.** The four
desktop shells **do** re-invoke the callbacks every paint — `windows/mod.rs:885`, `macos/mod.rs:6114`,
`x11/mod.rs:3583`, `wayland/mod.rs:4465` all call
`lw.invoke_cpu_image_callbacks(&OptionGlContextPtr::None)`. So fresh pixels **are** produced there;
they are just never damaged or composited. The bug is in the invalidation, not the callback.
**Headless, Android and iOS do not call it at all** (grep `invoke_cpu_image_callbacks` in
`headless/mod.rs`: zero hits — it only *consumes* stale results at `:687`). So headlessly the
callback **never runs**, and the whole node type is structurally invisible to the suite. Fixing E2
properly is therefore two changes, not one: damage the node's rect in `event.rs:1712`, **and** call
`invoke_cpu_image_callbacks` from the headless loop (impl at `layout/src/window.rs:1834`).

**Decision, and I would push back on framing this as "fix first or let them be red":**

- **E1: fix it.** It is *already* red in a **blocking** gate that is in `deploy_pages.needs`
  (`rust.yml:3105`) — master cannot deploy today. And "an inserted node has no author CSS" will
  poison a large slice of the `[mutate/*]` (≈2,400 lines) and `[css/*]` categories with a *second*
  failure signature layered on whatever they were meant to test. Fix before generating.
- **E2: do NOT fix before Friday. Pin it.** No corpus category exercises image callbacks
  meaningfully (`[leak/image]` = 1 line), and the fix is two changes in the engine's damage
  contract — not something to rush the week you are generating 13k tests. Keep
  `e2e/bug-*.json` red **but marked** (see below).

**E-prime — there is NO xfail mechanism, and this is the real problem.** Grep for
`expect_fail|xfail|known_red|expected_failure` across `e2e/*.json`, `full.rs`, `run.rs`: **zero
hits.** The gate is `for f in e2e/*.json; … failed=1; done; exit "$failed"`
(`rust.yml:2044-2069`). So **a known bug cannot be checked in without breaking a blocking gate** —
which is why master is red right now. At 13k tests this is fatal: you will have a hundred real,
triaged, not-yet-fixed bugs, and your only options will be "delete the test" (bug-enshrinement by
another name) or "gate permanently red" (a gate nobody reads).

**Add `"expect": "fail"` to `E2eTest`** (`full.rs:3218`), and make the runner treat it as a
**two-way** assertion: an `expect: fail` test that *passes* is a **FAILURE** ("this bug is fixed —
delete the marker"). That is the difference between a quarantine (rots) and a pin (a regression
detector in both directions). **Effort: 3 h.** This is the single highest-leverage item in §3.

## F. Other things I found

**F1 — `assert_response` is invisible to the generator, and any test using it is REJECTED.**
`parse_schema` scrapes the assertion dispatch by matching lines that *begin* with `"assert_`
(`gene2e.rs:527-548`). `assert_response` is not in the `match` in `evaluate_assertion`
(`full.rs:3642-3660`) — it is special-cased in the **step loop** at `full.rs:5366`
(`if op == "assert_response" {`), because it must read the *previous step's response payload*.
Consequences: it never enters `Schema::asserts` ⇒ never appears in the prompt; and
`Schema::is_known()` (`gene2e.rs:177-179`) returns **false**, so `validate()` (`:836-838`) would
**reject** any generated test that used it — *"unknown op `assert_response` (not in full.rs)"*.
**Fix:** add `"assert_response"` to the hardcoded `extra` list at `gene2e.rs:584-586` (that list is
already self-verifying — it filters on `src.contains("\"{o}\"")`). Then teach the prompt the
pattern: *"after any `get_*` op, assert on its RESPONSE with `assert_response`, never by re-reading
engine state."* **This is the fix that makes the whole `get_*` family non-vacuous. Effort: 1 h.**
(`assert_dom` and `assert_window_state` are in the dispatch — `full.rs:3648-3649` — and **do**
auto-surface. The brief was right to make me check rather than assume; the answer is 2 of 3.)

**F2 — 22 corpus lines ask for an assertion that does not exist.** `assert_state_machines_idle`
appears 22× in `scripts/E2E_TESTS.txt` and is **not** in `evaluate_assertion`. It will be rejected
at generation (good — `validate()` catches it) but those 22 cases will simply never be generated,
silently. Three more assertions `E2E_PLAN.md` designed and the corpus leans on are also missing:
`assert_manager_invariants`, `assert_composition`, `assert_damage_sound`. **Decide:** implement
`assert_state_machines_idle` (it is the cheapest and the highest-value — "after 400 ms of virtual
time, no manager has an active drag/animation/fade") or strip the phrase from the corpus.
**Effort: 4 h to implement.** Recommend implementing — the `[*/settle]` and `[*/stable]` categories
(~1,300 lines) are its natural home.

**F3 — the good news, stated plainly.** The unknown-assertion path **fails loudly**
(`full.rs:3661`: `other => AssertionResult::fail("Unknown assertion: …")`), and the generator's
`validate()` rejects unknown ops, denied ops and zombie ops *before* a test is ever written. The
gate that would have caught today's zombie-op class **is already in place for the generated fleet.**

**F4 — the `MANAGER_WIRING_AUDIT` red-cluster forecast.** Its DEAD rows (gesture pinch/rotate,
clipboard, a11y, gamepad, drag-source, changeset, gpu_state drain) map to corpus categories that
are **tiny**: `[gesture/*]` 2 lines, `[clipboard/leak]` 1, `[a11y/stale]` 1. The bulk of the
corpus — `[compose/*]` 2,040, `[css/*]` 2,088, `[mutate/*]` ~2,400, `[input/*]` 2,304,
`[resize/*]` 387 — routes through paths that are alive. **The corpus is well-aimed.** But re-audit
that doc against `fe8165f57` before triaging red tests against it.

### Ordered plan to Friday

| # | Task | Effort | Depends on |
|---|---|---|---|
| 1 | `"expect": "fail"` two-way xfail marker (§3.E-prime) — **unblocks CI, unblocks everything** | 3 h | — |
| 2 | `assert_response` into `gene2e.rs:584` `extra` + prompt guidance (§3.F1) | 1 h | — |
| 3 | Mock-font steering in the prompt (§3.A) | 1 h | — |
| 4 | `e2e/resize-damage.json` to **settle** the resize claim (§3.B) — do not pre-emptively fix | 1 h | — |
| 5 | Fix the inserted-node author cascade (§3.E1) | 1–3 d | — |
| 6 | `assert_state_machines_idle` (§3.F2) | 4 h | — |
| 7 | Per-frame diagnostics + `assert_no_silent_fallbacks` (§4.C) — **make it the default trailer on every generated test** | 1–2 d | — |
| 8 | Batching + `"isolated"` lane + shard matrix (§3.D) | 1 d | 1 |
| 9 | **Generate.** `--dry-run` first, then `--limit 200`, triage, then the full fan-out | — | 1–4, 7, 8 |
| 10 | Harness mutation job (§4.A) | 2–3 d | 8 |

Items 1–4 + 6 are **one day** and they are what stands between you and a signal-bearing run.
Item 5 is the only thing on the list that is genuinely engine work; if it slips, **pin it** (item 1
exists precisely so it can slip) and accept a known red cluster in `[mutate/*]`.

---

# PART 4 — Structurally preventing the three bug classes

This is the section to hand to a new session. Each class gets: the mechanism, what it would have
caught *today*, the effort, and the runtime cost.

## CLASS A — FALSE-GREEN: the harness reports success for work it never did

Four independent instances in one day: (1) `assert_screenshot` auto-baselined its own output;
(2) `redraw`/`relayout` let a test force the effect it claims to measure; (3) ZOMBIE ops returned
`ok` from a catch-all with no match arm; (4) a query op was asserted by re-reading engine state
instead of its response, so it stayed green against a dead op. All four are patched individually.
Individually patching this class is a losing game — it will recur with every new op.

### The mechanism: **harness mutation testing**

> **Invariant: for every op and every assertion in the protocol, neutering its implementation must
> make at least one test FAIL. If neutering it changes nothing, it is vacuous or untested.**

This kills the class mechanically, forever, and it runs in CI.

**How to neuter — a fault-injection env var, not a `cfg`.** A `#[cfg(mutation)]` build would need
one rebuild per mutant (~107 rebuilds — unaffordable). An env var costs **one** build:

```rust
// full.rs, next to the debug-server statics. Parsed once.
static NEUTERED: OnceLock<BTreeSet<String>> = OnceLock::new();
fn is_neutered(op: &str) -> bool {
    NEUTERED.get_or_init(|| std::env::var("AZ_E2E_NEUTER").unwrap_or_default()
            .split(',').filter(|s| !s.is_empty()).map(str::to_string).collect())
        .contains(op)
}
```

Two insertion points, ~15 lines total:

- **Ops** — at the top of `process_debug_event` (`full.rs:~7700`), before the `match`:
  `if is_neutered(op_name) { send_ok(request, None, None); return; }`
  — i.e. *reproduce the zombie*: answer `ok`, do nothing.
- **Assertions** — at the top of `evaluate_assertion` (`full.rs:3641`):
  `if is_neutered(op) { return AssertionResult::pass("neutered"); }`
  — *reproduce the auto-baseline*: pass unconditionally.

Both live behind the `debug-server` feature the e2e DLL already carries. **Runtime cost when
`AZ_E2E_NEUTER` is unset: one `OnceLock` read per op — unmeasurable. In production builds
(`debug-server` off) the code does not exist.**

**Mapping op → covering tests is free.** An op appears **literally, as a string**, in the JSON that
uses it. `grep -l '"op": *"scroll"' e2e/gen/*.json` *is* the coverage map. No instrumentation, no
tracing, no coverage build.

**The CI job (`e2e_mutation`)**

```
for each op O in parse_schema() ∪ assertions:          # ~88 ops + ~19 asserts ≈ 107
    tests = grep -l "\"op\": *\"O\"" e2e/gen/*.json    # cap at 20, sampled deterministically
    if tests is empty:  -> UNCOVERED(O)                # nobody tests this op at all
    batch(tests) into ONE array file
    run once with AZ_E2E_NEUTER=O
    if every test still passes: -> VACUOUS(O)          # <<< the false-green detector
fail the job, naming every VACUOUS and UNCOVERED op
```

**Cost:** ~107 processes, each running ≤20 tests in one batch ⇒ roughly the wall-clock of one shard
of the main suite, **~10-15 min on one runner.** It reuses the same DLL artifact. This is cheap
enough to run per-PR; if it is not, run it nightly and on any PR touching `full.rs`.

**Two required refinements:**

- **Whitelist the load-bearing ops.** Neutering `mount` or `wait_frame` breaks *every* test — that
  is expected, not a finding. Keep an explicit `MUTATION_EXEMPT` list (`mount`, `wait_frame`,
  `wait`, `tick_ms`) with a written reason each — same discipline as the `remap_node_ids`
  exemptions (§4.B).
- **`VACUOUS` must be an error, not a warning.** The whole point is that it fails the build.

**What it would have caught, today, with no other change:**

| Today's bug | Mutation that exposes it |
|---|---|
| `assert_screenshot` auto-baseline | neuter `assert_screenshot` ⇒ every screenshot test still green ⇒ **VACUOUS** |
| The five zombie ops | neuter `focus` ⇒ nothing changes (it already did nothing) ⇒ **VACUOUS** |
| `get_dom` asserted via engine re-read | neuter `get_dom` ⇒ the test still green ⇒ **VACUOUS** (this is *why* `assert_response` exists) |
| a test forcing its own effect via `redraw` | neuter `set_node_text` ⇒ the test still green because `redraw` repaints anyway ⇒ **VACUOUS** |

Four for four. **Effort: 2–3 days** (the neuter hooks are an afternoon; the job, the sampling, the
exempt list and the triage of the first run are the rest). It is the cheapest permanent win in this
document.

**The cheap subset, if the full job is too much:** mutate **only the 19 assertions**. An assertion
that cannot fail is strictly deadlier than an op that does nothing (a dead op usually makes *some*
downstream assertion fail; a dead assertion makes *everything* pass). 19 mutants ≈ 3 minutes.
Do this one first, this week, regardless.

## CLASS B — DERIVED STATE NOT RECOMPUTED after the DOM changes

Six-plus instances, one shape: **the DOM changed, something derived from it did not.**
The repo already contains the correct answer — it is just applied to **one axis only**.

### What exists (and works)

`LayoutWindow::remap_node_ids` (`layout/src/window.rs:8101`) destructures `Self` **exhaustively,
with no `..`** (`:8104-8175`). Every one of the 48 fields is either (a) node-keyed ⇒ calls
`NodeIdRemap::remap_node_ids` (`layout/src/managers/mod.rs:148`), or (b) `field: _` **with a written
reason**. A new field is an **`E0027` compile error** until it is classified. `0488d6a23` is the
proof: `frame_report` did not compile until someone decided what it was.

### The gap: that guards **one** axis, and the bugs are on the **other**

- **Axis 1 (guarded):** *the DOM was reconciled and NodeIds were renumbered.* ⇒ `NodeIdRemap`.
- **Axis 2 (UNGUARDED):** *the `StyledDom` was mutated **in place*** — `insert_node`,
  `set_node_text`, `set_node_classes`, `set_node_css_override`, a runtime restyle, a text edit. Every
  cache derived from that DOM is now stale, and **nothing at compile time forces anyone to say so.**

Every one of today's Class-B bugs is on Axis 2.

### The sharpest single instance — `is_layout_equivalent` is an allowlist by omission

`core/src/styled_dom.rs:2480`. `StyledDom` has **11 fields** (`styled_dom.rs:830-843`).
`is_layout_equivalent` compares **four** of them (`node_hierarchy`, `node_data`, `styled_nodes`,
and attributes within `node_data`). It **never looks at `css_property_cache`** (`:840`) or
`cascade_info` (`:835`).

So: **a mutation that writes the property cache while leaving `node_data` alone is, by this
function's definition, "layout equivalent" — and layout is skipped entirely** (called at
`dll/src/desktop/shell2/common/layout.rs:603`). That is verbatim the bug in the brief ("a mutation
writing inline props while layout reads the property cache"; "layout SKIPPED because
`is_layout_equivalent` says the DOM is unchanged"). The function is not *wrong*; it is
**unfalsifiable** — it enumerates what to compare, and anything a future field adds is silently
assumed not to matter.

It is also an **identity check masquerading as an equality check**: its correctness depends on `old`
and `new` being *different objects*. Three separate places in-tree already document the resulting
bug in comments (`common/event.rs:5363-5372`, `headless/mod.rs:1015-1024` and `:1605-1614`,
`macos/mod.rs:6001-6005`).

### The proposal — three mechanisms, in increasing cost

**B1. Make `is_layout_equivalent` exhaustive. (Half a day. Do this first.)**

```rust
pub fn is_layout_equivalent(old: &StyledDom, new: &StyledDom) -> bool {
    // EXHAUSTIVE BY DESIGN — no `..`. A new StyledDom field is an E0027 error
    // until you decide whether it can change layout. See remap_node_ids.
    let StyledDom { root, node_hierarchy, node_data, styled_nodes, cascade_info,
                    nodes_with_window_callbacks, nodes_with_datasets,
                    tag_ids_to_node_ids, non_leaf_nodes, css_property_cache, dom_id } = old;
    ...
}
```
Every field must then be **compared** or **`_`-ignored with a written reason**. Doing this exercise
honestly forces the `css_property_cache` question into the open, which is the bug.

**B2. The second exhaustive destructure — `LayoutWindow::invalidate_for_dom_change`. (3–5 days.)**

Mirror `remap_node_ids` exactly, on Axis 2:

```rust
pub trait DomDerived { fn on_dom_mutated(&mut self, change: &DomChange); }

impl LayoutWindow {
    /// EXHAUSTIVE BY DESIGN. No `..`. A new field must declare how it is
    /// invalidated when the DOM is mutated IN PLACE, or say why it need not be.
    pub fn invalidate_for_dom_change(&mut self, change: &DomChange) {
        let Self { layout_cache, layout_results, text_constraints_cache, dirty_text_nodes,
                   font_stacks_hash, text_cache, font_manager, a11y_manager,
                   cpu_image_callback_results, /* …all 48… */ } = self;
        layout_cache.on_dom_mutated(change);          // Solver3LayoutCache — window.rs:606
        layout_results.on_dom_mutated(change);        // the derived DOM — window.rs:621
        text_constraints_cache.on_dom_mutated(change);// (DomId,NodeId)-keyed — window.rs:710
        dirty_text_nodes.on_dom_mutated(change);      // window.rs:714
        *font_stacks_hash = 0;                        // window.rs:753 — see below
        // font_manager: _  // content-addressed by FontId/FontChainKey — cannot go stale
        // image_cache:  _  // keyed by image id, not node
        ...
    }
}
```

Every field named. Every exemption written down. `E0027` on the next one.

**The `LayoutWindow` fields this covers, and what is wrong with each today** (from a full field
audit of `window.rs:589-772`):

| Field | Line | Today | Why it is a bug waiting |
|---|---|---|---|
| `layout_cache: Solver3LayoutCache` | 606 | exempt: *"rebuilt wholesale by the layout pass"* | **The exemption is false.** `LayoutCache` (`layout/src/solver3/cache.rs:346-389`) persists `tree`, `cache_map`, `float_cache`, `counters`, `previous_positions` across frames, keyed by *layout index*, plus `cached_display_list: Option<(SubtreeHash, LogicalRect, DisplayList)>` and **`prev_dom_ptr: usize` — a raw-pointer-identity short-circuit** (`cache.rs:385-388`). An in-place mutation **preserves the pointer.** The fast path is currently dormant (always `None`/`0`); the day someone enables it, the stale-screen bug returns and the destructure will **not** catch it, because it is already classified exempt. |
| `font_stacks_hash: u64` | 753 | exempt | **DEAD FIELD** — never written, never read (only decl, `:0` in the ctor at `:881`, and `_` in the destructure at `:8164`). The `FONT_INVALIDATION_*` doc describes it as live. Classified-as-exempt is exactly where a should-recompute field hides. |
| `layout_results` | 621 | exempt: replaced by the layout pass | True **only if** the layout pass runs — which `is_layout_equivalent` can skip (B1). |
| `text_constraints_cache` / `dirty_text_nodes` | 710 / 714 | `remap_dom_keys` (Axis 1 only) | No Axis-2 invalidation. |
| `a11y_manager` | 641 | exempt: *"rebuilt per frame from the current StyledDom"* | A **behavioural** claim, not a structural one. Nothing enforces it; a cached field inside `A11yManager` breaks it and compiles fine. |
| `pre_preedit_content` | 757 | exempt: "plain state" | A snapshot of one node's inline content. If the DOM is rebuilt mid-preedit, it restores content into whatever node now holds focus. |
| `timers` | 680 | exempt | A `Timer` can carry a `DomNodeId` target. |
| `current_/previous_window_state` | 688/691 | exempt | `FullWindowState` carries hover/focus node refs — a second place a NodeId can go stale. |
| `cpu_image_callback_results` | 619 | exempt: keyed by `ImageRefHash` | Correct — **and it is where §3.E2 must hook.** |

**Also outside the net entirely:** four NodeId-holding managers are **not `LayoutWindow` fields**,
so the destructure cannot reach them — `layout/src/managers/scroll_into_view.rs`,
`selection.rs`, `changeset.rs`, `drag_drop.rs`. **Either hoist them into `LayoutWindow` or delete
them.** A compile-time net only guards what is inside it, and this is the hole in the net.

**B3. Type-level: `FreshStyledDom` vs `CommittedStyledDom`. (1 week; propose, do not schedule.)**
Newtype the two, so `is_layout_equivalent(&dom, &dom)` — the identity case that causes the
in-place-mutation skip — **does not typecheck.** This is the only mechanism that makes the bug
*unrepresentable* rather than *caught*. Worth doing after B1/B2 prove the pattern.

### What Class B would have caught

| Today's bug | Caught by |
|---|---|
| Managers keeping stale NodeIds | **Already caught** — `NodeIdRemap` (Axis 1). This is the existence proof. |
| `frame_report` unclassified | **Already caught** — `E0027` (`0488d6a23`). |
| Font stacks not re-resolved on mount | B2 — `font_stacks_hash` / font chains forced to declare an Axis-2 rule instead of rotting as a dead field |
| Inserted node gets no author cascade | B2 — the cascade result is DOM-derived state with **no** recompute path; `drop(css)` (`styled_dom.rs:1084`) becomes a **compile-visible contradiction**: you cannot implement `on_dom_mutated` for the cascade without the rules |
| Mutation writes inline props, layout reads the property cache | **B1 directly** — `css_property_cache` is one of the seven fields `is_layout_equivalent` never looks at |
| Layout skipped: `is_layout_equivalent` says unchanged after in-place mutation | **B1 + B3** |
| ~~Damage not tracked across a buffer realloc~~ | **This one turned out not to exist (§3.B).** But the *state* is real and unguarded: `previous_display_list`, `previous_scroll_offsets`, `previous_gpu_*` and the compositor live on `CpuBackend` (`headless/mod.rs:190-230`), **not** on `LayoutWindow`, so B2 does not reach them at all. **Extend the same exhaustive destructure to `CpuBackend` (B2b)** — it is the largest pile of DOM-derived cache currently outside the net. |

**Cost: zero at runtime.** It is a destructure and a trait. The cost is that **every new field costs
a decision** — which is the entire point, and is exactly why it works.

## CLASS C — SILENT FALLBACK / SWALLOWED FAILURE

> **The rule: a failed resolution must be a typed, surfaced outcome — never a silent default.**

Today's specimen: a `font-family` that matched nothing contributed **nothing** to the chain
(`third_party/rust-fontconfig/src/registry.rs` ~`:905-925`), and then `ensure_chains_nonempty`
(`layout/src/solver3/getters.rs:4086-4124`) handed **every** unmatched family **the same arbitrary
fallback face** — 8 families collapsed to 2 `FontId`s, text rendered in the wrong font, and a leak
test passed **vacuously**.

### The audit — this is not a one-off. Six live sites, same shape.

| # | Site | The silent default |
|---|---|---|
| C1 | **`css/src/css.rs:73`** — `impl From<AzString> for Css` does `new_from_str(s).0` | **Every CSS parse warning is discarded at the public API boundary.** The parser does the right thing (`css/src/parser2.rs:655` returns `(Css, Vec<CssParseWarnMsg>)`; an unknown property is explicitly downgraded to a warning and execution continues, `:1783-1789`) — and then **every production caller throws them away**: `core/src/xml.rs:5191` and `:7534` (the `<style>` path), `core/src/styled_dom.rs:411` and `:2663`, `core/src/gpu.rs:452`/`:1121`, `layout/src/headless.rs:429`, `layout/src/solver3/paged_layout.rs:660`, `css/src/system.rs:1706`. `Css` has **no `warnings` field**. `Css::from_string_with_warnings` (`css/src/css.rs:104`) exists and **nothing calls it.** **Net: a typo'd CSS property produces zero observable signal, anywhere, ever.** For a 13k-test CSS fleet this is the most dangerous line in the repo. |
| C2 | **`layout/src/solver3/getters.rs:3485`** — `into_fontconfig_chains()` | **Today's fix produced the data and never wired the channel.** `unresolved_families` / `last_resort_chains` (`:3445`/`:3449`) are **dropped on the floor** here; they never reach `FontManager` or `LayoutWindow`, so **no e2e test can assert on them.** And `report_unresolved_families` (`:4361`) is an `eprintln!` behind a **process-global `OnceLock<Mutex<BTreeSet>>`** — in a batched run (§3.D) only the **first test in the process** ever sees it. |
| C3 | `layout/src/solver3/display_list.rs:4413` | A missing/undecodable image: `if let Some(image_ref) = … { push_image }` — **no `else`, no log, no counter.** An empty box is indistinguishable from a correctly-empty box. (`get_image_ref_for_image_source`, `:4685`, returns `None` on cache miss `:4694`, decode failure `:4708`, SVG rasterize failure `.ok()` at `:4728`.) |
| C4 | `layout/src/text3/default.rs:768` (and `:805`) | `lookup_glyph_index(ch).unwrap_or(0)` — **every missing glyph silently becomes `.notdef`/tofu.** |
| C5 | `layout/src/font.rs:1961` | `mock.glyph_advances.get(&gid).copied().unwrap_or(0)` — a missing metric silently measures **width 0**. This is *in the mock-font path*: it is the precise mechanism by which a metrics test goes vacuously green. |
| C6 | `layout/src/font.rs:623` + 11 `ParsedFont::from_bytes` call sites | `b64.unwrap_or_default()` → empty bytes → "font parse failed"; and every one of the 11 sites passes a `&mut warnings` Vec that is **declared locally and dropped immediately**. |

Anti-pattern census (`src/` only): `unwrap_or_default()` — layout 259, css 85, core 58, dll 51.
Catch-all `_ =>` arms — layout 407, css 337, core 162, dll 296. **Do not try to lint all of these.**
Fence the *resolution* functions only (see enforcement below) — that is ~50 sites.

### The one place the repo got it right — copy it

The debug-server catch-all (`full.rs:12073-12081`) — the origin of the zombie-op bug — is now the
model, and it has **three** ingredients:

1. **a typed outcome** (the variant is reachable and named);
2. **a `log(LogLevel::Warn, …)`** into the buffer that `DebugEvent::GetLogs` (`full.rs:8423`)
   drains — i.e. a channel **an e2e test can assert on**;
3. **a build-time scan that refuses to proceed** — `gen-e2e`'s zombie gate keys off this exact
   catch-all and its `"Unhandled:"` marker, and **will not generate a test that uses an unhandled
   variant.**

Fonts have (1) only. CSS has (1) only, and discards it at every call site. Images have none.

### The proposal

**C-a. A per-frame diagnostics sink on `LayoutWindow` (NOT a process-global).**

```rust
// layout/src/window.rs — new LayoutWindow field (and therefore a NEW ENTRY
// in BOTH exhaustive destructures — see §4.B; that is the mechanism working).
pub struct FrameDiagnostics {
    pub unresolved_font_families: Vec<String>,   // ← C2, wired through from getters.rs:3445
    pub last_resort_font_chains: usize,          // ← C2, getters.rs:3449
    pub css_parse_warnings: Vec<CssParseWarnMsg>,// ← C1, stop doing `.0`
    pub unresolved_images: usize,                // ← C3
    pub notdef_glyphs: usize,                    // ← C4
    pub unhandled_ops: Vec<String>,              // ← the zombie channel, already logged
}
```
Reset by the existing `reset_frame_counters` op. **Per-window state — this is what makes batching
(§3.D) safe**, and it is why C2's `OnceLock` must die.

**C-b. `assert_no_silent_fallbacks` — and make it the DEFAULT TRAILER on every generated test.**
A new assertion (all counters zero, or under a declared budget), plus
`assert_diagnostics { kind, max }` for tests that *want* a fallback. Then have `gen-e2e` append
`{"op":"assert_no_silent_fallbacks"}` to **every** generated test unless the case explicitly opts
out. **13,223 tests, each asserting that nothing silently degraded, for the price of one prompt
change.** This is the highest-leverage single line in this entire document: it turns the whole
corpus into a silent-fallback detector at zero marginal cost.

**C-c. Enforcement — a fenced allowlist, not a global lint.**
A blanket clippy `disallowed_methods` on `unwrap_or_default` is useless here (259 sites in `layout`
alone, most of them legitimate). Instead, follow the pattern this repo **already** uses for
dependencies (`scripts/check_dep_justifications.py`): a CI script that scans a **fenced set of
resolution modules** — `layout/src/solver3/getters.rs`, `layout/src/solver3/display_list.rs`,
`layout/src/text3/`, `layout/src/font.rs`, and every caller of `Css::new_from_str` — and **fails the
build** on any `unwrap_or_default()` / `.ok()` / `_ =>` that does not carry a
`// SILENT-OK: <reason>` comment or appear in `scripts/silent_fallback_allowlist.toml`. Start
fenced; widen later. **~50 sites to triage, not 900.**

**C-d. (Phase 2, propose only.) `Resolved<T>`.**
```rust
#[must_use]
pub enum Resolved<T> { Exact(T), Fallback(T, FallbackReason) }
```
A fallback cannot be consumed without acknowledging it. This makes the class *unrepresentable*
rather than *detected*. It is a large refactor of the resolution surface; do it after C-a/C-b prove
their worth.

**Effort:** C-a + C-b: **1–2 days** (and C-b must land *before* the big generation run, or you pay
for 13k tests twice). C-c: **1 day**. C-d: **1+ week**.
**Runtime cost:** six counters and a `Vec<String>` per frame, written only on the failure path.
Zero on the happy path. The `Vec`s can be `#[cfg(feature = "debug-server")]` if anyone objects.

**What it would have caught:** the 8-families-collapse-to-2 font bug (C2 — `assert_no_silent_fallbacks`
would have gone red on the very first text test); the vacuous font-leak test (same); and — the one
that is still live and unguarded — **every CSS typo in 13,223 generated tests** (C1).

---

# PART 5 — New-session handoff

## Current state

- **Commit:** `fe8165f57` (master). Working tree clean at audit time.
- **What works:** `azul-doc gen-e2e` end-to-end — schema parsed at runtime from `full.rs`
  (`doc/src/gene2e.rs:431`), op denylist (`:258-388`), zombie gate (`:203-207`), content-addressed
  incremental generation with `--dry-run`/`--limit`/`--filter`/`--prune`. 13,223-line corpus at
  `scripts/E2E_TESTS.txt`. 19 assertions live. `mount`, `tick_ms`, `FrameReport`, damage PNGs, the
  five ex-zombie ops, `assert_response`, auto-registered mock fonts. Blocking CI gate
  `e2e_headless` (`.github/workflows/rust.yml:1987`).
- **What is RED, and why:** of the 10 `e2e/*.json`, **exactly one is red** —
  **`bug-inserted-node-no-author-css.json`** (`87b260540`). An inserted node never receives the
  author cascade: `StyledDom::create` runs `restyle(...)` and then **`drop(css)`**
  (`core/src/styled_dom.rs:1084`), and the insert path ends at
  `sd.recompute_inheritance_and_compact_cache()` (`dll/src/desktop/shell2/common/event.rs:2482`) —
  **UA defaults + inheritance only, no author re-cascade** — so `#b` lays out at the block default
  (384px) instead of 80×50.
  **This is failing a BLOCKING gate that `deploy_pages` depends on (`rust.yml:3105`) — master
  cannot deploy today.** Fix it (§3.E1) or land the xfail marker (§3.E-prime). Do one of the two
  **first, before anything else.**
  The other three `bug-*.json` were written red and have since been **fixed** (`6499f0c47`,
  `2bb6909ba`) — they are green regression tests now, not pins.
- **There is NO xfail mechanism.** Zero hits for `expect_fail|xfail|known_red`. Adding one (§3.E-prime)
  is task #1.

## Hard safety rules — violating these has cost real time

- **NEVER `cargo test -p azul-dll`.** It OOMs and hard-locks the machine. Always
  `timeout 600 cargo test -p <specific-small-crate>`. Never launch unbounded parallel builds.
- **Always `timeout 600 cargo …`.** No exceptions.
- **`api.json` has NO trailing newline.** (Verified: the file ends `}}` with no `\n`.) Any tool that
  rewrites it must preserve that, or every codegen diff becomes noise.
- **`LayoutWindow::remap_node_ids` (`layout/src/window.rs:8101`) destructures `Self` EXHAUSTIVELY
  BY DESIGN. Never add `..` to silence `E0027`.** The compile error *is* the feature — it is asking
  you to decide whether your new field is DOM-derived. Read the doc comment at `:8080-8100` first.
  The same rule will apply to `is_layout_equivalent` and `invalidate_for_dom_change` once §4.B lands.
- **A rate-limited LLM agent returns exit 0 with a plain-text limit message as its body.**
  **Verify the ARTIFACT, never the exit code** (`scripts/autotest_fleet.sh:161-170`). The file must
  exist, parse as JSON, and contain `"steps"`.
- **Never let a generation agent run `cargo`.** N agents each building will lock the machine. One
  global verify pass at the end.
- **`--dry-run` before every fleet.** Non-negotiable.

## Exact commands

```bash
# ── Rebuild the DLL (the codegen pipeline; see the memory note) ───────────────
cargo run -p azul-doc -- codegen all
timeout 600 cargo build --release -p azul-dll --features build-dll   # → target/release/libazul.so
#   `build-dll` implies `cabi_export` (dll/Cargo.toml:528) → the ~13.9k #[no_mangle] Az* symbols.
#   Sanity: nm -D --defined-only target/release/libazul.so | wc -l   # ≈ 12,700

# ── Build the headless e2e host (links the DLL; note the FEATURES) ───────────
export AZ_LINK_PATH=$PWD/target/release
export LD_LIBRARY_PATH=$PWD/target/release
timeout 600 cargo build --release -p azul-examples --example hello-world \
    --no-default-features --features link-dynamic
#   link-dynamic selects cabi_external = the extern "C" DECLARATION side. The dll
#   itself must NEVER be built with link-dynamic — that is what produces a
#   4-symbol libazul.so and `undefined symbol: AzU8Vec_delete` (§3.C).

# ── Run ONE scenario ─────────────────────────────────────────────────────────
AZ_BACKEND=headless AZ_E2E=e2e/mock-font-exact-metrics.json \
    timeout 300 ./target/release/examples/hello-world

# ── Run the whole gate exactly as CI does (rust.yml:2044-2066) ───────────────
for f in e2e/*.json; do
  AZ_BACKEND=headless AZ_E2E="$f" timeout 300 ./target/release/examples/hello-world \
    && echo "PASS $f" || echo "FAIL $f"
done

# ── Generate ─────────────────────────────────────────────────────────────────
cargo run -p azul-doc -- gen-e2e scripts/E2E_TESTS.txt e2e/gen --dry-run     # ALWAYS FIRST
cargo run -p azul-doc -- gen-e2e scripts/E2E_TESTS.txt e2e/gen \
    --jobs 6 --model haiku --effort low --limit 200 --filter resize
#   flags: --jobs N --model M --effort E --limit N --filter <tag> --dry-run --redo --prune
#   incremental + content-addressed: a line is done when its artifact exists AND passes the gate.

# ── The unit-test / css-review fleet (a separate tool, same lessons) ─────────
scripts/autotest_fleet.sh --dry-run
```

## Ordered task list (dependencies in brackets)

1. **`"expect": "fail"` two-way xfail marker** (`full.rs:3218` + the runner). Unblocks the red gate;
   everything downstream needs it. **3 h.** []
2. **`assert_response` → `gene2e.rs:584` `extra` list** + prompt guidance ("assert on the RESPONSE,
   never by re-reading engine state"). **1 h.** []
3. **Mock-font steering in `build_prompt`** (`gene2e.rs:694-780`). **1 h.** []
4. **`e2e/resize-damage.json`** — settle the resize-damage claim empirically (grow **and** shrink).
   It reads **correct** at HEAD (§3.B); do not "fix" it. **1 h.** []
5. **`assert_state_machines_idle`** (22 corpus lines want it; ~1,300 `[*/settle]` lines are its
   home). **4 h.** []
6. **Fix the inserted-node author cascade** (`styled_dom.rs:1084`; design in
   `DEFERRED_CASCADE_DESIGN.md`). **1–3 d.** [] — *if it slips, pin it with (1) and ship.*
7. **Class C: `FrameDiagnostics` on `LayoutWindow` + `assert_no_silent_fallbacks`, appended by
   default to every generated test.** Kill the `OnceLock` dedupe at `getters.rs:4361`. **1–2 d.**
   [needs 3 for the font half; must land BEFORE the big run or you generate 13k tests twice]
8. **Batching + `"isolated": true` lane + `shard` matrix on `e2e_headless`.** **1 d.** [1]
9. **GENERATE.** `--dry-run`, then `--limit 200` and triage the first batch by *failure signature*
   (not by test), then the full fan-out. [2,3,4,7,8; ideally 6]
10. **Class A: harness mutation testing** (`AZ_E2E_NEUTER` + the `e2e_mutation` job). Start with the
    **assertions-only** subset — 19 mutants, ~3 min, do it this week regardless. **2–3 d** for the full
    job. [8]
11. **Class B: exhaustive `is_layout_equivalent` (B1, half a day), then
    `LayoutWindow::invalidate_for_dom_change` (B2, 3–5 d), then extend the destructure to
    `CpuBackend` (B2b).** Also: hoist or delete the four NodeId-holding managers that sit outside
    `LayoutWindow` (`scroll_into_view.rs`, `selection.rs`, `changeset.rs`, `drag_drop.rs`) — a
    compile-time net only guards what is inside it.

**Friday is reachable.** Items 1–5 are one day and they are what stands between the fleet and a
signal. Item 7 is the one thing you should not skip: without it you will generate 13,223 tests that
cannot see a silent fallback, which is the exact bug class that produced this document.

# E2E Readiness — 2026-07-25

Supersedes `E2E_READINESS_2026_07_14.md`. Audited against `master` = **`b5ca32e5d`**. Every claim
below was re-derived from source at that commit — nothing was carried forward from the 07-14 doc
on faith, and several of its headline claims are now **FALSE** (marked **[WAS FALSE]** where the
old doc said the opposite).

Two questions, same as before:

1. **What blocks running `gen-e2e` at scale (Part 3).**
2. **How we structurally prevent the bug classes we keep finding (Part 4)** — the part that matters.

---

## VERDICT: **READY to bulk-generate, with one caveat and two must-dos.**

**READY.** The four blockers the 07-14 doc named are all gone:

| 07-14 blocker | Status at `b5ca32e5d` |
|---|---|
| generator blind to mock fonts | **FIXED** — `doc/src/gene2e.rs:763-770` steers every generated test onto `Azul Mock Mono` / `Azul Mock Wide` and forbids naming a real system font. |
| `assert_state_machines_idle` (22 corpus lines) does not exist | **FIXED** — implemented, plus the other three (`assert_manager_invariants`, `assert_composition`, `assert_damage_sound`). §3.A. |
| no xfail marker, blocking gate red | **FIXED** — `"expect": "fail"` works end to end. §1.A. |
| one OS process per JSON in a serial bash loop | **FIXED** — `azul-doc e2e <dir>` runs a whole directory in ONE process. §1.B. |

**The caveat:** the *runner* is ready; the *CI wiring* is not. `azul-doc e2e` does not appear
anywhere in `.github/workflows/` (verified: `grep -rn "azul-doc e2e" .github/` → no hits). Nothing
generated would run in CI until §3.E lands.

**Must-do before the fan-out (both cheap):**

1. **§3.E — put `azul-doc e2e <generated-dir>` behind a sharded CI job.** Without it a 13k corpus
   is a directory nobody executes. ~4 h.
2. **§3.D — decide the `e2e/gen/` triage policy up front** (xfail-on-first-red vs. quarantine
   lane). At 13k tests you will have hundreds of real reds on day one, and the `"expect": "fail"`
   marker only helps if the triage rule is written down before the reds arrive. ~1 h of policy.

Everything else on the list is an improvement, not a gate.

---

# PART 1 — What changed since 2026-07-14

## A. `"expect": "fail"` exists and works — **[WAS FALSE]**

The 07-14 doc's §3.E ("there is no xfail marker … the single highest-leverage item in §3") is
**no longer true.**

* The field: `E2eTest.expect: Option<String>` — `layout/src/e2e/full.rs:3511`.
* The verdict logic: `layout/src/e2e/report.rs`, ported from the DLL's printer
  (`dll/src/desktop/shell2/run.rs:46-47`, `:175-218`). The four-way table is documented at
  `report.rs:13-19`:

  | raw | `expect` | verdict | fails the gate |
  |---|---|---|---|
  | pass | *(none)* | PASS | no |
  | fail | *(none)* | FAIL | yes |
  | fail | `"fail"` | XFAIL | no |
  | pass | `"fail"` | **XPASS** | **yes** |

* **XPASS is red on purpose** (`report.rs:19`, enforced at `:112-132`): the guarded bug is fixed,
  so the marker must go. That is the difference between a pin and a quarantine.
* In use today: `e2e/mock-font-exact-metrics.json:3` is the one marked scenario, and it reports
  `XFAIL` in both hosts.

## B. `azul-doc e2e <file-or-dir>` — one process, whole directory — **[WAS FALSE]**

The 07-14 doc's §3.D ("one OS process per JSON in a serial bash loop … the #1 runtime blocker") is
**no longer true for the library path.**

* `doc/src/e2erun.rs` (116 lines): `azul-doc e2e <file-or-dir.json> [--filter <substr>] [--list]`
  (usage string at `:67`). `--filter` at `:45`, `--list` at `:50`, the runner at `:83`.
* It refuses an empty selection (`:91`) rather than reporting a vacuous green — a typo'd path or
  filter is red, not "0 tests, ok".
* **No debug-linked host binary is needed.** The 07-14 claim that "a debug-linked host is needed to
  run" no longer holds for this path: the server lives in `azul-layout` and the headless driver
  (`layout/src/e2e/runner.rs`) emulates the slice of the platform event loop the E2E path needs.

**Measured at `b5ca32e5d`** (this box, release build):

```
$ /usr/bin/time -f "%e s wall, %M KB peak RSS" ./target/release/azul-doc e2e e2e
test result: ok. 20 passed; 0 failed; 1 xfailed; 0 xpassed; 0 ignored; 0 measured; 0 filtered out
11.64 s wall, 39700 KB peak RSS
```

21 scenarios, one process, 11.6 s, **39.7 MB peak RSS**. Mean 547 ms/scenario — and most of that
is the scenarios' own `wait` steps, which really sleep, not engine time. Naive extrapolation:
**~121 min single-process for 13,223 tests**, i.e. comfortably shardable inside a 30-min job cap at
5–8 shards. The old ~7 h estimate is dead.

## C. The server moved into `azul-layout`; the DLL copy is still there — **DE-DUP IS OPEN**

* `layout/src/e2e/` — `mod.rs` (feature/hook docs), `full.rs` (13,509 lines, the op dispatch +
  `DebugEvent` + the `E2e*` JSON schema), `runner.rs`, `cpu_backend.rs`, `report.rs`.
* Gated behind the **non-default** `e2e-server` feature: `layout/Cargo.toml:302`; the default
  feature list (`:155`) does not contain it, so the published lean crate is unaffected. Two
  sub-features stay optional: `e2e-server-http` (`:307`, the TcpListener transport) and
  `e2e-server-platform` (`:315`, the timer registration that needs a `dyn PlatformWindow`).
* Three OS/DLL-coupled call sites are injected through `e2e::hooks` (native screenshot, mount-XML
  swap, Material Icons bytes), so the core dispatch is host-agnostic.
* **`dll/src/desktop/shell2/common/debug_server/full.rs` still exists** (12,187 lines vs. the
  layout copy's 13,509). The two have drifted. **De-duplicating it — making the DLL re-export
  `azul_layout::e2e` — is OPEN WORK.** Nothing in `dll/src/` references `azul_layout::e2e` today.
  Until it lands, an op added to one copy is invisible to the other host.

## D. Runner fidelity: `azul-doc e2e e2e` now matches the DLL host

`20 passed / 0 failed / 1 xfailed` — the same verdicts CI's DLL host reports. Getting there took
five fidelity fixes (`57cbd208a`, `efee23b4f`, `8c2788783`), each a PORT of the DLL rather than an
approximation:

1. **No frame was ever rendered.** Every damage assertion reads `LayoutWindow::frame_report`, whose
   only producer is the CPU backend's `render_frame`. The runner never rendered, so
   `accumulated_paint_damage` was permanently `None`. Consequence: `assert_changed` failed no
   matter what the engine did, **and `op_no_damage_when_idle` was VACUOUSLY GREEN** — it asserted
   an absence that was structurally guaranteed. `layout/src/e2e/cpu_backend.rs` is the port that
   fixed it. *This is the canonical specimen of Class A in Part 4.*
2. **Window-state changes were collapsed** last-write-wins, so a `key_down`+`key_up` pair landing
   in one continuation slice left only the key-RELEASED state and Tab-to-focus-next silently did
   nothing. Now each `CallbackChange` is applied in order and the results `max`ed
   (`runner.rs:236-239`).
3. **Wrong cwd** — scenarios must run from the repo root, not the crate dir.
4. **DOM-mutation changes were silently dropped** by a `_ => {}` in `service()` whose comment
   claimed the E2E op set does not produce them. It does (`set_node_text`,
   `set_node_css_override`, `insert_node`). `Runner::apply_user_change` is now a port of
   `PlatformWindow::apply_user_change`.
5. **Font-cache swap produced a stale `FontId`.** The DLL starts from an empty `FcFontCache` plus
   an async registry and re-installs `registry.shared_cache()` at the top of every
   `regenerate_layout`; `replace_fc_cache` re-registers the mock fonts, `index_memory_face`
   APPENDS, and `pick_memory_face` then returns the entry minted against the DISCARDED cache. The
   runner handed the window one eagerly-built cache and never took that path — which is why
   `mock_font_exact_metrics` passed here and failed on the host.

## E. The four missing assertions now exist (this session)

Implemented at `e88e35581`; see §3.A for the full contract and the failure-injection evidence.

## F. Three existing evaluators could pass vacuously — fixed (this session)

`63fbc9d31`. See §4.A — this is Class A recurring exactly as the 07-14 doc predicted it would.

## G. The generator now parses the schema that actually RUNS

`c609d99f1` repointed `FULL_RS` (`doc/src/gene2e.rs:65`) from the DLL's copy to
`layout/src/e2e/full.rs`. Before that, `gen-e2e` generated against one implementation while
`azul-doc e2e` executed a different, already-drifted one.

## H. `bug-inserted-node-no-author-css.json` is GREEN

The 07-14 doc's §3.E1 ("the RED test blocking the release") is fixed:
`azul-doc e2e e2e --filter bug_inserted` → `1 passed`.

---

# PART 2 — The surface, measured

```
$ ./target/release/azul-doc gen-e2e scripts/E2E_TESTS.txt <out> --dry-run --limit 1
[gen-e2e] corpus=scripts/E2E_TESTS.txt total=13223 already-done=0 to-generate=1
          (of 13223 outstanding, 0 invalid) stale-orphans=0
[gen-e2e] schema: 96 ops + 22 assertions + 4 step-loop ops (parsed from layout/src/e2e/full.rs)
[gen-e2e] policy: 91 allowed / 31 denied (gene2e.rs::OP_POLICY) / 0 zombie
```

| Thing | Count | Where |
|---|---|---|
| corpus lines | 13,223 | `scripts/E2E_TESTS.txt` |
| `DebugEvent` ops | 96 | parsed from `layout/src/e2e/full.rs` |
| assertion ops | **22** (was 18) | `evaluate_assertion` dispatch |
| step-loop ops | 4 | `gene2e.rs:608` `extra` |
| allowed / denied | **91 / 31** | `gene2e.rs:264-400` `OP_POLICY` |
| **zombie ops** | **0** | `Schema::is_zombie`, `gene2e.rs:209` |
| hand-written scenarios (`e2e/`) | 21 (20 pass, 1 xfail) | run by the DLL gate + `azul-doc e2e` |
| library fixtures (`layout/tests/e2e_fixtures/`) | **13** (was 9) | run by `e2e_json` + `azul-doc e2e` |
| corpus lines naming a specific assertion | 22, all `assert_state_machines_idle` | `grep -o "assert_[a-z_]*" scripts/E2E_TESTS.txt` |

The corpus names exactly ONE assertion by name. `assert_manager_invariants`,
`assert_composition` and `assert_damage_sound` appear only in `E2E_PLAN.md` — so their parameter
shape was free to design, and the generator reaches them through the prompt, not through corpus
text.

---

# PART 3 — The readiness checklist

Ordered by what blocks a *useful* run.

## A. The four missing assertions — **DONE** (`e88e35581`)

All four are REAL: they read `LayoutWindow` state and can fail. None is a stub. Each ships a
fixture under `layout/tests/e2e_fixtures/` and each was proven to fail by injection.

### `assert_state_machines_idle` — `full.rs:5574` (PLAN §g3)

Sweeps, via `collect_state_machine_leaks` (`full.rs:5456`) — and reports **every** leak, not the
first: active drag, un-ended gesture sessions, active scroll animations, `scroll_dirty`,
`gpu_state_manager.scrollbar_fade_active`, latched `text_edit.display_list_dirty`, a caret blink
that outlived its editor, unresolved `pending_focus_request` /
`pending_contenteditable_focus`, and (opt-out `"damage": false`) `FrameDamage::None`.

*Failure injection:* commenting out `self.scroll_manager.clear_scroll_dirty()` in
`layout/src/window.rs` (end of `layout_and_generate_display_list`) turns the fixture red with
`scroll_manager.scroll_dirty is still set — the display list will be rebuilt again`. Asserting
before the frame drains reports the damage half. Reverted; both suites green after.

### `assert_manager_invariants` — `full.rs:5616` (PLAN §g2)

`managers` (default: all eight) × `cross` (default `["X2","X3","X5","X6","X9","X10"]`).

* **X10** — no manager key may name a node absent from `layout_results`
  (`node_is_live`, `full.rs:5370`). Swept over scroll (needs the new
  `ScrollManager::state_keys()`, `scroll_state.rs:477`), hover hit-test history, focus, the active
  drag's anchor, the multi-cursor node, virtual-view keys, undo-redo node stacks.
* **X2** `has_active_animations()` ⟺ some `AnimatedScrollState.animation` is `Some`
  (`animating_keys()`, `scroll_state.rs:485`). **X3** an active drag agrees with the hit-test DOM.
  **X5** a multi-cursor anchor whose node vanished must have been cleared. **X6** `multi_cursor` ⇒
  focus is set and live. **X9** `scrollbar_fade_active` needs a registered scroll node.
* **X1, X4, X7, X8 are NOT implemented and requesting one FAILS with the reason** — never a silent
  pass. Notably **X4 is moot at HEAD: `LayoutWindow` has no `drag_drop_manager` field at all**, so
  the "two `Option<DragContext>` that must not disagree" pair the plan describes does not exist in
  `azul-layout`. An unknown manager name also fails ("Refusing to pass an unchecked manager").

*Failure injection:* `mount → scroll → unmount → assert_manager_invariants` goes red with
`X10 scroll: state key (0, 11) points at a node that no longer exists; X10 focus: focused_node
(0, Some(5)) points at a dead node` — a real dangling-state finding, no source edit needed.
Requesting `"cross": ["X1"]` and `"managers": ["a11y"]` both fail loudly. Making `node_is_live()`
return `true` unconditionally flips the dangling case to GREEN, proving the check is load-bearing.

### `assert_composition` — `full.rs:6086` (PLAN §g1)

Asserts named stages were ENTERED, **in the listed order**, and that the timeline then reached a
fixpoint (`"fixpoint": false` opts out). Backed by a per-step sample
(`e2e_record_composition_sample`, `full.rs:5995`) taken at the top of each step in
`resume_e2e_continuation`, so it observes the previous step's effects after the shell serviced
them. The trace is zeroed per test and by `reset_frame_counters` — the plan's "checkpoint".
Stages: `drag_active`, `selection_grew`, `scroll_started`, `scroll_animating`, `damage_patch`,
`damage_full`, `focus_set`, `editing_active`, `hover_active`. An unknown stage name fails.

*Failure injection:* reversing `expect` →
`the stages were entered OUT OF ORDER … observed [damage_full@5, focus_set@7, damage_patch@12,
scroll_started@12]`; asking for `drag_active` (never happens) → `1 of the expected stage(s) were
never entered`; a typo'd stage → `unknown stage 'scrolll_started'`.

### `assert_damage_sound` — `full.rs:6255` (PLAN §c)

The **global, stronger** form of `assert_damage_covers_changes`. Four differences, stated because
"we already have a damage assertion" is the objection this op exists to answer:

1. `assert_damage_covers_changes` **passes trivially on a `Full` repaint** ("a full repaint
   trivially covers every changed pixel" — `full.rs:4944`). Here `Full` is still measured for
   tightness, and `forbid_full` rejects it outright.
2. It checks **`present ⊇ paint`**, the invariant `FrameReport` documents and nothing asserted.
3. It adds the plan's **tightness** bound: `area(damage) ≤ max_overpaint_ratio × area(bbox of the
   pixels that really changed)`, default 4.0.
4. Opt-in `"pixel_identity": true` diffs the **damage-driven framebuffer** against an independent
   **full repaint** (`CallbackInfo::take_screenshot`, a different code path with a fresh glyph
   cache). The headless runner publishes the framebuffer (`runner.rs:764` →
   `E2E_PRESENTED_FRAME`, `full.rs:6215`); a host that cannot — the DLL, whose frames live on the
   GPU — **FAILS the check rather than skipping it silently**.

*Failure injection:* shrinking the reported damage rects to 60 % in `cpu_backend.rs` →
`UNDER-PAINT … 1475 uncovered of 2400 changed, first at (37, 0)`; `max_overpaint_ratio: 0.25` →
`OVER-PAINT … 1.00x (damage 2400 px², changed bbox 2400 px²)`; `forbid_full` against a resize →
`the repaint was FULL`; omitting `vs` → `missing 'vs'`. The clean fixture passes with
`forbid_full: true` **and** `pixel_identity: true`, i.e. the incremental path is byte-identical to
a full repaint for a one-box recolour.

**Classified in `OP_POLICY`** at `gene2e.rs:395-398` (all four allowed), so the generator offers
them — assertion count went 18 → 22, allowed 87 → 91.

## B. Three existing evaluators could pass vacuously — **DONE** (`63fbc9d31`)

See §4.A. All three now fail on the vacuous case, proven by removing the gates on a throwaway
build and watching the injections go green.

## C. Honest remaining gaps in the headless runner

These are **real** and they bound what a generated test can prove on the library path. None of
them is a false-green *today* (the ops involved either fail loudly or are simply inert), but each
is a place a generated test will assert less than it appears to.

| Gap | Evidence | Consequence |
|---|---|---|
| `apply_user_change` returns `DoNothing` for DLL-only facilities | `runner.rs:665` `_ => ProcessEventResult::DoNothing`, with the comment naming timers, threads, menus, tooltips, clipboard, text editing, drag & drop, window creation, routing, undo/redo | a corpus line in those categories runs, mutates nothing, and any assertion of ABSENCE after it is vacuous. `[clipboard/*]`, `[menu/*]`, `[timer/*]` categories cannot be trusted on this path — run them on the DLL host or skip them. |
| `relayout_iterations` is hardcoded to 1 | `runner.rs:294-295` — `.max(1)`, the only writer in `layout/` | **`assert_work_bounded` cannot catch an invalidation loop here.** Its `max_relayouts` bound is untestable on the library path; only `max_dom_regens` carries signal. |
| `hit_depth_cap` is never set | the only writer is `dll/src/desktop/shell2/common/event.rs:4192`; nothing in `layout/` writes it | the `MAX_EVENT_RECURSION_DEPTH` half of `assert_work_bounded` is DLL-only. On the library path it is always `false`. |
| `assert_damage_sound`'s `pixel_identity` is headless-only | `E2E_PRESENTED_FRAME` is filled by `runner.rs:764` only | correct behaviour (the op fails on other hosts rather than skipping), but it means the check does not run under the DLL gate. |
| `X1/X4/X7/X8` unimplemented | §3.A | requesting one is red, so no false-green — but those invariants are simply not covered. |

## D. CI wiring — **the caveat**

| Claim | Status |
|---|---|
| `e2e_json` declares `required-features = ["e2e-server"]` | **TRUE** — `layout/Cargo.toml:363-365`. |
| No CI job passes that feature, so cargo silently skips it | **NO LONGER TRUE as of `c609d99f1`** — `scripts/coverage.sh:110` now passes `--features e2e-server` for that target, and the `coverage` job runs `scripts/coverage.sh` (`rust.yml:768`, `:807`). `coverage.sh` sets `set -euo pipefail` (`:19`), so a failing `cargo test … \| tail -1` does propagate. `coverage` is in `deploy_pages.needs` (`rust.yml:3113`), so `e2e_json` is now blocking — via the coverage job, on the instrumented `coverage` profile. |
| The `e2e_headless` gate still uses the per-file `AZ_E2E` bash loop over a `hello-world` example | **TRUE** — `rust.yml:1995` (job), `:2054` (`cargo build … -p azul-examples --example hello-world --no-default-features --features link-dynamic`), `:2056-2081` (`for f in "${scenarios[@]}"; do … AZ_E2E="$f" timeout 300 ./target/release/examples/hello-world … done; exit "$failed"`). It is BLOCKING (`deploy_pages.needs`, `:3113`) and it does refuse a vacuous run (`no scenarios in e2e/ — the gate would be vacuous`). |
| `azul-doc e2e` runs in CI | **FALSE.** `grep -rn "azul-doc e2e" .github/` → **no hits.** The one-process runner is not wired into any workflow. |

**This is the gap that must close before generating.** Recommended shape:

```yaml
e2e_generated:
  strategy: { matrix: { shard: [0,1,2,3,4,5,6,7] } }
  run: ./target/release/azul-doc e2e e2e/gen --filter "shard-${{ matrix.shard }}"
```

…or add a `--shard i/n` flag to `e2erun.rs` (cleaner than encoding the shard in the filename).
Budget from §1.B: ~121 min single-process ÷ 8 ≈ 15 min/shard, inside the 30-min cap.

## E. Ordered plan

| # | Task | Effort | Blocks generation? |
|---|---|---|---|
| 1 | Sharded `azul-doc e2e e2e/gen` CI job (§3.D) | 4 h | **YES** — without it nothing generated is ever executed |
| 2 | Write the red-triage policy for `e2e/gen` (xfail-on-triage vs. quarantine lane) | 1 h | **YES** — policy, not code |
| 3 | **Generate.** `--dry-run`, then `--limit 200`, triage, then the full fan-out | — | — |
| 4 | De-duplicate the DLL's `full.rs` (make it re-export `azul_layout::e2e`) (§1.C) | 1–2 d | no, but every day it waits the two copies drift further |
| 5 | Wire `relayout_iterations` + `hit_depth_cap` in the headless runner (§3.C) | 1 d | no — but until then `assert_work_bounded` is half-blind on the library path |
| 6 | Port the DLL-only `apply_user_change` arms the corpus reaches (§3.C) | 2–3 d | no — skip those categories meanwhile |
| 7 | Harness mutation job (§4.A) | 2–3 d | no |

Items 1–2 are **one day**. Everything after item 3 is improvement.

---

# PART 4 — Structurally preventing the three bug classes

Unchanged in shape from the 07-14 doc, updated with what actually happened in the eleven days
since. **The prediction held: Class A recurred twice.**

## CLASS A — FALSE-GREEN: the harness reports success for work it never did

### The eleven-day record

| Instance | Found | Shape |
|---|---|---|
| `assert_screenshot` auto-baselined its own output | before 07-14 | the oracle was the thing under test |
| `redraw`/`relayout` let a test force the effect it measures | before 07-14 | denied in `OP_POLICY` |
| ZOMBIE ops returned `ok` from a catch-all with no match arm | before 07-14 | `Schema::is_zombie` gate |
| a query op asserted by re-reading engine state | before 07-14 | `assert_response` |
| **`op_no_damage_when_idle` green because no frame ever rendered** | **07-25** | **assertion of ABSENCE, absence structurally guaranteed** |
| **`gen-e2e` parsed the DLL's schema while `azul-doc e2e` ran the layout one** | **07-25** | tests written against ops the runner does not have |
| **`e2e_json` silently skipped by cargo for unmet `required-features`** | **07-25** | a whole test binary never built, no warning |
| **`assert_damage` passed on a typo'd parameter key** | **07-25** | every constraint `if let Some(..)`, fall through to `pass` |
| **`assert_idle_stable` / `assert_work_bounded` passed with zero frames rendered** | **07-25** | assertion of ABSENCE / pure upper bounds |
| **`assert_idle_stable` silently dropped its pixel check without `cpurender`** | **07-25** | `#[cfg]`-gated check, fall through to `pass` |

**The generalisation, now stated as a rule:**

> **An assertion of ABSENCE ("no damage", "no work", "no growth", "state X is None") passes for
> free whenever the machinery that would produce the thing never ran. Every such assertion needs a
> LIVENESS PRECONDITION.**
>
> **A `#[cfg]`-gated or `if let Some(..)`-gated check that falls through to `pass` is the same bug
> wearing a different hat.**

Applied at `63fbc9d31`: `frames_since_reset >= 1` (`layout/src/window.rs:439`, a field that
existed and nothing consulted) now gates `assert_idle_stable` (`full.rs:5080`) and
`assert_work_bounded` (`full.rs:5176`); `assert_damage` (`full.rs:4720`) requires at least one
recognised constraint and rejects unknown keys by name. The correct pattern was already in the
file — `assert_damage_incremental` explicitly fails on `damage.is_none()`.

**Audit rule for the next new assertion:** if you cannot write down an injection that makes it
fail, it does not exist yet. Both new-op commits this session carry the injection evidence in the
fixture's own `description` field, so it survives in the artifact rather than in a review comment.

### The mechanism (still the right answer): harness mutation testing

> **Invariant: for every op and every assertion in the protocol, neutering its implementation must
> make at least one test FAIL. If neutering it changes nothing, it is vacuous or untested.**

An `AZ_E2E_NEUTER=op1,op2` env var read once through a `OnceLock`, with two insertion points
(`process_debug_event` answers `ok` and does nothing; `evaluate_assertion` returns `pass`), costs
**one** build instead of ~107. The op→test coverage map is free: an op appears literally as a
string in the JSON that uses it, so `grep -l '"op": *"scroll"' e2e/gen/*.json` *is* the map.

This session's manual injections are exactly what that job would automate — including the two that
would have caught `assert_damage`'s typo hole and `assert_idle_stable`'s zero-frame hole without a
human noticing them.

## CLASS B — DERIVED STATE THAT SILENTLY STOPS BEING RECOMPUTED

The `NodeIdRemap` / `LayoutWindow::remap_node_ids` exhaustive destructure (no `..`, so a new field
is an `E0027` until classified) remains the model: it forces a *decision* at compile time.

Two live specimens at `b5ca32e5d`:

* **`relayout_iterations` is pinned at 1** in the headless runner (`runner.rs:294-295`). It is
  *classified* — the code says `.max(1)` deliberately — but the classification is "always 1", and
  an assertion reading it (`assert_work_bounded`'s `max_relayouts`) cannot distinguish "converged
  in one pass" from "we never counted". **A classified-but-constant field is the same hazard as an
  unclassified one, and the destructure guard does not catch it.**
* **`hit_depth_cap` has exactly one writer, in the DLL** (`event.rs:4192`). On the library path it
  is permanently `false`, which reads as "the event converged".

Both are §3.C entries. The structural lesson: the destructure guard proves a field was *thought
about*; it does not prove the field is *live on every host*. The e2e-side answer is the same
liveness discipline as Class A — an assertion over a counter should refuse to run when the counter
has no writer on this host, rather than reporting the default.

## CLASS C — SILENT FALLBACK

`ensure_chains_nonempty` handing every unmatched font family the same arbitrary `FontId` is still
the canonical specimen: text renders, nothing errors, and every font-identity and font-leak
assertion becomes vacuous. `2bb6909ba` made the failure observable
(`ResolvedFontChains.unresolved_families` / `.last_resort_chains`); the generator-side mitigation
landed at `gene2e.rs:763-770` (steer onto the built-in mock fonts, forbid real system font names).

**What is still missing is the assertion.** `assert_no_silent_fallbacks` — a trailer on every
generated test that reads the diagnostics counters and fails if any fallback fired unexpectedly —
does not exist. It is the single highest-value addition to Part 4 and it is independent of
everything in Part 3.

The three new ops added this session all follow the anti-fallback discipline explicitly:

* `assert_manager_invariants` fails on an unknown manager name and on an unimplemented invariant
  rather than skipping it;
* `assert_composition` fails on an unknown stage name;
* `assert_damage_sound` fails when `pixel_identity` is requested on a host that cannot supply the
  framebuffer.

That is the pattern to copy: **"I cannot check this" must be red, never green.**

---

# Appendix — reproducing this audit

```
$ git rev-parse --short HEAD
b5ca32e5d

$ cargo build --release -p azul-doc --bin azul-doc

$ ./target/release/azul-doc e2e e2e
test result: ok. 20 passed; 0 failed; 1 xfailed; 0 xpassed; 0 ignored; 0 measured; 0 filtered out

$ ./target/release/azul-doc e2e layout/tests/e2e_fixtures
test result: ok. 13 passed; 0 failed; 0 xfailed; 0 xpassed; 0 ignored; 0 measured; 0 filtered out

$ cargo nextest run --release -p azul-core -p azul-layout --lib --no-fail-fast
Summary [22.079s] 9599 tests run: 9599 passed, 0 skipped

$ cargo nextest run --release -p azul-doc -E 'test(gene2e)'
Summary [0.043s] 20 tests run: 20 passed, 134 skipped
```

**Known unrelated red at HEAD:** 8 tests under `doc/src/autofix/` (`type_index`, `module_map`,
`diff`, `analysis::reachability`) fail. They are pre-existing, predate this session's commits, and
have nothing to do with e2e — verified by stashing every e2e change and re-running.

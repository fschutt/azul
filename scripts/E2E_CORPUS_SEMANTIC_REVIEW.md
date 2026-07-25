# E2E corpus — independent semantic review

**Subject:** `scripts/E2E_TESTS.txt` (13,223 lines, 161 tags) and its expander
`scripts/gen_e2e_cases.py`, reviewed *before* ~13k tests are generated from it.

**Reviewer stance:** second opinion, read-only on the corpus and its generator.
A separate agent owns the dangling-referent fixes in those two files; nothing here
was edited. Every claim below is grounded in a file:line that was actually read.

**The criterion.** The owner's intent is *"ideally these are to stress test the
managers"*, plus coverage completeness over the `CallbackInfo` API and the input
event surface. So the question per line is not "is this grammatical" but:

> Does it put a real manager invariant under pressure in a way that can catch a
> bug, and does its assertion match what that manager can actually get wrong?

A line that reads fine but cannot stress a manager is waste. A line whose
assertion is unrelated to the manager it exercises is worse — it goes green while
the manager is broken.

---

## 0. Verdict in one paragraph

The corpus is **structurally excellent and semantically over-committed**. The
axes (widget × interaction × mutation × phase × assertion-family) are the right
axes, the g4 "dangling NodeId under mid-interaction mutation" family is genuinely
the payload, and uniqueness-by-construction is enforced properly. But the corpus
was written against the *engine's declared* surface, not against the *headless
runner's actual* surface, and the two have diverged badly. **Roughly 4,300 of
13,223 lines (32.6%) currently describe an interaction the headless runner
silently no-ops, an op the policy gate denies, or an invariant that is declared
unimplemented and always fails.** The single most dangerous class is the silent
no-op: `Runner::apply_user_change` ends in `_ => ProcessEventResult::DoNothing`
(`layout/src/e2e/runner.rs:728`), and **50 of the 71 `CallbackChange` variants
land there**. A test built on one of those passes without doing anything, and
then counts as coverage. That is worse than no test.

Recommendation: **generate in four waves** (§8). Wave 1 (~5,600 lines) is sound
today. Waves 2–4 must wait on runner work, or on the runner being made to fail
loudly instead of no-op — which is a ~20-line change and is the highest-leverage
single fix available (§7.1).

---

## 1. Coverage map: managers × corpus

`layout/src/managers/` has 22 modules. `LayoutWindow`
(`layout/src/window.rs:698-795`) actually instantiates **19** of them.

| Manager | Live on `LayoutWindow` | Corpus lines mentioning it | Dedicated `[manager/*]` | In `assert_manager_invariants` `KNOWN_MANAGERS` | Drivable headless |
|---|---|---|---|---|---|
| scroll_state | ✔ `scroll_manager:711` | 4,308 | 15 | ✔ `scroll` | ✔ |
| text_edit | ✔ `:717` | 3,568 | 15 | ✔ `text_edit` | ⚠ partial |
| gesture | ✔ `gesture_drag_manager:713` | 3,271 | 15 | ✔ `gesture` | ⚠ partial |
| focus_cursor | ✔ `focus_manager:715` | 1,296 | 15 | ✔ `focus` | ✔ |
| selection | ✖ *(no field — data types only)* | 1,296 | 15 | ✔ (aliases `text_edit`) | ⚠ partial |
| virtual_view | ✔ `:725` | 902 | 15 | ✔ | ✔ |
| hover | ✔ `:723` | 856 | 15 | ✔ | ✔ |
| undo_redo | ✔ `:795` | 742 | 15 | ✔ | ✖ **dead** |
| clipboard | ✔ `:721` | 388 | 15 | ✖ | ✖ **no op exists** |
| scroll_into_view | ✖ *(free functions)* | 284 | 15 | ✖ | ✔ |
| text_input | ✔ `:793` | 246 | 15 | ✖ | ✖ **dead** |
| gpu_state | ✔ `:727` | 105 | 15 | ✖ | ✔ (via scroll) |
| changeset | ✖ **no manager struct at all** | 53 | 15 | ✖ | ✖ |
| drag_drop | ✖ *(X4 note: no field)* | 48 | 15 | ✖ | ✖ |
| keyring | ✔ `:753` | 45 | 15 | ✖ | ✖ **no op exists** |
| biometric | ✔ `:748` | 35 | 15 | ✖ | ✖ **no op exists** |
| gamepad | ✔ `:762` | 27 | 15 | ✖ | ✖ **no op exists** |
| sensors | ✔ `sensor_manager:758` | 21 | 15 | ✖ | ✖ **no op exists** |
| a11y | ✔ `:728` | 16 | 15 | ✖ | ✖ **no op exists** |
| file_drop | ✔ `:719` | 15 | 15 | ✖ | ✖ **no op exists** |
| geolocation | ✔ `:743` | 15 | 15 | ✖ | ✖ **no op exists** |
| permission | ✔ `:736` | 15 | 15 | ✖ | ✖ **no op exists** |

*(Mention counts are keyword matches over the whole corpus and overlap heavily —
"drag" hits gesture, selection and scroll lines alike. Treat them as an
order-of-magnitude signal, not a partition.)*

### 1.1 Findings

**Over-representation is extreme and roughly 40:1.** `scroll_state`, `text_edit`
and `gesture` account for ~11,000 keyword hits; `a11y`, `file_drop`,
`geolocation` and `permission` get exactly their 15 dedicated lines and nothing
else. That is defensible in principle — scroll and gesture *are* where the
dangling-key bugs live — but it means the tail managers are covered by 15 lines
each that (see below) cannot run.

**Nine managers have literally zero drive path.** There is no debug op for
clipboard, file drop, permission, geolocation, biometric, keyring, sensors,
gamepad, or a11y. The full op list is 96 `DebugEvent` variants
(`layout/src/e2e/full.rs`, `pub enum DebugEvent`) and none of them touches those
subsystems. Their 135 `[manager/*]` lines are unbuildable as written.

**`changeset` is not a manager.** `layout/src/managers/changeset.rs` declares
`TextChangeset` / `TextOpInsertText` / `TextOpDeleteText` and no `*Manager`
struct. The generator's manager axis is `os.listdir(managers/)`
(`scripts/gen_e2e_cases.py:88-93`), so it invented 15 lines for a data module.
Same mechanism produced `[manager/selection]` and `[manager/scroll_into_view]`
for modules that hold types and free functions.

**`assert_manager_invariants` accepts only 8 manager names.**
`layout/src/e2e/full.rs:5383-5392` sets `KNOWN_MANAGERS = [scroll, hover, focus,
gesture, selection, text_edit, virtual_view, undo_redo]`, and :5452 turns any
other name into a hard failure — *"Refusing to pass an unchecked manager."* So
**14 of 22 managers cannot be named in the corpus's single strongest
cross-manager assertion**, i.e. 210 `[manager/*]` lines have no assertion that
fits them.

**`undo_redo`'s X10 sweep is vacuous today.** `assert_manager_invariants`
iterates `lw.undo_redo_manager.node_stacks` (`full.rs:5599`). The only writers to
`node_stacks` outside the module's own unit tests are
`window.rs:7690`/`:7695` (`store_content_snapshot` / `record_operation`), both on
the text-edit commit path — which the runner cannot reach (§3). So the loop runs
zero times, `checked` stays 0, and the assertion passes without checking
anything. Meanwhile the corpus's only undo drive ops (`commit_undo_snapshot`,
`undo_app_state`, `redo_app_state`, handled at `full.rs:6530-6556`) operate on
the *session's* `RefAnyUndoManager` over `app_data` — **a completely different
object from `LayoutWindow::undo_redo_manager`**. The corpus conflates the two.

---

## 2. The always-red set: unimplemented invariants

`eval_assert_manager_invariants` (`layout/src/e2e/full.rs:5381`) declares
`KNOWN_CROSS = ["X2","X3","X5","X6","X9","X10"]` (:5395) and an explicit
`UNIMPLEMENTED_CROSS` table (:5398-5424) for **X1, X4, X7, X8**. Requesting one
of those returns `AssertionResult::fail` with *"is NOT IMPLEMENTED and will not
be silently passed"* (:5459).

The corpus emits 24 lines for each of X1..X10 (`gen_e2e_cases.py:640-651`:
10 invariants × 12 interactions × 2 variants = 240). Therefore:

> **96 lines (`[cross/X1]`, `[cross/X4]`, `[cross/X7]`, `[cross/X8]`) will
> produce tests that are red on the first run and stay red forever**, unless the
> generator silently rewrites them into a different assertion — in which case the
> test's content is unrelated to the line, which is the other failure mode.

Verbatim:

```
[cross/X1] run a text-selection drag across the node to completion and assert invariant X1: scroll_into_view and ScrollManager agree on which container scrolled and by how much
[cross/X4] run a node drag on the node to completion and assert invariant X4: GestureAndDragManager.active_drag and the legacy DragDropManager.active_drag never disagree about whether a drag is live
```

X4 is worse than unimplemented: `full.rs:5406-5409` states *"LayoutWindow has NO
`drag_drop_manager` field — the deprecated second `Option<DragContext>` does not
exist in azul-layout, so the pair cannot disagree."* The invariant is
**unfalsifiable by construction**. The hand-authored seed `[drag/dual]` makes the
same mistake:

```
[drag/dual] start a drag through GestureAndDragManager and assert the deprecated DragDropManager.active_drag never disagrees about whether a drag is live, at every frame of the drag
```

**Rate: 96/240 cross lines (40.0%) are always-red; 24 of those (X4) are also
logically vacuous.**

---

## 3. Physical plausibility — the dangerous class

This is the finding that matters most.

### 3.1 The mechanism

`Runner::apply_user_change` (`layout/src/e2e/runner.rs:317`) ports the DLL's
`apply_user_change` for *some* `CallbackChange` variants and ends at :722-728:

```rust
// Everything else (timers, threads, menus, tooltips, clipboard, text
// editing, drag & drop, window creation, routing, undo/redo) needs
// facilities only the DLL host has …
_ => ProcessEventResult::DoNothing,
```

The comment says *"No scenario in `e2e/` reaches them"* — which was true when it
was written and is emphatically false for this corpus.

Measured against `pub enum CallbackChange` in `layout/src/callbacks.rs`:

- **71** variants total
- **21** have a real arm in the runner
- **50** fall through to `DoNothing`

The 50: `AcceptDrop, AddCursor, AddImageToCache, AddSelectionRange, AddThread,
AddTimer, BeginInteractiveMove, CloseWindow, CommitUndoSnapshot, CreateNewWindow,
CreateTextInput, DeleteBackward, DeleteForward, HideTooltip, InjectNativeGesture,
InsertText, MoveCursor, MoveCursor{Down,Left,Right,Up}, MoveCursorTo{DocumentEnd,
DocumentStart,LineEnd,LineStart}, OpenMenu, ProcessTextSelectionClick,
QueueWindowStateSequence, RedoAppState, RemoveImageFromCache,
RemoveSelectionById, RemoveThread, RemoveTimer, RequestHitTestUpdate,
ResetCursorBlink, ScrollActiveCursorIntoView, SetCopyContent,
SetCursorVisibility, SetCutContent, SetDragData, SetDropEffect,
SetSelectAllRange, SetSelection, SetTextChangeset, ShowTooltip,
Start/StopCursorBlinkTimer, SwitchRoute, ToggleCursorVisibility, UndoAppState`.

The zombie guard in `doc/src/gene2e.rs` (`Schema::is_zombie`, :209) only catches
`DebugEvent` variants with no *dispatch* arm — and `KNOWN_ZOMBIES` is empty
(:1820). **It has no visibility into the second layer**, where an op dispatches
correctly, pushes a `CallbackChange`, and the runner drops it on the floor. That
is a real hole in an otherwise well-designed gate.

### 3.2 The five dead drive ops

Tracing every `OP_POLICY`-allowed op through its `DebugEvent` arm to the
`CallbackChange` it pushes:

| Op | Pushes | Runner |
|---|---|---|
| `text_input` | `CreateTextInput` (`full.rs:11795`) | **DoNothing** |
| `swipe` | `InjectNativeGesture` | **DoNothing** |
| `pinch` | `InjectNativeGesture` | **DoNothing** |
| `rotate` | `InjectNativeGesture` | **DoNothing** |
| `long_press` | `InjectNativeGesture` | **DoNothing** |
| `key_down` (Backspace/Delete) | `DeleteBackward` / `DeleteForward` (`full.rs:11718-11724`) | **DoNothing** (the `ModifyWindowState` half still runs) |

Everything else — `mouse_*`, `click`, `double_click`, `scroll`, `key_up`,
`touch_*`, `pen_*`, `resize`, `move`, `dpi_changed`, `focus`, `blur`,
`set_node_*`, `insert_node`, `delete_node`, `scroll_*` — routes through
`ModifyWindowState` / `ChangeNode*` / `ScrollTo`, all of which have real arms.
Those are sound.

**`text_input` is the headline.** It is one of the corpus's central drive ops, it
is the only way to reach `text_edit`, `text_input`, `changeset`, `selection` and
`undo_redo`, and in the headless runner it does nothing except verify that
something has focus (`full.rs:11786-11803`) and then return `ok`.

### 3.3 Line counts

Modelling each family against its dead axis members:

| Family | Total | Lines whose drive is a runner no-op | Rate |
|---|---|---|---|
| `compose/2` | 1,260 | **720** (any of 5 dead stages of 15) | 57.1% |
| `compose/3` | 780 | **620** | 79.5% |
| `mutate/*` | 2,400 | **600** (3 dead interactions of 12) | 25.0% |
| `input/*` | 2,304 | **288** (`text-1`, `text-200`, `swipe`) | 12.5% |
| `cross/X2,3,5,6,9,10` | 144 | **36** | 25.0% |
| `op/*` | 704 | **40** (5 dead ops × 8 templates) | 5.7% |
| `callback/*` | 1,612 | **432** (55 fns pushing a no-op change) | 26.8% |
| `manager/*` | 330 | **135** (9 managers with no op at all) | 40.9% |
| **Total** | | **~2,871** | |

The five dead `STAGES` (`gen_e2e_cases.py:657-671`) are: *type a character into
the focused node*, *undo the typing*, *copy the selection to the clipboard*,
*long-press to open a context menu*, *pinch-zoom the content*. A sixth, *focus a
contenteditable node and blink the caret*, is half-dead — focus works,
`StartCursorBlinkTimer` does not — which taints a further 108 `compose/2` lines.

The three dead `INTERACTIONS` (:186-199) are: *an undo stack recorded against the
node*, *a long-press gesture on the node*, *a pinch gesture centred on the node*.

### 3.4 Why this is the worst class

Take a dead-drive line and its four `input/*` assertions:

```
[input/liveness] mount a red stretched flexbox filling the window and deliver a text_input of a single character to it, assert the damage set is non-empty and the pixels actually changed
[input/settle]   mount a red stretched flexbox filling the window and deliver a swipe gesture to it, assert that after the event the window returns to idle with zero damage within 5 ticks
```

- `liveness` **fails loudly** — good, that is a working canary.
- `damage`, `bounded` and `settle` **pass trivially**, because nothing happened.

So for each dead event, 3 of 4 generated tests are vacuously green and 1 is red.
Across the 288 dead `input/*` lines that is **216 vacuous greens and 72 reds**.
The reds will look like engine bugs and burn triage time; the greens will be
counted as coverage of text input, which the suite does not have at all.

The same 3:1 ratio applies to `compose/*` (5 of 6 `COMPOSE_ASSERTS` are
settle/patch/counter-style and pass on a no-op timeline) and to `mutate/*` (4 of
5 `G4_ASSERTS` pass when the interaction never started).

---

## 4. Assertion-fit audit

Systematic sample: the whole `css/*` matrix (2,160 lines, judged by property
class), the whole `input/liveness` matrix (576 lines, judged by widget × event),
plus 40 hand-read lines across `compose/2`, `compose/3`, `mutate/*`, `op/*`,
`manager/*` and `callback/*`. Findings by class:

### 4.1 Assertion contradicts the action — 48 lines, 100% of the affected cells

`gen_e2e_cases.py:148-165` classifies `z-index` and `visibility` as
`paint-only`. The engine disagrees. `CssPropertyType::can_trigger_relayout`
(`css/src/props/property.rs:1554-1605`) lists the paint-only set explicitly, and
**neither `ZIndex` (`property.rs:923`) nor `Visibility` (`:999`) is in it** — so
the engine's own answer for both is `true`.

```
[css/paint-only] mount a red stretched flexbox filling the window and change its z-index via set_node_css_override, assert the pixels change, the damage is a patch, and it does not trigger a relayout
[css/paint-only] mount a red stretched flexbox filling the window and change its visibility via set_node_css_override, assert the pixels change, the damage is a patch, and it does not trigger a relayout
```

2 props × 24 widgets × 1 of the 3 paint-only assertions = **48 lines assert the
negation of what the engine declares**. The other 96 lines for those two props
are merely mis-tagged.

The rest of the CSS classification is **correct** and worth saying so: `color`,
`background-color`, `background-image`, `border-color`, `border-radius`,
`box-shadow`, `opacity`, `transform` all map to entries in the paint-only
exclusion list; `border-width`, `width`, `height`, `padding`, `margin`, the font
and text properties all correctly relayout; `cursor` is correctly inert.

### 4.2 Assertion cannot fail — vacuity

**`[css/none]`, 72 lines.** All three cursor assertions are negative:

```
[css/none] mount a red stretched flexbox filling the window and change its cursor via set_node_css_override, assert nothing is repainted at all and the damage stays None
[css/none] mount a red stretched flexbox filling the window and change its cursor via set_node_css_override, assert no relayout is triggered and the frame is byte-identical
```

These pass identically whether the engine is correct *or* whether
`set_node_css_override` did nothing at all. They need a positive control (assert
the computed `cursor` actually changed via `get_node_css_properties`) to have any
power. **Additionally**, the second one is likely to be **always-red for a
mechanical reason**: the runner's `ChangeNodeCssProperties` arm returns
`ProcessEventResult::ShouldIncrementalRelayout` **unconditionally**
(`runner.rs:465`), and `process_window_events` sets
`relayout_iterations = max(depth+1)` (`runner.rs:274`). So a cursor-only change
does report a relayout iteration in this host.

**`[manager/*]` lifecycle assertions applied to managers with no node keys — 90+
lines.** `MANAGER_LIFECYCLE` (`gen_e2e_cases.py:275-288`) is crossed with every
manager, including ones that have no per-node state:

```
[manager/clipboard] exercise the clipboard manager by copying a text selection, then assert it keeps exactly one entry per live node, never two
[manager/keyring] exercise the keyring manager by storing and reading a keyring secret from a callback, then assert it acquires state on the first relevant event
```

`ClipboardManager` holds two `Option<ClipboardContent>` fields
(`layout/src/managers/clipboard.rs`); `KeyringManager` holds
`last_result / in_flight / pending_event` (`keyring.rs`). "One entry per live
node", "remaps its key when a preceding sibling is inserted" and "survives a full
DOM rebuild without holding a dead key" are **not statements about these
objects**. Six of the twelve lifecycle assertions are node-key statements, so
~6 × 15 non-keyed managers ≈ **90 lines assert a property their subject does not
have**.

### 4.3 Assertion contradicts physics — `input/liveness`, ~274 of 576 (47.6%)

`INPUT_ASSERTS` (`gen_e2e_cases.py:344-352`) crosses all 24 events with all 24
widgets and asserts *"the damage set is non-empty and the pixels actually
changed"* for every cell. Many cells cannot change a pixel. Modelling each widget
by (has text / has focusable / has scrollable overflow) and each event by what it
can touch:

| Event | Widgets where a pixel change is implausible |
|---|---|
| `right-click`, `middle-click` | 24/24 (no `:active`/context styling in the seeds) |
| `move-noop` | 24/24 (*declared to change nothing*) |
| `up-outside` | 24/24 (no outstanding press) |
| `text-1`, `text-200` | 24/24 (runner no-op) |
| `swipe` | 24/24 (runner no-op) |
| `key-tab`, `key-esc` | 18/24 (no focusable node) |
| `wheel-v/h/d` | 16/24 each (no scrollable overflow) |
| `key-down`, `key-updown` | 11/24 each |

Total **274/576 = 47.6%**. The purest example:

```
[input/liveness] mount a red stretched flexbox filling the window and deliver a mouse_move that stays inside the node and changes nothing to it, assert the damage set is non-empty and the pixels actually changed
```

The event is *defined* as changing nothing, and the assertion demands change.
This is not a borderline judgement call — the generator will either emit a
guaranteed-red test or quietly rewrite the assertion, and both outcomes are bad.

*(This is a model, not a run. The assumptions are stated above and the widget
table is in `gen_e2e_cases.py:121-146`; the exact number will move by a few
percent depending on the seed DOMs the generator writes. The order of magnitude
— "about half" — is robust.)*

### 4.4 Ops the policy gate denies — 280 lines, 100% rejection

`OP_POLICY` (`doc/src/gene2e.rs:264-404`) is the law; `validate()` rejects the
denied half. The corpus's `op/*` family was expanded over **every** `DebugEvent`
variant (`gen_e2e_cases.py:378-384`) with no policy filter.

- **27 of the 88 corpus ops are explicitly denied** → 27 × 8 templates = **216
  lines**: the whole component/IDE family (`create_component`,
  `update_component_render_fn`, `import_component_library`, `export_code_zip`, …),
  the geometry queries (`get_node_layout`, `get_display_list`, `get_layout_tree`,
  `get_all_nodes_layout`, `get_virtual_view_layout`), plus `close`, `get_logs`,
  `open_file`, `resolve_function_pointers`, `run_e2e_tests`.
- **`redraw` and `relayout` are also denied** (`gene2e.rs:352-360`) — *"masks a
  broken invalidation path"*, the single most important rule in the whole policy
  — for a further 16 `op/*` lines **and** for 48 `idle/*` lines that were emitted
  with `op="Relayout"` / `op="Redraw"` (`gen_e2e_cases.py:341-347`):

```
[op/managers] drive the get_node_layout debug op against a mounted DOM, assert no manager key points at a node that does not exist afterwards
[idle/relayout-noop] mount a red stretched flexbox filling the window then issue a relayout op with no state change, assert the resulting damage is None and no pixel differs from the previous frame
```

**Total 280 lines (216 + 16 + 48) that the gate will reject outright.** Not
dangerous — they fail at generation time, loudly — but they are 2.1% of the
corpus spent on nothing, and they will produce a wall of gate errors that hides
real problems.

### 4.5 Underspecified — the generator must invent the test

Two systematic sources:

1. **The `empty` widget.** *"an empty body with no content at all"* is crossed
   with every CSS property: *"mount an empty body with no content at all and
   change **its** background-color"*. There is no node. 30 props × 3 assertions =
   **90 css lines** where the generator must invent a node, at which point the
   test is not the line.
2. **`assert_damage_sound` needs a `vs` snapshot.** `full.rs:6070-6075` hard-fails
   on a missing `vs` parameter. Every line saying *"pixel-identical to the
   full-repaint oracle"* (≈ 500 across `compose/*`, `css/*`, `scroll/*`,
   `input/damage`) requires the generator to insert a `snapshot_frame` step it was
   never told about. This one is **benign** — the assertion fails loudly rather
   than silently — but it is a systematic prompt gap worth closing.

### 4.6 Rate summary

| Class | Lines | of sampled base | Severity |
|---|---|---|---|
| Runner silently no-ops the interaction | ~2,871 | 21.7% of corpus | **critical — vacuous green** |
| Asserts a pixel change that cannot occur | ~274 | 47.6% of `input/liveness` | high — noisy red |
| Policy-denied op | 280 | 2.1% of corpus | medium — gate rejects |
| Invariant declared unimplemented | 96 | 40.0% of `cross/*` | medium — permanent red |
| Assertion contradicts the engine's own table | 48 | 6.7% of `css/paint-only` | medium |
| Assertion cannot fail (no positive control) | ~162 | — | medium — false coverage |
| Assertion doesn't apply to its subject | ~90 | 27.3% of `manager/*` | medium |
| Underspecified, generator invents content | ~590 | 4.5% of corpus | low–medium |

Overlaps are real (an `[op/*]` line can be both denied and dead); the union is
approximately **4,300 lines / 32.6%**.

---

## 5. API-surface coverage: `CallbackInfo`

### 5.1 The true number

The owner said 190; the coordinator's awk said 120. **Both are wrong, in
different directions.**

- `awk '/^impl.*CallbackInfo/,/^}/' layout/src/callbacks.rs | grep -cE '^\s*pub (fn|const fn|extern)'` → **120**, but that range also swallows `impl Clone for RenderImageCallbackInfo` (`callbacks.rs:4746`) and `impl RenderImageCallbackInfo` (`:4762`), and counts `pub const fn new`.
- Brace-matched over the single real `impl CallbackInfo` block (`callbacks.rs:798`): **118** unique `pub fn`.
- `api.json` `["0.2.0"]["api"]["callbacks"]["classes"]["CallbackInfo"]["functions"]`: **254** — these are the getters (`get_hit_node`, `get_scroll_state`, `is_dragging`, `inspect_*`, …), exposed through the FFI surface rather than the one Rust impl block.

**The defensible figure is 256**: `api.json` ∪ Rust impl, minus
`new`/`create`/`from_ptr`. That is exactly what the generator computes
(`gen_e2e_cases.py:52-70`) and it is the right definition — it is the surface an
app author can call.

### 5.2 Coverage

The corpus names **241 distinct `CallbackInfo::*` functions** across 1,612
`callback/*` lines (query fns get 6 templates, mutators 8, screenshots 4 —
`gen_e2e_cases.py:495-536`). Split against runner reachability:

| Bucket | Fns | Lines | Meaning |
|---|---|---|---|
| **(a) covered + drivable** | 62 mutators + 124 query/read-only = **186** | ~1,180 | generate now |
| **(b) covered + NOT drivable** | **55** | **432** | **silently vacuous — defer** |
| **(c) not covered at all** | **15** | 0 | gap |

**Bucket (b) — 55 functions whose `CallbackChange` hits `DoNothing`.** These are
the ones that will generate green tests for work that never happened:

`accept_drop, add_cursor, add_image_to_cache, add_selection_range, add_thread,
add_timer, begin_interactive_move, close_window, commit_undo_snapshot,
create_text_input, create_window, delete_backward, delete_forward, hide_tooltip,
inject_native_gesture, insert_text, move_cursor, move_cursor_down,
move_cursor_left, move_cursor_right, move_cursor_to_document_end,
move_cursor_to_document_start, move_cursor_to_line_end,
move_cursor_to_line_start, move_cursor_up, open_menu, open_menu_at,
open_menu_for_node, process_text_selection_click, queue_window_state_sequence,
redo_app_state, remove_image_from_cache, remove_selection_by_id, remove_thread,
remove_timer, request_hit_test_update, reset_cursor_blink,
scroll_active_cursor_into_view, set_copy_content, set_cursor_visibility,
set_cut_content, set_drag_data, set_drop_effect, set_route_param,
set_select_all_range, set_selection, set_text_changeset, show_tooltip,
show_tooltip_at, start_cursor_blink_timer, stop_cursor_blink_timer, switch_route,
take_changes, take_screenshot_base64, undo_app_state`

Verbatim examples of the resulting tests:

```
[callback/effect] call CallbackInfo::set_copy_content from a click callback and assert the resulting frame actually refreshes: damage is non-empty and the pixels differ
[callback/effect] call CallbackInfo::add_timer from a click callback and assert the resulting frame actually refreshes: damage is non-empty and the pixels differ
[callback/patch] call CallbackInfo::insert_text from a click callback and assert the repaint it causes is an incremental Rects patch, never a full redraw, and matches the full-repaint oracle pixel for pixel
```

`add_timer` asserting that adding a timer repaints pixels is wrong even in the
DLL; in the runner it is doubly meaningless.

**Bucket (c) — 15 uncovered functions** (added to the surface after the corpus
was generated at `fe981ccd5`):

`find_scroll_parent, get_deepest_hovered_node, get_drag_data, get_dropped_files,
get_hovered_files, get_multi_cursor_selections, get_node_hit_test_bounds,
get_permission_status, get_previous_window_state, get_primary_selection,
get_route_param, get_route_pattern, get_system_style, has_any_selection,
has_pending_relayout_change`

All 15 are read-only queries, so they are cheap to add and — importantly —
**7 of them are drivable today** (`find_scroll_parent`,
`get_deepest_hovered_node`, `get_node_hit_test_bounds`,
`get_previous_window_state`, `get_system_style`, `has_pending_relayout_change`,
`get_multi_cursor_selections`). No stale references: every function the corpus
names still exists.

---

## 6. API-surface coverage: input events

### 6.1 What the surface actually is

`core/src/dom.rs:1152` `pub enum On` has **25 variants, not 30**. The extra five
the coordinator listed (`Id`, `Class`, `IdOrClass`, `OptionIdOrClass`, `Self`)
belong to `enum IdOrClass` at `core/src/dom.rs:1238-1243` — a *selector* type,
not an event type.

Within `On`'s 25: **20 are real input events** (`MouseOver`, `MouseDown`,
`Left/Middle/RightMouseDown`, `MouseUp`, `Left/Middle/RightMouseUp`,
`MouseEnter`, `MouseLeave`, `Scroll`, `TextInput`, `VirtualKeyDown`,
`VirtualKeyUp`, `HoveredFile`, `DroppedFile`, `HoveredFileCancelled`,
`FocusReceived`, `FocusLost`) and **5 are semantic/a11y actions, not input**
(`Default`, `Collapse`, `Expand`, `Increment`, `Decrement`).

But `On` is only the convenience shorthand. The vocabulary the engine actually
dispatches on is `EventFilter` (`core/src/events.rs:2285`) over five concrete
families — **188 filter slots, 88 distinct event names**:

| Family | Variants | Source |
|---|---|---|
| `HoverEventFilter` | 65 | `core/src/events.rs:1648` |
| `FocusEventFilter` | 47 | `:1903` |
| `WindowEventFilter` | 66 | `:2013` |
| `ComponentEventFilter` | 6 | `:2249` |
| `ApplicationEventFilter` | 4 | `:2267` |

**The corpus does not know this enum exists.** Its event axis is a hand-written
list of 24 informal descriptions (`gen_e2e_cases.py:200-225`) — "a single left
click", "a swipe gesture" — never derived from `EventFilter`. So the owner's
"every input event type should be covered" is not achieved and is not even
measured.

### 6.2 Coverage matrix, three buckets

| Bucket | Count | Names |
|---|---|---|
| **(a) drivable today** | **40 / 88** | `MouseOver, MouseDown, Left/Right/MiddleMouseDown, MouseUp, Left/Right/MiddleMouseUp, MouseEnter, MouseLeave, MouseOut, Scroll, ScrollStart, ScrollEnd, VirtualKeyDown, VirtualKeyUp, TouchStart/Move/End/Cancel, PenDown/Move/Up, FocusReceived, FocusLost, FocusIn, FocusOut, WindowFocusReceived, WindowFocusLost, Resized, Moved, DpiChanged, DoubleClick, SystemText{Single,Double,Triple}Click, DragStart, Drag, DragEnd` |
| **(b) corpus targets it, runner no-ops it** | **16 / 88** | `TextInput, LongPress, Swipe{Left,Right,Up,Down}, PinchIn, PinchOut, Rotate{Clockwise,CounterClockwise}, Composition{Start,Update,End}, Copy, Cut, Paste` |
| **(c) no debug op exists at all** | **32 / 88** | `HoveredFile, DroppedFile, HoveredFileCancelled, DragEnter, DragOver, DragLeave, Drop, GeolocationFix, GeolocationError, SensorChanged, GamepadInput, PermissionChanged, BiometricResult, KeyringResult, PenEnter, PenLeave, PenSqueeze, PenDoubleTap, PenHover, ThemeChanged, MonitorChanged, CloseRequested, AfterMount, BeforeUnmount, NodeResized, DefaultAction, Selected, Updated, Device{Connected,Disconnected}, Monitor{Connected,Disconnected}` |

**45% of the engine's event vocabulary is genuinely covered. 18% is covered on
paper but silently dead. 36% is untouched.**

Bucket (c) is where the whole tail-manager problem comes from: `file_drop`,
`drag_drop`, `geolocation`, `sensors`, `gamepad`, `permission`, `biometric`,
`keyring` and `a11y` are each unreachable precisely because their event families
have no debug op. And `ComponentEventFilter::AfterMount` / `BeforeUnmount` /
`NodeResized` being uncovered is a notable miss — those are ordinary app-level
lifecycle events with a known history of bugs (see the AfterMount-on-X11 gap in
the slippy-map work), and they need no new host facility, only a debug op.

---

## 7. Structural problems beyond the dangling-referent fix

### 7.1 The runner must fail loudly, not silently — highest leverage fix in this document

`runner.rs:728`'s `_ => DoNothing` is the root cause of ~2,871 dangerous lines
and 432 dangerous callback lines. **Change it to panic (or return a poisoned
result the harness reports as `unsupported`) naming the dropped variant.** Then:

- every vacuous test becomes a loud, actionable failure;
- the corpus needs no triage pass to find them — running it *is* the triage pass;
- `gene2e.rs`'s zombie machinery gains its missing second layer for free.

This is a ~20-line change and it converts the corpus's single worst property
(silent false coverage) into its single most useful one (a precise, automatically
generated list of what the runner still has to port).

### 7.2 Extend `Schema::is_zombie` to the `CallbackChange` layer

`is_zombie` (`gene2e.rs:209`) checks *"declared in `DebugEvent`, no match arm in
`full.rs`"*. Add a second predicate: *"the op's dispatch arm pushes a
`CallbackChange` that `runner.rs` does not handle"*. Both facts are derivable by
the same dumb line-scanner already in `parse_schema`. That makes `text_input`,
`swipe`, `pinch`, `rotate`, `long_press` fail the gate today and un-fail
themselves automatically the moment someone ports the arm — the same self-healing
property `implementing_a_zombie_re_enables_it_automatically` (:1913) already
proves for layer one.

### 7.3 The generator cannot be re-run

`gen_e2e_cases.py:37` reads
`FULL_RS = dll/src/desktop/shell2/common/debug_server/full.rs`. **That file was
deleted in `89d66d7bf` ("refactor(debug-server): delete the DLL's duplicated
12k-line op dispatcher")**; the schema now lives at `layout/src/e2e/full.rs`,
which is what `gene2e.rs:65` reads. `debug_ops()` is called at module scope
(:99), so `gen_e2e_cases.py` and `--check` both raise `FileNotFoundError` before
doing anything. The checked-in corpus is therefore **frozen and unverifiable**.
(Consolation: the 88 ops it captured are all still present in today's 96-variant
enum, and the 8 new ones are harness-control ops — `mount`, `unmount`, `tick_ms`,
`snapshot_frame`, `snapshot_resources`, `get_frame_report`, `capture_damage_png`,
`reset_frame_counters` — exercised implicitly everywhere. So the corpus is stale
but not wrong on this axis.) Flagging for the owning agent; not fixed here.

### 7.4 Apply `OP_POLICY` and `KNOWN_CROSS` at generation time

The corpus is expanded over raw enum listings — `os.listdir(managers/)`, every
`DebugEvent` variant, `X1..X10` from the plan document. The policy tables that
already encode which of those are usable (`OP_POLICY`, `KNOWN_MANAGERS`,
`KNOWN_CROSS`, `UNIMPLEMENTED_CROSS`) are consulted only later, by the validation
gate. Consulting them in the expander removes 280 + 96 + 210 = **586 lines** with
no loss of coverage, and stops the corpus drifting from the policy again.

### 7.5 Assertion polarity should be a property of the cell, not the row

`INPUT_ASSERTS` and `PROP_ASSERTS` apply one fixed assertion to every widget.
Half the `input/liveness` matrix asserts "pixels changed" where nothing can
change (§4.3). The fix is a per-cell predicate — the generator already has the
widget's shape in `WIDGETS` and the event's nature in `INPUT_EVENTS`; emitting
`assert the damage stays None` instead of `assert the pixels changed` when the
pair cannot interact turns ~274 noisy reds into ~274 genuine
**over-invalidation** tests, which is a real and valuable bug class the corpus
currently does not cover at all.

### 7.6 Separate the two undo systems

`[manager/undo_redo]`, `[undo/mutate]`, `[undo/renumber]`, the `undo the typing`
compose stage and the `undo stack recorded against the node` interaction all read
as one thing but span two unrelated objects: `LayoutWindow::undo_redo_manager`
(per-node text history, `window.rs:795`) and the E2E session's
`RefAnyUndoManager` over `app_data` (`full.rs:6530`). Only the second is
drivable, and only the first is what `assert_manager_invariants` inspects. These
need distinct tags — e.g. `[undo/app-state]` vs `[undo/node-text]` — or every
undo test will measure the wrong object.

---

## 8. Recommended generation order

### Wave 1 — generate now (~5,600 lines, highest bug-finding value per token)

| Tag family | Lines | Why |
|---|---|---|
| `mutate/*` minus 3 dead interactions | 1,800 | **The payload.** Dangling `NodeId` under mid-interaction mutation is exactly what X10 was written for, all 9 live interactions are mouse/scroll/pen-driven, and `assert_manager_invariants` really checks it (`full.rs:5477-5578`). |
| `css/layout` + `css/structural` | 1,368 | Property classification verified correct against `can_trigger_relayout`; `set_node_css_override` has a real runner arm with a documented stale-screen fix (`runner.rs:452-462`) that these will regression-guard. |
| `input/*` minus dead events and minus contradictory cells | ~1,300 | Mouse/scroll/touch/pen/resize/dpi all drive `ModifyWindowState` correctly. |
| `resize/*` + `dpi/*` | 483 | Fully drivable; `resize_pending` → `clear_caches()` (`runner.rs:775-779`) is exactly the kind of invalidation path that breaks quietly. |
| `scroll/*` | 205 | `scroll_manager` is the best-covered, best-instrumented manager; X2/X9 both check it. |
| `idle/*` minus the 48 denied | 72 | Cheapest possible regression net. |
| `leak/*` | 291 | `snapshot_resources` is a real op; counters are real. |
| `cross/X2,X3,X5,X6,X9,X10` minus dead interactions | 108 | The only cross-invariants that are implemented. |
| `callback/*` bucket (a) | ~1,180 | Drivable; the query half is a genuine dangling-referent net (`callback/stale`, `callback/mutation`). |

*(Overlapping; the union is ~5,600 after de-duplication against shared lines.)*

### Wave 2 — after the runner fails loudly (§7.1)

Regenerate nothing; just **run** Wave 1 with the loud runner. It will emit the
exact list of `CallbackChange` variants the corpus needs. Port them in that
order. Expect `CreateTextInput`, `InsertText`, `DeleteBackward`/`Forward` and the
`MoveCursor*` family to dominate — porting those alone unlocks `text_edit`,
`text_input`, `selection`, `changeset` and node-level `undo_redo`, i.e. five
managers and ~1,500 corpus lines.

### Wave 3 — after `InjectNativeGesture` + text editing land (~2,900 lines)

`compose/2`, `compose/3`, the 3 dead `mutate/*` interactions, the dead
`input/*` events, `callback/*` bucket (b) for the newly-ported variants. These
are the highest-value lines in the corpus *once they can run* — multi-stage
manager chains are precisely where cross-manager disagreement hides — which is
why they must not be generated before then.

### Wave 4 — needs new debug ops and probably a mock host

`manager/*` for the 9 hostless managers (135 lines), the 32 bucket-(c) event
types, `file_drop`, `drag_drop`, clipboard. Consider a `MockHostFacilities` shim
on the runner rather than porting each; the state these managers hold is small
(`KeyringManager` is 3 fields, `ClipboardManager` is 2).

### Drop outright (586 lines)

- **280** policy-denied `op/*` and `idle/{relayout,redraw}-noop` lines (§4.4).
- **96** `cross/X1|X4|X7|X8` lines (§2) — regenerate if and when those
  invariants are implemented; X4 should be deleted permanently, it is
  unfalsifiable.
- **90** `[manager/*]` node-key lifecycle assertions applied to non-keyed
  managers (§4.2).
- **15** `[manager/changeset]` lines — `changeset` is not a manager.
- **48** `[css/paint-only]` z-index/visibility "does not trigger a relayout"
  lines, or re-tag both properties as `layout` (§4.1).
- **72** `[css/none]` lines unless a positive control is added (§4.2).

---

## 9. Suggested new corpus families to close the gaps

Same `[category/sub] description` format. **Not written to the corpus** — the
generator's owner should add them.

### 9.1 The 15 uncovered `CallbackInfo` functions (7 drivable now)

```
[callback/read] call CallbackInfo::get_node_hit_test_bounds from inside a callback fired by a click and assert it returns a rect consistent with the current DOM and that merely calling it produces NO damage and NO relayout
[callback/read] call CallbackInfo::find_scroll_parent on a node three levels inside a nested scroll container and assert it names the INNER container, and that calling it produces no damage
[callback/read] call CallbackInfo::get_deepest_hovered_node while the cursor sits over two overlapping nodes and assert it names the topmost one and not the container
[callback/read] call CallbackInfo::has_pending_relayout_change on an idle frame and assert it is false, then immediately after a width override and assert it is true
[callback/read] call CallbackInfo::get_previous_window_state after a resize and assert it reports the PRE-resize dimensions while get_current_window_state reports the new ones
[callback/read] call CallbackInfo::get_system_style from a callback and assert it returns the same handle on every one of 20 idle frames and no counter grows
[callback/read] call CallbackInfo::get_multi_cursor_selections after a drag-select across three rows and assert the returned ranges match the selection the text_edit manager reports
[callback/stale] call CallbackInfo::find_scroll_parent from a callback that runs one frame AFTER the scroll container was deleted, assert it returns None instead of a dangling id
[callback/mutation] call CallbackInfo::get_node_hit_test_bounds immediately after a preceding sibling was inserted so every following NodeId shifted, assert the rect refers to the same LOGICAL node as before the shift
```

The remaining 8 (`get_drag_data`, `get_dropped_files`, `get_hovered_files`,
`get_permission_status`, `get_primary_selection`, `get_route_param`,
`get_route_pattern`, `has_any_selection`) belong in Wave 4 — write them, tag them
`[callback/read]`, but gate them behind their host facility.

### 9.2 Over-invalidation — the missing bug class (~274 lines, free)

Flip the polarity of every `input/liveness` cell that cannot change pixels. This
costs nothing (the cells already exist) and covers a class the corpus has zero of:

```
[input/noop] mount a red stretched flexbox filling the window and deliver a mouse_move that stays inside the node and changes nothing to it, assert the damage stays None and the frame is byte-identical to the previous one
[input/noop] mount a 10x10 grid of coloured boxes and deliver a vertical wheel scroll to it, assert that because nothing is scrollable the damage stays None rather than repainting the grid
[input/noop] mount a paragraph of selectable text and deliver a key_down of the Tab key, assert that with no focusable node present focus stays None and no repaint is generated
[input/noop] mount an image node inside a flex row and deliver a right click, assert no damage is produced when no context styling exists
```

### 9.3 Uncovered event types that need no new host facility

`ComponentEventFilter` (`core/src/events.rs:2249`) and the window-lifecycle
filters are drivable with a small debug-op addition and are ordinary app surface:

```
[event/component] mount a tab strip with three switchable panels and switch a tab so a panel subtree is replaced, assert AfterMount fires exactly once for the new panel and BeforeUnmount exactly once for the old, and no manager keeps a key from the unmounted subtree
[event/component] mount a form with three focusable fields and resize the window so a field's box changes, assert NodeResized fires for the resized node and not for its unchanged siblings
[event/component] mount a tree view with expandable nodes and expand one, assert Expand fires once, the subtree mounts, and the window settles to zero damage
[event/window] mount a vertically scrollable list of 40 rows and change the system theme, assert ThemeChanged fires once, the repaint covers every node whose computed colour changed, and no relayout is spent
[event/window] mount a form with three focusable fields, move window focus away and back, assert WindowFocusLost then WindowFocusReceived fire exactly once each and the caret blink state matches
```

### 9.4 The two undo systems, disambiguated

```
[undo/app-state] commit an app-state snapshot, mutate the app state so the DOM rebuilds, then undo_app_state, assert the DOM returns to the snapshot's shape and no manager holds a key from the intermediate tree
[undo/node-text] type into a contenteditable node to build LayoutWindow::undo_redo_manager.node_stacks, assert exactly one stack exists for that node and its node_id resolves live
[undo/node-text] type into node B to build a node-text undo stack, insert a new node BEFORE B so every following NodeId shifts, then assert the stack still keys B and not whatever node inherited B's old NodeId
[undo/node-text] type into a node to build a node-text undo stack, delete the node, assert the stack is dropped rather than left keying a dead NodeId (undo_redo is NOT in update_managers_with_node_moves)
```

That last one is worth writing today even though it cannot run: `full.rs:5605`
flags in its own violation message that `undo_redo` is **not** in
`update_managers_with_node_moves`. That is a live bug the corpus should be
pointing at.

### 9.5 Positive controls for the negative-assertion families

```
[css/none] mount a single grey button with a :hover rule and change its cursor via set_node_css_override, assert get_node_css_properties reports the NEW cursor value AND the damage stays None (the property landed, the screen did not move)
[manager/clipboard] exercise the clipboard manager by copying a text selection, then assert pending_copy_content holds exactly one entry and is replaced, not appended to, by a second copy
[manager/keyring] exercise the keyring manager by storing a secret, then assert in_flight returns to 0 and last_result is Some once the operation resolves
```

---

## 10. What is solid — stated plainly

Not everything is a finding, and the following should not be touched:

- **`mutate/*` (2,400 lines)** is the best family in the corpus. The
  interaction × mutation × phase × assertion cross is exactly right, `MUTATIONS`
  (`gen_e2e_cases.py:169-178`) names the real renumbering hazards, and `PHASES`
  (:180-186) hits the mid-animation and pre-up windows where dangling keys
  actually appear. 1,800 of its lines are drivable today.
- **The CSS property → relayout classification is correct** for 28 of 30
  properties, verified line-by-line against
  `css/src/props/property.rs:1554-1605`.
- **`assert_damage_sound` (`full.rs:6029`) is a genuinely strong assertion** —
  present ⊇ paint, a tightness bound, and optional pixel-identity against an
  independent full repaint. It is not a stub, and the ~500 corpus lines that
  invoke it are well spent.
- **`assert_manager_invariants`' refusal semantics are exemplary.** Unknown
  manager → fail; unimplemented invariant → fail *with the reason*; never a
  silent pass. The corpus's problem is that it ignores those tables, not that the
  tables are wrong.
- **`OP_POLICY`'s `redraw`/`relayout` denial** (`gene2e.rs:352-360`) is the
  single most important rule in the pipeline and its stated reasoning is exactly
  right.
- **Uniqueness by construction** (`gen_e2e_cases.py:284-300`) with a hard assert
  on collision is the right design; 13,223 lines, zero duplicates.
- **The hand-authored `SEEDS` block** (`gen_e2e_cases.py:722-775`) is the highest
  signal-per-line content in the file. `[damage/disjoint]`, `[damage/scrolled]`,
  `[text/stale]`, `[hover/stale]`, `[scroll/damage]` each name a specific,
  plausible, previously-observed bug. More of these is worth more than more
  combinatorics.

---

*Reviewed against: `layout/src/e2e/runner.rs`, `layout/src/e2e/full.rs`,
`layout/src/window.rs`, `layout/src/callbacks.rs`, `layout/src/managers/*.rs`,
`core/src/dom.rs`, `core/src/events.rs`, `css/src/props/property.rs`,
`doc/src/gene2e.rs`, `scripts/gen_e2e_cases.py`, `scripts/E2E_TESTS.txt`.
No corpus or generator file was modified.*

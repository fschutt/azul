# E2E Protocol Audit — is the op surface the headless window's I/O boundary?

Audited against the working tree at `master` = `983357ebd` (note: the brief said
`338f096bd`; the tree I read is `983357ebd`, working tree clean except
`scripts/SPEC_CONFORMANCE_REVIEW.md`).

**Thesis under test:** the headless window is a server; a client sends generic input
ops and gets damage patches back; the e2e harness is just a client of that protocol.
Therefore the op surface should be *exactly* the headless window's I/O boundary.

**Verdict up front:** the thesis is structurally sound — nothing below the boundary is
platform-bound (§5). But the op surface today is **not** the boundary. It is missing
about a dozen real input channels, it cannot read back most of what a presenter reads,
five of its 88 ops are declared-but-silently-ignored, and roughly a third of it is
IDE/component tooling that has nothing to do with a window. The single worst finding is
not in the input list at all: **the image/texture-callback damage contract is broken by
construction, on every platform, and is completely unreachable from headless** (§2.C).

---

## 0. Where the surfaces actually are

There are **two** unrelated mock-event surfaces in the tree, and only one of them is the
"op protocol":

| Surface | Definition | Transport | Applied via |
|---|---|---|---|
| `DebugEvent` (the ops, 88 variants — counted) | `dll/src/desktop/shell2/common/debug_server/full.rs:1526` (`#[serde(tag = "op", rename_all = "snake_case")]`, `full.rs:1525`) | HTTP `POST /` → `handle_event_request` (`full.rs:3118`) → spmc → a **16 ms timer inside the window** (`create_debug_timer`, `full.rs:~10248`) | `process_debug_event` (`full.rs:6137`), against a `CallbackInfo` |
| `HeadlessEvent` | `dll/src/desktop/shell2/headless/mod.rs:87–116` | in-process queue, `poll_event()` | the headless run loop, `headless/mod.rs:1290–1560` |

They do **not** have the same coverage. `HeadlessEvent` has `FileHover` / `FileDrop` /
`FileHoverCancel` (`headless/mod.rs:109,112,115`) which `DebugEvent` does not have at
all. `DebugEvent` has touch/pen/gestures which `HeadlessEvent` does not have. This split
is itself pollution: two mock surfaces means two chances to drift, and they already have.

The engine's real input boundary is narrower and much simpler than either: platform
shells mutate `FullWindowState` (`layout/src/window_state.rs:86`) and poke a handful of
managers, then call `process_window_events()`. Everything else is derived by state-diffing
(`common/event.rs:20–45`).

---

## 1. THE INPUT BOUNDARY (union over all six shells)

For each channel: what the shells actually call, and whether an op exists.

### 1.1 Mouse — COVERED
- Position: `x11/events.rs:568,622,668`; `wayland/mod.rs:2706,3081,3324,3361,3399`;
  `windows/mod.rs:1615,3088,3187,3298,3383,3459,3522`; `macos/events.rs:169,300,357,377`;
  `android/mod.rs:629` (`ms`); `ios/mod.rs:351`.
- Buttons: `x11/events.rs:572–574`; `wayland/mod.rs:2882–2891`; `windows/mod.rs:3300,3385,3461,3524`;
  `macos/events.rs:173–175,215–217`.
- Cursor enter/leave: `x11/events.rs:668,672`; `wayland/mod.rs:3081` (leave), `3324` (enter).
- Ops: `MouseMove` / `MouseDown` / `MouseUp` / `Click` / `DoubleClick` (`full.rs:1528–1568`). **OK.**
- Gap (minor): no op sets `CursorPosition::OutOfWindow` — mouse-leave is not expressible.

### 1.2 Scroll — PARTIAL
All shells funnel into `ScrollManager::record_scroll_from_hit_test`
(`layout/src/managers/scroll_state.rs:533`), whose signature takes
`source: ScrollInputSource` (line 537): `x11/events.rs:711`, `wayland/mod.rs:2962` and
`3035` (two different axis sources), `macos/events.rs:483`, `windows/mod.rs:3713`,
headless `headless/mod.rs:1522`.
- Op: `Scroll { x, y, delta_x, delta_y }` (`full.rs:1569`) — **no `source` field**. Wheel-vs-touchpad
  (line vs pixel) scrolling is a real behavioural fork in the physics timer and is not expressible.

### 1.3 Keyboard — COVERED
`x11/events.rs:836,917`; `wayland/mod.rs:2357`; `macos/events.rs:1065`; `android/mod.rs:703`.
Ops `KeyDown`/`KeyUp` with `modifiers` (`full.rs:1577–1586`). **OK.**

### 1.4 Text / IME — HOLE (composition)
- Committed text: op `TextInput { text }` (`full.rs:1587`). **OK.**
- **Pre-edit / composition**: shells call `lw.text_edit_manager.set_preedit(...)` —
  `wayland/mod.rs:1918`, `macos/mod.rs:1214` and `1882`; Windows handles
  `WM_IME_STARTCOMPOSITION` / `WM_IME_COMPOSITION` (`windows/mod.rs:2684,2685`); X11 uses XIM
  (`common/event.rs:110,115`).
  **There is no op that drives `set_preedit`.** The entire IME composition state machine
  (preedit string, preedit range, commit, cancel) is untestable and unreachable from a client.

### 1.5 Touch — COVERED (user's suspicion REFUTED)
Shells merge into `current_window_state.touch_state`: `x11/mod.rs:629`,
`wayland/mod.rs:2598,2636,2659`, `windows/mod.rs:2590` (WM_POINTER), `android/mod.rs:651`,
`ios/mod.rs:365`. macOS **does not** inject touch (`macos/mod.rs:3779` is
`touch_state: Default::default()` and nothing writes it — so this is not a hole, it is a
platform that has no touch).
Ops `TouchStart`/`TouchMove`/`TouchEnd`/`TouchCancel` exist (`full.rs:1594–1611`) and are
implemented against `touch_state` (`full.rs:8874–8926`). **Not a hole.**

### 1.6 Pen / stylus — PARTIAL HOLE (user's suspicion PARTIALLY CONFIRMED)
All six pen-capable shells call the gesture manager:
`x11/mod.rs:684`, `wayland/mod.rs:2673` (`handle_tablet_frame`, fed by the
`zwp_tablet_tool_v2` listeners at `wayland/events.rs:798–858`), `windows/mod.rs:2556`,
`macos/events.rs:267` (`feed_tablet_pen`), `android/mod.rs:682`, `ios/mod.rs:416`.

The engine entry point is `GestureAndDragManager::update_pen_state_full`
(`layout/src/managers/gesture.rs:915`) which takes **eleven** parameters:
`position, pressure, tilt, in_contact, is_eraser, barrel_button_pressed, device_id,
tangential_pressure, barrel_roll_rad, tool_id`.

The ops `PenDown`/`PenMove`/`PenUp` (`full.rs:1614–1637`) carry only
`x, y, pressure, x_tilt, y_tilt`. **Missing entirely: `is_eraser`, `barrel_button_pressed`,
`tangential_pressure`, `barrel_roll_rad`, `device_id`, `tool_id`, and proximity
in/out** — even though the shells feed all of them (eraser: `wayland/events.rs:800`,
`android/mod.rs:597` barrel button; rotation: `wayland/events.rs:841`; proximity:
`wayland/events.rs:813` `tool_proximity_out`). The eraser tip and the barrel button are
*behavioural* (a paint app switches tool on them) and cannot be tested.

### 1.7 Gestures — COVERED (but synthetic)
Ops `Swipe`/`Pinch`/`Rotate`/`LongPress` (`full.rs:1642–1675`). These bypass the in-process
detector and write the manager's override slot (per their own doc comment, `full.rs:1639–1641`).
Fine for callback testing; note they do **not** test the detector itself — that is tested
by driving raw touch/mouse.

### 1.8 File drag & drop — HOLE
Shells: `macos/events.rs:897,933`, `windows/mod.rs:1653,1696`, and the headless mirror
`headless/mod.rs:1310,1327,1348` all drive `lw.file_drop_manager`
(`set_hovered_files` / `set_dropped_files` / `clear_hover_cancelled`).
`HeadlessEvent::FileHover|FileDrop|FileHoverCancel` exist (`headless/mod.rs:109–115`).
**No `DebugEvent` op exists.** So drag-and-drop is testable via the in-process headless API
but *not* over the protocol — exactly the drift the thesis is meant to eliminate.

### 1.9 Clipboard — HOLE (both directions)
Paste is read by `get_system_clipboard()` (`common/event.rs:261–281`), a **compile-time
platform call**: on Linux it calls the real X11 clipboard (`common/event.rs:272` →
`x11/clipboard.rs:96`) regardless of whether we are headless or on Wayland. It is applied
via `clipboard_manager.set_paste_content` (`common/event.rs:2573`, `4512`).
**There is no op to set the clipboard content a paste will see**, so any Ctrl-V test reads
whatever is in the developer's/CI machine's clipboard — nondeterministic. There is likewise
no op to read what the app *copied* (the output side, §3).

### 1.10 Focus in / out — HOLE (worse: a lie)
Shells: `x11/mod.rs:2592,2605`; `wayland/mod.rs:3094,3294`; `macos/mod.rs:2403,2440`.
Ops `Focus` and `Blur` are **declared** (`full.rs:1686,1687`) but there is **no match arm**
for them in `process_debug_event`. They fall into the catch-all at `full.rs:10224`, which
logs `"Unhandled: …"` and then **`send_ok(request, None, None)`** — the client gets `ok`
and nothing happened. See §2.A.

### 1.11 Resize — COVERED. `Resize` op (`full.rs:1678`) implemented at `full.rs:6176`.

### 1.12 DPI / scale change — HOLE (declared, ignored)
Shells: `x11/mod.rs:2729–2748`; `wayland/events.rs:217–326` (three separate scale paths);
`windows/mod.rs:2877` (WM_DPICHANGED); `macos/events.rs:808–817`, `macos/mod.rs:4346–4394`;
`android/mod.rs:443`.
Op `DpiChanged { dpi }` is **declared** (`full.rs:1689`) and **unhandled** (catch-all).

### 1.13 Window move — HOLE (declared, ignored)
Shells: `x11/mod.rs:2119`; `windows/mod.rs:3054`; `macos/mod.rs:2483,4124`.
Op `Move { x, y }` **declared** (`full.rs:1682`), **unhandled** (catch-all).

### 1.14 Window frame state (minimize / maximize / fullscreen, from the OS) — HOLE
Shells: `wayland/events.rs:1501`; `windows/mod.rs:2867`; `macos/mod.rs:2275,2288,2324,2337`.
**No op exists.** A client cannot tell the engine "the WM just maximized you".

### 1.15 Theme / appearance change — HOLE
Only Windows currently injects a live theme change:
`windows/mod.rs:4406–4407` (`current_window_state.theme = …`). The others compute a
`SystemStyle` at startup (`linux/system_style.rs:592`, `macos/system_style.rs:234`).
**No op exists** to flip `theme` at runtime, so `@theme()` CSS re-cascade is untestable
(note `GetComponentPreview` has an `override_theme` — but that is the component previewer,
not the window).

### 1.16 Monitor change — HOLE
`windows/mod.rs:3042` sets `monitor_id`; `macos/mod.rs:3055` `detect_current_monitor`,
`macos/mod.rs:2513,2529` refresh `lw.monitors` on display reconfiguration.
**No op exists.**

### 1.17 Close — COVERED (`Close`, `full.rs:1688`, handled).

### 1.18 Timers / animation ticks — HOLE (user's suspicion CONFIRMED)
The engine is time-driven: cursor blink (`common/event.rs:1526`), scroll momentum, long-press
(`common/event.rs:4237`), drag autoscroll (`common/event.rs:3243`), CSS animations — all via
`start_timer` + `process_timers_and_threads` (`common/event.rs:5133`), which read the **real**
clock (`headless/mod.rs:1521`, `Instant::from(std::time::Instant::now())`).

- There is **no virtual clock op**. `tick_ms` exists only in commit `3a0350ac1`, which is
  **not an ancestor of HEAD** (verified with `git merge-base --is-ancestor` → false; it lives
  on the two agent worktree branches).
- `WaitFrame` (`full.rs:1838`) is implemented at `full.rs:6681–6683` as **`send_ok` and nothing
  else** — it is a no-op that does not wait for a frame.
- `Wait { ms }` (`full.rs:1839`) is implemented as `std::thread::sleep`
  (`full.rs:6685–6687`) **inside the debug timer callback**, i.e. it blocks the UI thread it is
  supposed to be observing.
- There is **no query** for "what timers/animations are pending" or "is the UI settled".

This is why animation/timing is the flakiest thing in the suite: the only synchronisation
primitive is a real sleep on the thread being tested.

### 1.19 Gamepad / sensors — HOLE (low priority)
`common/capability_pump.rs:56` `pump()` calls `extra::gamepad::poll()` (line 63) and
`extra::sensors::poll()` (line 72) on a shared timer (`common/event.rs:5539`). Real devices
only; **no injection op**. Not platform-shell-injected (it's cross-platform, timer-driven),
so it is arguably *inside* the boundary — but it is real input with no mock.

---

## 2. DIFF AGAINST `DebugEvent`

### 2.A The five zombie ops (declared, silently `ok`) — fix first, it's free
`process_debug_event`'s catch-all (`full.rs:10224–10232`) logs a warning and calls
`send_ok`. These five variants have **no match arm anywhere in the file** (verified by
grepping `DebugEvent::<Variant>` — zero hits outside the declaration):

| Op | Declared | Handler |
|---|---|---|
| `Focus` | `full.rs:1686` | **none → returns `ok`** |
| `Blur` | `full.rs:1687` | **none → returns `ok`** |
| `Move { x, y }` | `full.rs:1682` | **none → returns `ok`** |
| `DpiChanged { dpi }` | `full.rs:1689` | **none → returns `ok`** |
| `GetDom` | `full.rs:1695` | **none → returns `ok`** |

A test that focuses the window, changes the DPI, or moves it *passes* while doing nothing.
This is worse than a missing op: a missing op fails loudly.

### 2.B HOLES — real platform-injected input with no working op

Ordered by how much engine behaviour they gate.

| # | Hole | Platform evidence | Engine entry point |
|---|---|---|---|
| H1 | **Texture / image-callback redraw + its damage** (see §2.C — the big one) | `x11/mod.rs:3583`, `windows/mod.rs:885`, `wayland/mod.rs:4465`, `macos/mod.rs:6114` | `LayoutWindow::invoke_cpu_image_callbacks` (`layout/src/window.rs:1545`), `CallbackChange::UpdateImageCallback` (`layout/src/callbacks.rs:226`) |
| H2 | **Virtual clock / frame settle** (`tick_ms`, `wait_until_idle`, `get_pending_timers`) | all shells; timers at `common/event.rs:5133` | real `Instant` (`headless/mod.rs:1521`) |
| H3 | **IME preedit / composition** | `wayland/mod.rs:1918`, `macos/mod.rs:1214,1882`, `windows/mod.rs:2684–2685` | `text_edit_manager.set_preedit` |
| H4 | **File drag & drop** (hover / drop / cancel) | `macos/events.rs:897,933`, `windows/mod.rs:1653,1696` | `file_drop_manager` (`headless/mod.rs:1310` shows the exact ingress) |
| H5 | **Clipboard inject (paste source) + clipboard readback** | `common/event.rs:261–281`, `x11/clipboard.rs:46` | `clipboard_manager.set_paste_content` / `get_copy_content` |
| H6 | **Focus / Blur** (zombie) | `x11/mod.rs:2592,2605`; `wayland/mod.rs:3094,3294`; `macos/mod.rs:2403,2440` | `current_window_state.window_focused` |
| H7 | **DPI change** (zombie) | `x11/mod.rs:2729`; `wayland/events.rs:217`; `windows/mod.rs:2877`; `macos/events.rs:808` | `size.dpi` + relayout |
| H8 | **Window frame state from the WM** (min/max/fullscreen/restore) | `wayland/events.rs:1501`; `windows/mod.rs:2867`; `macos/mod.rs:2275–2337` | `flags.frame` |
| H9 | **Theme change** | `windows/mod.rs:4406` | `current_window_state.theme` |
| H10 | **Pen: eraser / barrel button / rotation / tangential / proximity** | `wayland/events.rs:800,813,841`; `android/mod.rs:597`; `ios/mod.rs:416` | `update_pen_state_full` (11 params, `gesture.rs:915`) |
| H11 | **Scroll source** (wheel vs touchpad/pixel) | `wayland/mod.rs:2962` vs `3035` | `ScrollInputSource` (`scroll_state.rs:537`) |
| H12 | **Window move** (zombie) | `x11/mod.rs:2119`; `windows/mod.rs:3054`; `macos/mod.rs:2483` | `current_window_state.position` |
| H13 | **Monitor change / monitor list** | `windows/mod.rs:3042`; `macos/mod.rs:2513,3055` | `monitor_id`, `lw.monitors` |
| H14 | **Mouse leave** (`CursorPosition::OutOfWindow`) | `x11/events.rs:672`; `wayland/mod.rs:3081` | `mouse_state.cursor_position` |
| H15 | Gamepad / sensor injection | `common/capability_pump.rs:63,72` | `push_gamepad_state` |

### 2.C H1 — THE TEXTURE-CALLBACK DAMAGE CONTRACT IS BROKEN, AND INVISIBLE

This is the finding to act on. Per the refinement: we do not care what the callback draws;
we care that the engine's invalidation bookkeeping responds. It does not.

**What the path is.** A texture node is `NodeType::Image(ImageRef)` whose data is
`DecodedImage::Callback(CoreImageCallback)` (`core/src/resources.rs:844`).
`LayoutWindow::invoke_cpu_image_callbacks` (`layout/src/window.rs:1545`) walks the layout,
finds every callback-image node, invokes its `RenderImageCallback` with the laid-out
`HidpiAdjustedBounds` and the GL context (which is `None` in CPU mode — the doc comment at
`window.rs:1538–1540` says the callback is expected to take its CPU branch), and stores the
produced `ImageRef` keyed by the **original** image's hash into
`cpu_image_callback_results` (`window.rs:1620`). `CpuRenderState` then looks it up
(`window.rs:1541`, `layout/src/cpurender/raster.rs:789`).

**Finding 1 — headless never invokes them. LOUDLY.**
`invoke_cpu_image_callbacks` is called by every real shell:
`x11/mod.rs:3583`, `windows/mod.rs:885`, `wayland/mod.rs:4465`, `macos/mod.rs:6114`.
It is **never called in `headless/mod.rs`** (grep: zero hits). Headless only *reads* the map —
`headless/mod.rs:767`:
```rust
.with_image_callback_results(layout_window.cpu_image_callback_results.clone());
```
— and that map is initialised empty (`layout/src/window.rs:598`) and never written on this
path. **Every `RenderImageCallback` node in headless renders with no callback result, and the
callback is never invoked at all.** The entire callback-image family — canvases, camera/capture
tiles, AzPaint's `render_canvas`, video surfaces — is *structurally invisible* to the headless
suite today. Not "renders wrong pixels": *never runs*.

**Finding 2 — the invalidation is a no-op, on every platform.**
The way an app says "my texture changed" is `CallbackInfo::update_image_callback`
(`layout/src/callbacks.rs:1115`) → `CallbackChange::UpdateImageCallback { dom_id, node_id }`
(`callbacks.rs:226`). Its handler, in full (`dll/src/desktop/shell2/common/event.rs:1656–1658`):
```rust
CallbackChange::UpdateImageCallback { dom_id: _, node_id: _ } => {
    ProcessEventResult::ShouldReRenderCurrentWindow
}
```
It ignores the node. It does not mark it dirty. It does not regenerate the display list.
(`UpdateAllImageCallbacks`, `event.rs:1660`, is identically empty.)

Now follow the damage. `CpuBackend::render_frame` derives damage **from a display-list diff**
(`headless/mod.rs:456` `dl_damage`), and the first arm of the dispatch
(`headless/mod.rs:569–586`) is:

> `// Nothing changed — skip rendering entirely.` → `self.last_present_damage = FrameDamage::None; return Vec::new();`

A texture redraw does not change the display list — the `Image` item still carries the same
`ImageRefHash` (that is the whole design, `window.rs:1613–1615`). So the diff is empty, the
frame is **skipped**, and `FrameDamage::None` is reported. The contract
*"trigger a texture redraw ⇒ the engine emits an update whose damage rect is that node's rect"*
**fails today, returning no damage at all**.

This is not a headless artefact — all six shells present through the same
`CpuBackend::render_frame` and blit `last_present_damage` (x11 `3653`, wayland `4520`,
windows `933`, macos `6142`). The sibling `CallbackChange::ChangeNodeImage`
(`event.rs:1636–1653`) had exactly this bug and was fixed by explicitly calling
`lw.regenerate_display_list_for_dom(*dom_id)`, with a comment that spells out the failure mode:

> *"Without this the stored display list still carries the OLD image item: the CPU diff sees
> 'nothing changed' and skips … a per-frame ChangeNodeImage (camera/capture tile) stays frozen
> on its placeholder forever."*

`UpdateImageCallback` never got that fix. So: **a live canvas that redraws itself via
`update_image_callback()` is frozen on all platforms.** I could not find any compensating
path (no `invalidate_image` / `MarkImageDirty` / texture txn — grep for
`InvalidateImage|invalidate_image|RequestTextureRedraw|MarkImageDirty` returns nothing).
Caveat: I audited the CPU/damage path; if WebRender has a separate GPU texture-update
transaction I did not find it, and the `ChangeNodeImage` comment above ("the GPU image-only
txn re-sends the old scene") suggests it does not.

**What must be added** (the testable contract, no GL required):
1. **Engine fix (not an op):** `UpdateImageCallback` must re-invoke that node's callback and
   damage the node's rect — mirroring `ChangeNodeImage`'s `regenerate_display_list_for_dom`,
   or (better) an explicit damage push of the node's `used_size` rect.
2. **Headless must call `invoke_cpu_image_callbacks(&self.common.gl_context_ptr)` before
   `render_frame`** — a one-liner mirroring `x11/mod.rs:3583`. Without this, nothing below is
   observable.
3. **Stub producer:** a built-in `RenderImageCallback` that fills a solid colour from a
   counter and increments an invocation count. `cpu_image_callback_results` is exactly the
   right seam — it is CPU-only by construction and already threaded into the rasteriser
   (`raster.rs:789`), so it stands in for a GL texture with no GL anywhere.
4. **Ops:**
   - `mount_image_callback { node_id | selector, color?, width?, height? }` — replace a node's
     `NodeType` with an `Image(DecodedImage::Callback(stub))`.
   - `trigger_image_callback { node_id | selector }` — push
     `CallbackChange::UpdateImageCallback`; returns the resulting `FrameDamage`.
   - `get_image_callback_invocations { node_id }` → `{ count }` — asserts the callback was
     actually re-run.
   - `get_frame_damage` → `FrameDamage` (see §3) — the assertion target:
     `damage == [node rect]`.

Same lens applied elsewhere: for any capability we cannot *render* headlessly, the question is
whether its **invalidation bookkeeping** is reachable. For textures it is (via
`UpdateImageCallback`) — so it is testable and belongs in the op surface. Out of scope,
explicitly, and not reported as holes: GL/GPU pixel correctness, `TakeNativeScreenshot`
(`full.rs:1845`; headless `get_raw_window_handle()` returns `RawWindowHandle::Unsupported`,
`headless/mod.rs:1717`), and `get_gl_context` returning a real context
(headless `gl_context_ptr: OptionGlContextPtr::None`, `headless/mod.rs:925`).

### 2.D POLLUTION — ops that are not on the window boundary

Not one of these corresponds to anything a platform shell does. They are an IDE/codegen
service that happens to share a socket with the window protocol.

**Component / IDE family (24 ops)** — `full.rs:1932–2110`:
`ResolveFunctionPointers` (1929), `GetComponentRegistry` (1935), `GetLibraries` (1937),
`GetLibraryComponents` (1939), `ExportCode` (1945), `ExportCodeZip` (1950),
`ImportComponentLibrary` (1959), `ExportComponentLibrary` (1965), `CreateLibrary` (1971),
`DeleteLibrary` (1978), `CreateComponent` (1983), `DeleteComponent` (1991),
`UpdateComponent` (1998), `GetComponentPreview` (2020), `GetComponentRenderTree` (2054),
`GetComponentSource` (2061), `UpdateComponentRenderFn` (2072), `UpdateComponentCompileFn` (2081),
`OpenFile` (2093 — literally shells out to `xdg-open`/`cmd /C start`, `full.rs:10190–10210`).

**Harness/service leakage:** `GetLogs` (`full.rs:1700`), `RunE2eTests` (`full.rs:1872` — the
protocol containing its own test runner is a layering inversion; the *client* should be the
runner).

**Internal pokes no shell calls:** `Relayout` (`full.rs:1834`), `Redraw` (`full.rs:1835`).
No platform shell ever asks the engine to relayout out of band — relayout is a *consequence*
of an event. Keeping these lets a test paper over a missing invalidation (exactly the H1 bug:
if a test pokes `redraw` it will never notice that `update_image_callback` produces no damage).

**Borderline — keep, but they are queries, not window I/O:** the DOM-mutation family
(`InsertNode` 1882, `DeleteNode` 1897, `SetNodeText` 1902, `SetNodeClasses` 1909,
`SetNodeCssOverride` 1919) and `SetAppState` (1851). No shell injects these. They are
legitimate *harness scaffolding* (you must be able to put the app in a state), so they belong
in the HARNESS-CONTROL group (§4), not in INPUT — but they should be labelled as such, because
today they sit in the same flat enum as `MouseDown` and that is what makes the surface feel
unbounded.

---

## 3. THE OUTPUT BOUNDARY

What a shell actually reads out of the engine in order to present and drive the window:

| # | Output | Where the shell reads it | Reachable from the op API today? |
|---|---|---|---|
| O1 | **Pixels** (`CpuBackend::last_frame`) | all six presenters | **Yes** — `TakeScreenshot` (`full.rs:1844`, impl `6690`) |
| O2 | **Damage rects** (`last_present_damage: FrameDamage`, `headless/mod.rs:287`; enum `headless/mod.rs:159–166`) | x11 `3653`, wayland `4520`, windows `933`, macos `6142`, android `227`, ios `1216` | **NO — hole in HEAD.** No op reads it. (`FrameReport` / `capture_damage_png` from commit `3a0350ac1` are **not** in HEAD — verified not an ancestor; they exist only on the two agent worktree branches.) |
| O3 | **Cursor icon** the engine wants | `x11/events.rs:644–647`, `windows/mod.rs:3145–3147`, `macos/events.rs:334–338` (`set_cursor` from `mouse_state.mouse_cursor_type`) | **NO.** `GetCursorState` (`full.rs:1865`) is the *text caret* (position + blink), not the OS cursor icon. Hover-cursor behaviour (`cursor: pointer` on a button) is untestable. |
| O4 | **IME caret rect** (`ime_position`) | `windows/mod.rs:1728` + `sync_ime_position_to_os` (`3904`, `4074`), `x11/mod.rs:3398`, `wayland/mod.rs:3628`, `macos/mod.rs:4259` | **NO.** |
| O5 | **Clipboard writes** (what the app copied) | `sync_clipboard` reads `clipboard_manager.get_copy_content()` — `x11/clipboard.rs:46–56`, `wayland/clipboard.rs:48` | **NO.** Copy is untestable. |
| O6 | **Window-state requests from the engine** (title, frame, size/min/max, always-on-top, resizable, visibility, close) | `windows/mod.rs:1806` (`SetWindowTextW`), `1865–1945` (frame), `1748–1778`; `wayland/mod.rs:3636–3737`; `macos/mod.rs:4426–4460` | **NO.** `GetState` (`full.rs:1694`, impl `6148`) returns a `WindowStateSnapshot` with size/dpi/focus/focused-node only — not title, not frame, not the requested flags. |
| O7 | **Relayout/redraw requirement** (`ProcessEventResult`) | every shell branches on it | **NO** — not exposed. This is the cheapest possible regression signal ("did this event cause a relayout?") and it is not readable. |
| O8 | **Monitor list** (`lw.monitors`) | `windows/mod.rs:485`, `macos/mod.rs:2513` | **NO.** |
| O9 | DOM/layout/scroll/selection/focus introspection | not read by shells (debug-only) | Yes — the `Get*` family. Legitimate *assertion* surface, not part of the presenter boundary. |

**What `3a0350ac1` covers when it lands:** `FrameDamage` is queryable (`FrameReport`) and damage
regions can be returned as PNG (`capture_damage_png`) — i.e. **O2 and the patch half of the
protocol**. It does **not** touch O3–O8. So even after it merges, a client can reassemble the
*screen* but cannot obtain the *cursor*, the *title*, the *IME rect*, the clipboard, or any
window-state request — a remote-display client would show pixels with a permanently-wrong
mouse cursor and no window title.

---

## 4. THE MINIMAL COMPLETE OP SET (= the boundary)

### A. INPUT ops — the mock-event surface (one per real injection channel)

Existing and correct: `mouse_move`, `mouse_down`, `mouse_up`, `key_down`, `key_up`,
`text_input`, `touch_start`, `touch_move`, `touch_end`, `touch_cancel`, `resize`, `close`,
`swipe`, `pinch`, `rotate`, `long_press`. (`click`, `double_click`, `click_node` are sugar over
`mouse_*` — keep, they're cheap.)

To ADD or FIX:

| Op | Params | Why |
|---|---|---|
| `focus` / `blur` | — | **implement the existing zombies** (§2.A) |
| `move` | `x: i32, y: i32` | implement the zombie |
| `dpi_changed` | `dpi: u32` | implement the zombie |
| `mouse_leave` | — | `CursorPosition::OutOfWindow` (H14) |
| `scroll` | **add** `source: "wheel" \| "touchpad"` | H11, `ScrollInputSource` |
| `pen_down` / `pen_move` / `pen_up` | **add** `is_eraser, barrel_button, tangential_pressure, barrel_roll_rad, device_id, tool_id`; `pen_up` should take `pressure`/`in_contact` | H10, `gesture.rs:915` |
| `pen_proximity` | `in: bool, x, y` | H10 (`wayland/events.rs:813`) |
| `ime_set_preedit` | `text: String, cursor_begin: usize, cursor_end: usize` | H3 → `text_edit_manager.set_preedit` |
| `ime_commit` | `text: String` | H3 |
| `file_hover` | `x, y, paths: Vec<String>` | H4 — port `HeadlessEvent::FileHover` |
| `file_drop` | `x, y, paths: Vec<String>` | H4 |
| `file_hover_cancel` | — | H4 |
| `set_clipboard` | `text: String` | H5 — the *source* a paste reads |
| `window_frame_changed` | `frame: "normal"\|"minimized"\|"maximized"\|"fullscreen"` | H8 |
| `theme_changed` | `theme: "light"\|"dark"` | H9 |
| `monitor_changed` | `monitor_id: u32, scale: f32, size: [f32;2]` | H13 |
| `gamepad_state` / `sensor_state` | device payload | H15 (optional) |
| `trigger_image_callback` | `node_id \| selector` | **H1** — the texture-redraw trigger |

### B. OUTPUT / QUERY ops — the presenter surface

| Op | Returns | Status |
|---|---|---|
| `get_frame_damage` | `FrameDamage` = `none \| full \| rects[]` | **ADD** (O2; `FrameReport` in `3a0350ac1` covers this) |
| `capture_damage_png` | per-rect PNG patches + rects | **ADD** (in `3a0350ac1`) |
| `take_screenshot` | full-frame PNG | exists (`full.rs:1844`) |
| `get_cursor_icon` | `MouseCursorType` | **ADD** (O3) |
| `get_ime_caret_rect` | `ImePosition` | **ADD** (O4) |
| `get_clipboard` | what the app copied (`get_copy_content`) | **ADD** (O5) |
| `get_window_requests` | title, frame, size/min/max, flags, close_requested | **ADD** (O6) — extend `GetState` |
| `get_last_process_result` | `ProcessEventResult` | **ADD** (O7) — the cheapest invalidation assertion |
| `get_monitors` | `lw.monitors` | **ADD** (O8) |
| `get_image_callback_invocations` | `{ node_id, count }` | **ADD** (H1) |
| `get_pending_timers` / `is_idle` | timer ids + next deadline; settled? | **ADD** (H2) |
| assertion/introspection `get_*` family | as today | keep |

### C. HARNESS-CONTROL ops — legitimate scaffolding, explicitly *not* the boundary

`mount` (in `3a0350ac1`), `tick_ms { ms }` (**virtual clock — H2, the single highest-leverage
addition after H1**), `snapshot` / `restore_snapshot`, `set_app_state`, `get_app_state`,
`mount_image_callback` (the H1 stub), the DOM-mutation family, `wait_frame` (**must actually
wait — it is a no-op today**, `full.rs:6681`).
`wait { ms }` should be **deleted** in favour of `tick_ms` (it currently `thread::sleep`s the UI
thread, `full.rs:6686`).

### D. DELETE from the protocol (move to a separate `/ide` endpoint or a separate socket)

The 24 component/IDE ops, `ExportCode*`, `ImportComponentLibrary`, `ResolveFunctionPointers`,
`OpenFile`, `GetLogs`, `RunE2eTests`, and the internal pokes `Redraw` / `Relayout`.
These are a fine product — they are just not the window's I/O boundary, and mixing them into it
is precisely what destroyed the completeness criterion.

---

## 5. SANITY-CHECK: does "run headless, curl the ops, reassemble from patches" actually work?

**Yes, the core is sound.** Nothing below the boundary is platform-bound:

- Rendering is pure software. `CpuBackend::render_frame` (`headless/mod.rs:358`) rasterises into
  a pixmap with `cpurender` — no GL, no WebRender required. All six shells present through the
  *same* function and blit `last_present_damage` (x11 `3653`, wayland `4520`, windows `933`,
  macos `6142`, android `227`, ios `1216`). The presenters really are thin.
- Hit-testing has a CPU path (`common/event.rs:988` `get_cpu_hit_tester`,
  `azul_layout::headless::CpuHitTester`), so headless is not secretly GPU-bound.
- Damage already exists as a first-class value (`FrameDamage`, `headless/mod.rs:159–166`, with
  `to_physical_rects` documented as *"the ONE conversion every platform presenter should use"*),
  and the enum already distinguishes `None` / `Rects` / `Full` — which is exactly the
  vocabulary a remote-display protocol needs.

**But there are five structural things in the way. Be honest about them:**

1. **`RawWindowHandle::Unsupported` (`headless/mod.rs:1717`)** forecloses, in headless:
   native screenshots, native menus/dialogs/tooltips, OS-level IME contexts, real GL context
   creation, and OS drag-and-drop handoff. Any user callback that reaches for the raw handle
   gets `Unsupported` and must have a fallback. For the *display protocol* this is mostly fine
   (we synthesize the input and composite the output ourselves) — but it means a headless
   server cannot support native context menus, and the `use_native_context_menus` flag
   (`x11/mod.rs:4533`, `macos/mod.rs:2891`) becomes a fork the client cannot see.

2. **The GL/texture callback family currently does not run at all in headless** (§2.C).
   Until `invoke_cpu_image_callbacks` is called from the headless loop, any app with a canvas
   is a blank rectangle on the wire. This is a two-line fix but it is load-bearing for the
   thesis: an "engine decoupled from the platform" that silently drops the entire
   callback-image node type is not decoupled.

3. **Blocking / non-deterministic OS calls remain on the shared path.**
   `get_system_clipboard()` (`common/event.rs:261–281`) hits the *real* X11 clipboard on Linux
   even when headless (it is `#[cfg(target_os = "linux")]`, not `#[cfg(feature)]`), so a paste
   in a headless server on a machine with no X display goes down the `x11_clipboard` init path
   and degrades to `None` — nondeterministic, unassertable, and a per-op syscall. Same shape for
   `capability_pump` polling real gamepads/sensors (`capability_pump.rs:63,72`).
   These need a headless stub, injected via ops.

4. **The op transport is a 16 ms polling timer inside the window** (`create_debug_timer`,
   `full.rs:~10248`; interval `from_millis(16)`), with ops applied through a `CallbackInfo`.
   That gives you ≤1 op per 16 ms and, for `Wait { ms }`, a `thread::sleep` **on the UI thread**
   (`full.rs:6686`). It works for a test harness; as a display server it caps interactive
   throughput at ~60 ops/s and lets one client stall the whole engine. If this becomes a real
   protocol, ops need to be drained in a loop per tick (and `wait` deleted).

5. **No virtual clock.** Everything time-driven (cursor blink, scroll momentum, long-press,
   animations) reads the real `Instant` (`headless/mod.rs:1521`). Deterministic replay — the
   thing that makes "the tests are a side effect" actually pay off — requires `tick_ms`. It
   exists in `3a0350ac1`; it is not in HEAD.

**Bottom line:** the decoupling is real and the architecture is right. What is not true today is
the completeness claim: the op surface is roughly *half* the input boundary, *one* of the nine
output channels, plus about thirty ops that do not belong to a window at all. And the highest-value
gap is not an input event — it is that the engine's own texture invalidation produces **no damage**,
which means the protocol's core promise (damage patches) is already lying for an entire node type,
on every platform.

### Recommended order
1. **Fix `UpdateImageCallback` to damage the node's rect**, and call
   `invoke_cpu_image_callbacks` from the headless loop. (Real product bug: live canvases are
   frozen on all platforms.)
2. Land `3a0350ac1` (`FrameReport`, `capture_damage_png`, `tick_ms`, `mount`) — O2 + H2.
3. Implement the five zombie ops (`focus`, `blur`, `move`, `dpi_changed`, `get_dom`) and make
   `wait_frame` actually wait. Free, and they are currently returning green on nothing.
4. Add H3–H5 (IME preedit, file drop, clipboard) — three real input channels with zero coverage.
5. Add the output ops O3–O7 (cursor icon, IME rect, clipboard readback, window requests,
   `ProcessEventResult`).
6. Split the IDE/component family off the window protocol.

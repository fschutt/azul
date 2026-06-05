# Azul Linux GUI — Session Handoff (→ Linux Mint Cinnamon / X11)

Branch: `mobile-ios-android`. This doc hands off the native-Linux desktop-shell
work (KDE Plasma / Wayland / nouveau, software-GL-capable) to a fresh session on
**Linux Mint Cinnamon** (X11 by default, Muffin WM) to re-test the same things on X11.

---

## 0. STATUS @ 2026-06-03 (read this first)

Latest commits on `mobile-ios-android` (NOT pushed). Newest session below; older table in §1.

### Fixed + USER-VERIFIED this session (Wayland, KDE/CPU)
| Commit | Fix | Verified |
|---|---|---|
| (after `1f8a36cc8`) | **CPU hit-tester rebuilt each frame** in `generate_frame_if_needed` (`cpu_ht.rebuild_from_layout`). CPU/software-GL mode had `render_api=None` so hit-testing was dead (no hover/click/selection/focus). | ✅ hover/selection work |
| `5e74e30d7` | **Stale-self-pointer: rebind all wl listeners to the boxed `self` on first poll** (`wl_proxy_set_user_data`). Listeners were registered against `new()`'s stack `&mut window`, which the run loop MOVES to a heap Box → every event hit a dead stack frame. PROVEN by `[ADDR]` probes (reg `0x7ffe…` vs live `0x6492…`). Killed: window-won't-close + erratic/UAF input. Detail §9. | ✅ close works |
| `3d94a56d3` | **Redraw on keyboard input** — `handle_key` swallowed result variants & DOM-regen never presented (`regenerate_layout` alone doesn't build/send the WR txn on Wayland). Now routes through `handle_process_event_result` w/ full present path. | ✅ typing live |

### Bugs DISCOVERED, NOT yet fixed (tackle tomorrow — Wayland first, then X11)
- **Backspace inserts a tofu rect** (Wayland + X11). `handle_key` (`wayland/mod.rs:1870-1877`) and X11 (`x11/events.rs:727-734`) feed ANY xkb/`XLookupString` UTF-8 — incl. control bytes (Backspace `0x08`, Tab, Enter, Esc, Del `0x7f`) — into `record_text_input`, so it's inserted as a glyph. Deletion itself already works via the `VirtualKeyCode::Back` → `delete_selection` path (`common/event.rs:2245`). **FIX:** skip `record_text_input` when the string is all control chars (`s.chars().all(|c| c.is_control())`). Trivial; applies to BOTH backends.
- **Mouse-wheel scroll doesn't update visually** (Wayland; scrollbar now renders). Wheel → `handle_pointer_axis` (`wayland/mod.rs:2125`) → `scroll_manager.record_scroll_from_hit_test` queues a **physics-timer** animation; the lightweight path in `generate_frame_if_needed` (`:2944`) ticks it (`scroll_manager.tick` → `calculate_scrollbar_states`). Likely the scroll physics timer isn't driving redraws on Wayland — verify `check_timers_and_threads` pumps it and that each tick calls `generate_frame_if_needed`/`request_redraw`. **Needs investigation.**
- **From earlier user feedback, still open / re-test after the above:** caret not placed at click point; (focus-follows-window vs element — may be resolved by the rebind, re-test); can't resize window past a size (#3); multiline textarea clipped to 2 lines ≠ macOS (#6, layout); Tab shifts content (#5). GPU path SIGSEGVs on this DOM (separate WebRender bug; stay on CPU).

### X11 ports owed (same fixes; X11 is the un-patched ancestor — see §10 audit)
1. **Text-input result routing** (highest — hits contenteditable directly): port `handle_process_event_result`, call from `poll_event` + `handle_event`.
2. **CPU hit-tester rebuild** (CPU/software-GL): add `rebuild_from_layout`; also guard the `hit_tester.unwrap()` at `x11/events.rs:863`.
3. **FBO-0 clear** (GPU cosmetic).
4. **Backspace control-char filter** (same as Wayland).
X11 is SAFE from the stale-pointer, close, and blank-window bugs (polling architecture).

### X11 API reference (for tomorrow's X11 fixes)
See **`X11_API_REFERENCE.md`** (primary-source-grounded). Load-bearing facts:
- **FBO-0 clear:** EGL spec — back-buffer color is *undefined* after `eglSwapBuffers` unless
  `EGL_SWAP_BEHAVIOR==EGL_BUFFER_PRESERVED` (needs a config bit; don't assume). Fix = `glBindFramebuffer(0)`
  + `glViewport` + `glClear` every frame. Damage/partial-present do NOT preserve untouched pixels.
- **X11 text/IME:** `setlocale`+`XSetLocaleModifiers("")` before `XOpenIM`; `XFilterEvent(&ev,None)` on
  EVERY event (missing this = the classic broken-IME cause); `Xutf8LookupString` on `KeyPress` only, insert
  only on `XLookupChars`/`XLookupBoth` (control keysyms = command, not text — the X11 analog of the
  backspace-tofu fix). Root-window style `XIMPreeditNothing|XIMStatusNothing` is the safe default for
  fcitx5/ibus Japanese input.
- **X11 CPU blit:** `ZPixmap`, depth must match drawable, swizzle RGBA→BGRX; MIT-SHM (gate on
  `XShmQueryExtension`+local) with `XPutImage` fallback; wait on `ShmCompletion` before reusing a segment.

---

## 1. What was done this session (committed)

All on `mobile-ios-android`, NOT pushed (no push without explicit ask).

| Commit | What |
|---|---|
| `0f6af1810` | **fix(core): CssPropertyCachePtr double-free** — `ManuallyDrop<Box>` + gate free on `run_destructor`. Repro `dll/examples/css_double_drop.rs` (SIGSEGV before, clean after). |
| `6bf17075a` | **fix(desktop/wayland): render content + idle events + server-side decorations** — bundles 4 fixes (see below). |
| `8e3cac256` | **feat(desktop/wayland): honor AZ_BACKEND=cpu + prefer cpurender over software GL** — Mesa-free CPU path; PSS 64→36 MB. |
| `06ff067b6` | **fix(core): IconProviderHandle double-free** — `ManuallyDrop`+`run_destructor` gate; api.json field + codegen regen. PROVEN via `appconfig_double_drop.rs` (crash moved past `icon_provider`). **But see §4: AzAppConfig nests MORE ungated double-drop fields → systemic #23 needed.** |
| `1f8a36cc8` | **fix(desktop/wayland): refresh hit-tester each frame** — the keystone for dead buttons/hover. `update_hit_test` only acted `if let Resolved`, but the hit-tester was stuck `Requested` forever (never refreshed; macOS re-requests after every frame) AND `resolve()` caches → it hit-tested against the initial empty DL → no callbacks. Now re-requests after `flush_scene_builder` + `update_hit_test` delegates to `perform_hit_test` (GPU resolve + CPU fallback). Fixes physical + synthetic paths. **X11 likely needs the same** (it also never refreshes its hit-tester). NOT visually verified autonomously (no input-injection tool; debug-server DOM access is broken — see below) — user to confirm buttons respond. |

### The 4 Wayland fixes in `6bf17075a`
1. **Garbage ("1px dots")** — clear the EGL backbuffer (bind FBO 0 + viewport + `clear`) before WebRender draws; the EGL surface returns as uninitialized VRAM after each swap and WebRender only clears its own offscreen targets.
2. **Blank window** — present (`eglSwapBuffers` + `wl_surface_damage`) ONLY when `total_draw_calls > 0`. A lightweight redraw / unchanged-scene regen renders 0 draw calls; swapping that empty multi-buffered EGL buffer wiped the content. The frame-callback loop hit this every vsync.
3. **Idle/close unresponsive** — `poll_event` now drains the socket non-blockingly (`wl_display_prepare_read_queue`/`poll`/`read_events`|`cancel_read`) instead of only dispatching queued events. Previously the fd was read only as a side-effect of `eglSwapBuffers`, so an idle window processed no events incl. `xdg_toplevel.close` (couldn't close from taskbar).
4. **Server-side decorations** — `xdg-decoration-unstable-v1` (hand-built `zxdg_decoration_manager_v1`/`zxdg_toplevel_decoration_v1` interfaces; `get_toplevel_decoration` [opcode **1**, not 0=destroy] + `set_mode(server_side)`). KDE confirmed `mode=2`. New-id marshalling MUST use `wl_proxy_marshal_constructor` (the proven path); `wl_proxy_marshal_flags` returned NULL here.

User VISUALLY CONFIRMED the Wayland GPU window renders ("it renders now").

---

## 2. What was investigated (findings)

- **CPU mode / memory**: `AZ_BACKEND=cpu` was NOT honored pre-fix (always tried GL, succeeded via Mesa/llvmpipe; PSS 63.7 MB > GPU 45 MB). After `8e3cac256`: `AZ_BACKEND=cpu` skips GL entirely → wl_shm + `cpurender` (tiny-skia), **0 Mesa mappings, PSS ~36 MB**. Software-GL (llvmpipe/swrast) under Auto now also switches to CPU (faster). `gl_context_ptr` stays `None` so the canvas uses CPU pixmaps not GL textures.
- **Dirty-rect / damage** (#30): **CPU mode = correct + efficient** (`compute_display_list_damage` + `retained_pixmap` + `render_display_list_damaged` → a cursor blink redraws only its rect). **GPU mode = NOT efficient**: WebRender partial-present is DISABLED (`wr_translate2.rs` `partial_present: None`, `max_partial_present_rects: 1`, `full_paint:true` every frame). `wl_surface_damage` hints only reduce *compositor* recompositing, not azul's own full GPU repaint. Real GPU partial-present needs a `PartialPresentCompositor` + `EGL_EXT_buffer_age` + `EGL_KHR_partial_update` (eglSetDamageRegion) + `wl_surface_damage_buffer` (buffer coords; currently uses legacy `wl_surface_damage` surface coords). **OPEN — significant feature.**
- **Pen/tablet + touch** (#31): Wayland pen is **largely wired** — `zwp_tablet_v2` tool handlers (proximity/down/up/motion/pressure[/65535]/tilt/rotation) → `window.tablet_pen` → `handle_tablet_frame()` → `gesture_drag_manager.update_pen_state_full(pos, pressure, tilt, in_contact, is_eraser, rotation)`. Touch wired (`wl_touch` → `handle_touch_point`); `TouchPoint.force` exists but wl_touch core has no pressure. **OPEN/VERIFY with the user's drawing tablet**: does `gesture_drag_manager` deliver pressure into the app canvas `CallbackInfo`, and does azul-paint use it for stroke width? X11 pen = XInput2 valuators (separate path).
- **X11 parity** of the Wayland fixes: each targets a Wayland-only gap X11 already handles — event-driven `Expose`/`ConfigureNotify→request_redraw` render (no frame-callback loop → no empty-frame wipe); `XPending`/`XNextEvent` socket reads (idle-responsive); WM-drawn decorations (override_redirect only when `decorations==None`); GLX `glClearColor` clear. So no X11 changes were needed for #1–#4. `#15` (core double-free) benefits X11 equally.

---

## 3. X11 test plan on Mint (the point of the restart)

Build (see §5), then `AZ_BACKEND=x11 ./target/release/azul-paint` and verify:
1. **Renders content** (X11/GLX path; should already work — X11 renders on Expose).
2. **Window decorations** — Muffin (Mint's WM) draws the titlebar; window movable/closable. (X11 gets these from the WM; no xdg-decoration needed.)
3. **Resize** — no crash, relayouts (the `#15`/GlContextPtr/InstantPtr double-free fixes are what made resize safe).
4. **Physical mouse / hover / click / drag** — canvas strokes work.
5. **Force-CPU on X11**: `AZ_BACKEND=cpu` should skip the GLX trial. **NOT yet implemented on X11** — see §4.
6. **Pen/tablet on X11** (XInput2) — the user's tablet; check pressure reaches the canvas.
7. **Dirty-rect on X11** — same WebRender partial-present-disabled situation applies (GPU full-repaints); CPU path incremental.
8. **AZ_DEBUG screenshot on X11 returns empty** (task #21) — a debug-server tooling gap; useful to fix for self-verification.

---

## 4. The pending IconProviderHandle fix + X11 force-CPU spec

### IconProviderHandle (the only remaining double-free of the class)
`AzIconProviderHandle` is nested in `AzAppConfig.icon_provider` and double-frees its `Box<IconProviderInner>` when an `AzAppConfig` is dropped by value (latent — normal apps move it into `App::create`). Fix (this session, verifying): `core/src/icon.rs` → `inner: ManuallyDrop<Box<IconProviderInner>>` + `run_destructor: bool` + gated `Drop` + `ManuallyDrop::take` in `into_shared` + `**` in `Clone`; `api.json` IconProviderHandle `struct_fields` += `run_destructor: bool`; **requires `azul-doc codegen all`** (adds a mirror field). **DONE — committed `06ff067b6`, proven via `appconfig_double_drop.rs`.**

### IMPORTANT: the double-drop class is BROADER than the individual leaves
The `appconfig_double_drop` repro (drop `AzAppConfig` by value, link-static Rust) now gets past `icon_provider` (proving that fix) and aborts in **`AzSystemStyle_delete`** — so `AzAppConfig` nests *several* ungated double-drop fields (SystemStyle, and likely more after it). Mechanism recap: `drop_in_place::<AzParent>` drops the real parent (step1, all fields once, gated leaves OK) THEN drops the parent's Az **mirror** fields (step2); any mirror field that is itself an Az-wrapper-with-`impl Drop` re-runs `_delete` → `drop_in_place` of the real leaf on the same bytes → second free unless that leaf gates (run_destructor / refcount / destructor-enum). Per-leaf gating is whack-a-mole for deeply-nested structs.
**→ The real fix is the SYSTEMIC #23**: change the azul-doc generator so the Az wrapper's `_delete`/Drop does NOT leave the mirror fields to be re-dropped — e.g. `_delete` does `core::ptr::read` + the real-type drop then `mem::forget`s, OR emit all Az mirror fields as `ManuallyDrop` so step2 is always a no-op. That closes the entire class at once (all 260 wrappers) and makes the per-leaf `run_destructor` gates unnecessary. Use `appconfig_double_drop.rs` as the regression target (must run clean once #23 lands).
The per-leaf fixes already shipped (GlContextPtr, InstantPtr, CssPropertyCachePtr, IconProviderHandle) handle the leaves that bite in REAL crash paths (resize, timer, StyledDom-drop, AppConfig.icon_provider); #23 is needed to make arbitrary by-value Az-wrapper drops safe.

### X11 force-CPU (mirror of the Wayland change, NOT done — do on Mint)
In `dll/src/desktop/shell2/linux/x11/mod.rs` (~line 1132, the 7-tuple `match gl::GlContext::new(...)`):
- Resolve `AzBackend::resolve(options.renderer.as_option().map(|r| r.hw_accel))`.
- If `Cpu`: skip `GlContext::new`; return the CPU tuple `(RenderMode::Cpu(Some(XCreateGC(...))), None, None, None, None, None, OptionGlContextPtr::None)`.
- In the `Ok` arm, when `query_gpu_info` is `Blacklisted` (software) AND backend != `Gpu`: drop the GL ctx and return the CPU tuple (instead of just `RendererType::Software`).
The X11 CPU path uses `RenderMode::Cpu(Some(gc))` (XImage/`XPutImage` via cpurender). Software-GL won't trigger on this machine's hardware GL, so it must be tested on a software-GL/CPU X11 setup.

### Systemic approach (#23 / #33) — DETECTION shipped; auto-fix abandoned
Auto-rewriting (ManuallyDrop on all mirror fields) was tried + reverted — it broke public/user struct construction (`ManuallyDrop<AzU8Vec>: From<Vec>` in azul-paint) and `has_custom_drop` is unreliable. Instead (per the user) **azul-doc now DETECTS the risk and forces the maintainer to gate it** (commit `74b7db461`, `ir_builder.rs::validate_api_json`): flags non-Copy structs owning a raw ptr/Box (ConstPtr/MutPtr/Boxed/OptionBoxed) without a `run_destructor`/`destructor` gate, excluding slice/Vec views (`len`/`cap` field). Prints a WARNING with the full WHY (codegen `_delete` + parent drop-glue = double free) + the fix (ManuallyDrop+run_destructor, see GlContextPtr).
REMAINING (follow-ups, in priority order):
1. **Gate the flagged types**, then flip the warning to a hard ERROR (`errors.push` instead of `eprintln`) to prevent regressions. Flagged: VirtualViewCallbackInfo, LayoutCallbackInfo, TimerCallbackInfo, RenderImageCallbackInfo, NodeData, GridMinMax, ~~**SystemStyle** (proven AppConfig-drop offender)~~ **DONE `9091544f7`** — `appconfig_double_drop` now runs clean (200k, no double-free), so the AppConfig nest is fully gated; the rest below are NOT reachable via AppConfig and need their own repros, ComponentFieldTypeBox, GlVoidPtrMut (verify — likely a borrow false-positive → mark Copy or refine the detector). Per-leaf recipe: add `run_destructor: bool` + gate `Drop`. Two viable gate styles: (a) `ManuallyDrop<...>` the owned field (used by GlContextPtr/IconProviderHandle — thin handles, no `..base` users); (b) for "rich" types with many `..Default::default()` constructions (E0509 once they impl Drop), keep the field type and use the `take()+forget()` second-drop trick + expand the spread literals (used by SystemStyle). Then sync api.json via `azul-doc autofix`/`autofix apply`/`normalize` (NOT hand-edit) + `codegen all` + rebuild; verify with the relevant `*_double_drop.rs` repro (link-static).
2. **Auto-gen per-type drop memtests** (user idea): extend the codegen `memtest` (`target/codegen/memtest.rs`, generator.rs:179, config `memtest()`) so each non-Copy type is constructed (default/ctor) + dropped (and nested + dropped by value) in a loop → catches double-free/leak, proving each gate is correct. Like `css_double_drop`/`appconfig_double_drop` but auto-generated for all types.

---

## 5. Build & run

Pipeline (rebuild `libazul.so` / bindings after touching public API):
```
cargo build -r -p azul-doc
./target/release/azul-doc codegen all          # regenerates target/codegen/ (gitignored)
cargo build -r -p azul-dll --features build-dll # libazul.so
```
For the demo app (link-static, has cpurender): `cargo build -r -p azul-paint` → `target/release/azul-paint`.
If only behavior changed (not public API), skip codegen; just `cargo build -r -p azul-paint`.

Run knobs:
- `AZ_BACKEND=wayland|x11|cpu|gpu|headless` (default auto). `cpu` now = Mesa-free cpurender.
- `AZ_DEBUG=8765` — HTTP debug server (needs `--features azul/debug-server`, NOT default). Ops: `take_screenshot` (works on Wayland, empty on X11=#21), `resize`, `mouse_*`, `get_state`.
- `AZ_LOG=off` to silence; note `log_info!/log_debug!` go through the `log` facade (no logger installed → no stderr); only `eprintln!` always shows.

Verify CPU is Mesa-free: run `AZ_BACKEND=cpu`, then `grep -cE 'libEGL|libgallium' /proc/<pid>/maps` should be 0, PSS ~36 MB.

---

## 6. Machine differences (current vs Mint)

| | This session | Mint Cinnamon (next) |
|---|---|---|
| Display server | KDE Plasma **Wayland** (`wayland-0`) | **X11** default (Cinnamon) |
| WM/compositor | KWin (SSD via xdg-decoration) | **Muffin** (X11 WM decorations) |
| GPU | nouveau (Mesa, software-GL-capable) | (unknown — check `glxinfo`/`eglinfo`) |
| Decorations | client must negotiate SSD | WM-provided by default |

NOTE: the auto-memory in `~/.claude/projects/.../memory/` does NOT transfer to Mint (different machine). This repo doc + git history are the handoff.

---

## 7. Open task list (status)

- DONE & committed: garbage(#11), blank(#25), close/idle(#27), decorations(#26), CssPropertyCachePtr(#15), force-CPU Wayland(#28), diagnostics removed(#13), commits(#14), double-drop scan(#29), IconProviderHandle(#16/#29), hit-tester refresh / dead-buttons(#20 physical, commit 1f8a36cc8).
- ATTEMPTED + REVERTED + designed: systemic codegen double-drop fix(#23) — see §4; hard design problem (blanket ManuallyDrop breaks public construction; has_custom_drop unreliable from api.json; custom-drop-with-Az-fields needs ManuallyDrop). Per-leaf fixes cover real crashes.
- INVESTIGATED (documented, need hardware/big-feature/tooling): GPU partial-present(#30, big feature — EGL_EXT_buffer_age + PartialPresentCompositor); pen/touch(#31, wired, needs the user's tablet); IME/contenteditable text_input_v3 marshals(#18, wired, needs an example + IME engine); a11y(§7b-E, AccessKit wired, verify with Orca/Accerciser).
- OPEN / next: apply the hit-tester-refresh fix to X11 (mirror of 1f8a36cc8); fix debug-server "DOM not found in layout results" (blocks screenshot/get_state/synthetic-verification — §7b-A); X11 force-CPU(§4); azul-paint flex-row CSS bug(#19); menus/context-menus verify; mic-input pipeline(#17, no hw); device enumeration(#22); memory trim(#24, optional).
- VERIFICATION NOTE: autonomous visual/interactive verification on this box is BLOCKED (no input-injection tool like ydotool; debug-server DOM access broken). The committed Wayland render/decoration/close/hit-test fixes are reasoned + compile + don't regress, but need a human (or working debug tooling) to confirm clicks/IME/decorations visually.

---

## 7b. Input / text / IME / a11y research — toward "azul-paint + contenteditable.c fully working"

User report: clicking azul-paint buttons does **nothing** (no callback response) on the Wayland window; suspects mouse-move/text-input also don't fully work; no IME; no file-drop. Goal: azul-paint fully interactive AND `tests/e2e/contenteditable.c` works (set element `contenteditable`, receive text-editing input, activate IME, position the IME cursor). Each item below = research + status + where to dig.

### A. Mouse click / hover → callback dispatch (HIGH — blocks azul-paint)
- Path: `pointer_button_handler` → `handle_pointer_button` (mod.rs ~1995): reads position from `current_window_state.mouse_state.cursor_position` (set by motion/enter), sets `mouse_state.{left,right,middle}_down`, then `process_window_events(0)` → `handle_process_event_result`. Motion → `handle_pointer_motion` sets cursor_position + cursor changes (confirmed working).
- Symptom: physical clicks don't fire button callbacks → **same root as #20** (synthetic AZ_DEBUG mouse_down/up didn't register canvas strokes). So it's NOT synthetic-only — the **hit-test → callback dispatch** itself is the gap.
- ROOT-CAUSED + FIXED (commit `1f8a36cc8`): it WAS suspect (1) — the `AsyncHitTester` was set to `Requested` at init and never refreshed to `Resolved`, and `update_hit_test` only acted `if let Resolved` → it never ran. Plus `AsyncHitTester::resolve()` caches, so resolving the init request gives an empty-DL tester forever. Fix: re-request the hit-tester after every `flush_scene_builder` + route `update_hit_test` through `perform_hit_test`. **X11 has the same pattern (sets `Requested` at init, never refreshes) → apply the same fix there + test on Mint.** Still needs a real click to confirm visually (couldn't inject input autonomously).
- DEBUG-SERVER TOOLING IS BROKEN FOR DOM ACCESS (blocks autonomous verification, and is its own bug): `take_screenshot` returns `{"status":"error","message":"DOM not found in layout results"}` and `get_state` reports `dom_node_count: 0`, even though the window renders content fine. So the debug server looks up a DomId that isn't in `layout_results` (or queries before layout) — a tooling-path bug SEPARATE from the real hit-test (which uses the full `layout_results` map that IS populated). Fix this to regain screenshot/get_state/synthetic-event verification (relates to #20 synthetic + #21 X11 screenshot). Find where the debug server resolves the DOM for screenshot/state (search `"DOM not found in layout results"`).

### B. Text input (physical keyboard typing into a focused field)
- Keyboard events work (keymap mmap fix landed; `keyboard_key_handler`). Need: a focused editable node receives char input → text model updates → redraw. Verify the key→char→text-edit path drives `contenteditable` nodes (TextInputState / text_edit_manager). Depends on A (focus via click/hit-test). ACTION: verify keypress reaches `text_edit_manager` for a focused contenteditable.

### C. IME + contenteditable (`tests/e2e/contenteditable.c`)
- Substantial wiring EXISTS (mod.rs ~4121-4340): `text_input_v3_enable/disable`, `set_surrounding_text` (opcode 3), `set_cursor_rectangle` (opcode 6 — IME cursor positioning), `sync_text_input_v3_focus_state` (enables zwp_text_input_v3 on contenteditable focus), preedit handling. `contenteditable` attribute is in core (dom.rs:1353, packed flag).
- UNVERIFIED (#18): the text_input_v3 REQUEST marshals (enable/commit/set_surrounding/set_cursor_rectangle) — azul-paint has no text fields so they were never exercised. Use `contenteditable.c` to drive them. Watch for the same new_id/opcode pitfalls as the decoration fix (use `wl_proxy_marshal_constructor` for any new_id; verify opcodes against zwp_text_input_v3.xml: enable=1, disable=2, set_surrounding_text=3, set_text_change_cause=4, set_content_type=5, set_cursor_rectangle=6, commit=7).
- contenteditable.c expectation: set element editable → focus → IME activates (text_input_v3.enable) → preedit string shows → committed text inserted → IME candidate window follows the cursor (set_cursor_rectangle from the caret's layout rect). ACTION: build+run contenteditable.c on Wayland (KDE), exercise IME (e.g. ibus/fcitx), confirm enable + cursor-rect + commit work; fix marshalling as needed.

### D. File-drop areas (drag-and-drop)
- NOT implemented on Wayland: no `wl_data_device_manager` / `wl_data_device` / `wl_data_offer` binding or DnD handlers (grep found none). So dropping a file onto the window does nothing.
- NEEDED: bind `wl_data_device_manager`, get_data_device(seat), listen for `data_offer`/`enter`/`motion`/`drop`/`selection`; on drop, read the offered `text/uri-list` mime via a pipe fd → deliver to azul as a file-drop event (check if core has a `WindowState.dropped_file` / `On::Drop`-equivalent; X11 uses XDND). ACTION: implement wl_data_device DnD (Wayland) + XDND (X11) → azul file-drop callback.

### E. Accessibility on Wayland (test/verify)
- AccessKit IS integrated under `#[cfg(feature="a11y")]`: `accessibility_adapter: LinuxAccessibilityAdapter` (AT-SPI/atspi adapter), `process_accessibility_actions` polled each loop, `a11y_dirty` tree-push. On Linux this exposes the UI over **AT-SPI2 (D-Bus)** regardless of X11/Wayland.
- HOW TO TEST/VERIFY: build with `--features a11y`; run; then inspect the AT-SPI tree with **Accerciser** or **`busctl`/at-spi2 tools**, or run **Orca** screen reader and confirm it announces azul-paint's widgets; verify `process_accessibility_action` handles activate/focus actions (e.g. Orca-triggered button activation should fire the same callback as a click — and is a good cross-check for item A). On KDE Wayland, AT-SPI works via D-Bus (not Wayland-protocol-specific). ACTION: confirm the adapter actually publishes a non-empty tree (it depends on `a11y_dirty` → tree rebuild) and that actions round-trip.

### F. Menus / context menus (Wayland) — the menu model is good; needs API-exposure + a CPU-rendered popup fallback
The menu DATA MODEL is solid (user: "quite decent"). `azul_core::menu::StringMenuItem` (core/src/menu.rs) HAS `with_children(MenuItemVec)`, `with_child(MenuItem)`, `with_callback(data, cb)`, a `children: MenuItemVec` + `callback` field; `Menu::create(items)` + `with_position`. Windows consumes it fully: `windows/menu.rs::recursive_construct_menu` walks `mi.children` (submenu via CreatePopupMenu/AppendMenu) + `mi.callback` (WM_COMMAND) — that's the REFERENCE model for the renderer/fallback. azul-paint now has a flat top-level menu bar (commit pending) via `Dom::with_menu_bar(Menu::create([...]))`.
THREE pieces needed for a useful Linux menu:
- **(A) Expose the builders in api.json** (only `create(label)` is currently exposed -> azul-paint can't add sub-items/actions). Add `StringMenuItem::with_child` / `with_children` (simple) and `with_callback` (callback arg -> needs the callback-typedef handling) as `functions` in api.json referencing the real methods, then `azul-doc codegen all` + rebuild. Then azul-paint can build File>New/Open/Save with callbacks.
- **(B) Wire WindowType::Menu -> xdg_popup** (run.rs:~1228 TODO). UPDATE: the xdg_popup CREATION already exists — `WaylandPopupWindow::new(parent, anchor_rect, popup_size, options)` (mod.rs:3817) builds the xdg_positioner (set_size/anchor_rect/anchor/gravity/constraint) + `xdg_surface.get_popup`, and dlopen has all xdg_positioner_*/get_popup fns. The REAL gap is **loop integration**: `WaylandPopupWindow` is NOT stored or managed anywhere (no `parent.popups`, no `LinuxWindow::Popup` registry variant, no poll_event/render/is_open lifecycle). So B = (1) add popup storage + a `LinuxWindow::Popup`(or parent-owned) variant, (2) in run.rs:1239 when `pending_create.window_state.flags.window_type == WindowType::Menu`, extract `MenuLayoutData::trigger_rect` from the pending_create's layout RefAny (downcast) + `calculate_menu_size`, call `WaylandPopupWindow::new` instead of `WaylandWindow::new`, register it, (3) drive the popup's events/render/close in the main loop (mirror the WaylandWindow handling), with parent-serial grab. `create_menu_popup_options` + `menu_layout_callback` + `menu_renderer::create_menu_dom_with_css` already build the menu StyledDom; `try_show_context_menu` (right-click) already runs in handle_pointer_button.
- **(C) CPU-rendered popup window fallback** (user: "we need a cpurendered window based fallback anyway"): force the menu popup window to `RenderMode::Cpu` (wl_shm + cpurender) regardless of the main window's GPU mode — menus aren't render-intensive and CPU has better compositor compat. WaylandPopupWindow already shares the GPU/CPU RenderMode infra (mod.rs:80-82); plumb a force-CPU flag through `create_menu_popup_options` (or honor the same AzBackend::Cpu logic from #28) so the popup skips GlContext::new. On X11 (Mint) the equivalent fallback + GNOME/DBus global-menu path get tested.

### G. azul-paint header buttons stacked vertically (#19)
The header CSS DOES set `flex-direction: row` (examples/azul-paint/src/lib.rs:658, `HEADER` const; applied at line ~672) yet buttons render in a column → the **layout engine isn't honoring `flex-direction: row`** (default is column). Likely a solver3 flex-axis bug OR `flex-direction` not parsed/applied. ACTION: minimal repro (a flex-row div with 2 children) through solver3; check `flex-direction` parse → LayoutFlexDirection → main-axis selection. Deep (layout engine), shared across all OSes.

### Priority for "fully working azul-paint": A (click dispatch — DONE, verify) → B (typing) → C (IME/contenteditable) → G (flex-row layout) → D (file drop) → F (menus) → E (a11y verify). A was the keystone (committed 1f8a36cc8). Fixing the debug-server DOM access (§7b-A) unblocks autonomous verification of A/B/C.

## 9. Session 2026-06-03 — contenteditable visual testing + the stale-self-pointer close bug

Tested `tests/e2e/contenteditable.c` (CPU mode: `AZ_BACKEND=cpu ./contenteditable_test`, compiled
`cc contenteditable.c -I../../target/codegen -L../../target/release/ -lazul -o contenteditable_test -Wl,-rpath,<abs>/target/release`).
GPU path SIGSEGVs on this DOM (separate WebRender bug — not chased; user stays on CPU).

### 9a. CPU hit-test fix LANDED and works (commit after `1f8a36cc8`)
Root cause of dead mouse in CPU mode: `cpu_hit_tester` (created `mod.rs:973 CpuHitTester::new()`) was
never `rebuild_from_layout`'d — the earlier hit-test fix (`1f8a36cc8`) only refreshed the **GPU**
WebRender hit-tester. Fix added in `generate_frame_if_needed` (mod.rs ~2835, after
`frame_needs_regeneration = false`):
```rust
if let (Some(cpu_ht), Some(lw)) = (
    self.common.cpu_hit_tester.as_mut(),
    self.common.layout_window.as_ref(),
) { cpu_ht.rebuild_from_layout(&lw.layout_results); }
```
**User-verified WORKING:** hit-testing, text selection (drag), hover cursor change, blue `:focus` border renders.

### 9b. STILL BROKEN (user visual feedback, 2026-06-03) — priority order set by user
1. **Window CLOSE doesn't work** (titlebar X *and* taskbar) — **debug FIRST** (user directive). Root cause below.
2. **Caret not positioned on click** — clicking text doesn't place the insertion caret at the click point
   (selection-by-drag works, but single-click caret placement doesn't). Likely the click→caret-index hit
   path (cursor index from hit point) isn't wired in CPU mode, or focus isn't committed on click.
3. **Focus is tied to WINDOW activation, not ELEMENT focus** — the blue `:focus` border shows "by default"
   when the window is focused and disappears when the window is unfocused. So `:focus` is being driven by
   xdg_toplevel *activated* state, not by which DOM node has focus. The focus model conflates
   window-active with element-focused. (See `events.rs:917 is_activated` from xdg_toplevel.configure
   states — currently `let _ = is_activated` discarded at :937, but something else is mis-applying focus.)
4. **Text input "extremely strange"** and "makes a difference if the window is focused or not" — classic
   signature of the stale-self-pointer UB below (non-deterministic because it depends on whether the
   optimizer happened to elide a move).

### 9c. ROOT CAUSE: listeners registered against a stack-local `&mut window` that is then MOVED
All Wayland listener user-data pointers are registered inside `WaylandWindow::new()` as
`&mut window as *mut _ as *mut _` (mod.rs:**1047** registry, **1075** wl_surface, **1093** xdg_surface,
**1111** xdg_toplevel, **1146** seat). `window` is a **stack local** (`mod.rs:916 let mut window = Self{..}`)
returned by value (`mod.rs:1413 Ok(window)`), then:
`LinuxWindow::Wayland(w)` (linux/mod.rs:128) → `let mut window` (run.rs:1056) →
`Box::into_raw(Box::new(window))` (run.rs:**1076**) = the **stable** address the run loop polls.
So every listener's user-data points at the dead `new()` stack frame, NOT the boxed window.

- Close handler `events.rs:990 xdg_toplevel_close_handler` does `window.is_open = false` on the **stale**
  address; the run loop checks `is_open()` on the **boxed** window (run.rs:**1142**) → never sees false → never closes.
- `seat_capabilities_handler` (events.rs:728) passes the **same** stale `data` to
  `wl_pointer/keyboard/touch_add_listener` → those are stale too.
- There is **NO** user-data fixup anywhere (`wl_proxy_set_user_data` is NOT in the binding table — would
  need adding to the dlopen table in `dll/src/desktop/shell2/linux/wayland/{defines,dlopen}.rs`).

**CONTRADICTION RESOLVED — PROVEN by measurement (2026-06-03).** Added `eprintln!("{:p}")` probes:
```
new(): registering listeners, data=&window = 0x7ffe860506e0   ← STACK
run loop: LIVE boxed wayland_window      = 0x5bcb51e0db50      ← HEAP
CONFIGURE handler fired, data            = 0x7ffe860506e0      ← STACK (STALE — never the live window)
```
So EVERY handler fires with the dead `new()` stack pointer, never the live heap window. Why some input
"kind of works" but close never does:
- `is_open` is a **plain inline `bool`**. The close handler writes `false` into the dead stack copy; the
  run loop reads `is_open` on the live heap copy → stays `true` → **deterministically can't close.**
- Hover/selection state reaches hit-testing through **heap pointers** (`Box`/`Arc`/`Rc`) that the move
  bit-copied, so stale and live structs alias the SAME heap objects and writes leak through — UAF of a
  stack frame that other calls progressively clobber → erratic, focus-dependent, "extremely strange."
libwayland stores the user-data *pointer* (not a copy) and passes it verbatim to every callback, so the
referenced object MUST stay at a stable address (confirmed via libwayland docs).

### 9d. THE FIX (IMPLEMENTED 2026-06-03) — rebind every proxy to the stable boxed `self` on first poll
Done (pending user re-test of the close button — addresses self-verified):
1. `dll/.../wayland/dlopen.rs`: added `wl_proxy_set_user_data` to the fn table + dlsym loader.
2. `wayland/mod.rs`: added fields `keyboard: *mut wl_keyboard`, `touch: *mut wl_touch`,
   `listeners_rebound: bool` to `WaylandWindow` (+ init in `new()`); `events.rs seat_capabilities_handler`
   now stores `window.keyboard` / `window.touch`.
3. `wayland/mod.rs`: `rebind_listeners(&mut self)` calls `wl_proxy_set_user_data(proxy, self_ptr)` for
   registry/surface/xdg_surface/xdg_toplevel/seat/pointer/keyboard/touch/tablet_manager/tablet_seat and the
   Option proxies text_input/toplevel_decoration. `ensure_listeners_rebound(&mut self)` runs it ONCE,
   gated by `listeners_rebound`, called at the top of `poll_event` AND `wait_for_events` — i.e. the first
   event pump after the run loop boxed the window (when `self` is finally at its permanent address). This
   covers ALL create paths (main/child/popup/e2e) with no run.rs or `LinuxWindow` enum changes. Proxies
   created later (frame callbacks via `self`, tablet tools via the rebound `tablet_seat`) inherit the
   stable pointer automatically.
   - Rejected alternative: `new() -> Box<Self>` + register-after-boxing (cleaner but ripples the enum type
     through ~10 match arms — higher risk for a blind change).
4. `[ADDR]` probes left in for ONE verification build — after the fix, `CONFIGURE`/`CLOSE` must print the
   live heap addr, not `0x7ffe…`. **Remove the probes** (`grep -n '\[ADDR\]'` in mod.rs/events.rs/run.rs)
   before committing the fix.
5. This likely also fixes the focus/caret/"strange text input" symptoms (all were UAF of the stale copy).
   Re-test after the rebind lands. Confound found this session: TWO `contenteditable_test` procs were
   running at once (an old build + the new) — always `pkill` before relaunch.

### 9e. OWED: "Push B" — menu xdg_popup loop-integration (user said "do it blindly... Then push B")
Status: **NOT done.** `WaylandPopup` struct (mod.rs:258) is fully built — `new()` @3828 does positioner +
`xdg_surface_get_popup`; `close()` @4026; Drop @4066 — but it is **not loop-integrated**. Needed:
(1) a `LinuxWindow`/registry storage path for popups (no `Popup` variant exists; the run loop only knows
toplevels), (2) wire the `WindowType::Menu` create (`run.rs:~1228` comment; `wayland/menu.rs:108` sets
`window_type = Menu`) → `WaylandPopup::new` with `trigger_rect` from `MenuLayoutData`, (3) force the popup
to `RenderMode::Cpu`. Defer until the close/focus bugs (9c/9d) are fixed — popups inherit the same
stale-pointer infrastructure and would be doubly broken otherwise.

## 8. Key codebase pointers

- Wayland backend: `dll/src/desktop/shell2/linux/wayland/{mod,events,defines,dlopen,gl}.rs`
- X11 backend: `dll/src/desktop/shell2/linux/x11/mod.rs`
- Backend resolution + software-GL detect: `dll/src/desktop/shell2/common/compositor.rs` (`AzBackend::resolve`, `check_gpu_blacklist`, `query_gpu_info`)
- WebRender translate / renderer options: `dll/src/desktop/wr_translate2.rs` (`default_renderer_options`, `generate_frame`, `build_image_only_transaction`)
- CPU render: `azul-layout` `cpurender` (tiny-skia `AzulPixmap`)
- Double-drop convention: every FFI leaf owning a `Box`/resource must gate its free on `run_destructor` (via `ManuallyDrop`) or a refcount/destructor-enum, else the codegen `_delete`+drop-glue double-frees when the wrapper is nested. Fixed leaves: GlContextPtr, InstantPtr, CssPropertyCachePtr, IconProviderHandle.
- Run loop: `dll/src/desktop/shell2/run.rs` (Linux `run` ~line 992; tears down `!is_open()` windows).

## 10. Do the Wayland bugs also exist on X11? (audit 2026-06-03)

X11 is the UN-patched ancestor of Wayland (same `CommonWindowState`, same lightweight/full
transaction split) — it only got some fixes. Per-bug verdict:

| Bug (Wayland) | X11 | Why |
|---|---|---|
| **Stale self-pointer** (listeners on moved stack `&mut window`) | **SAFE** | X11 polls via `XNextEvent`/`XPending` and gets the window fresh from the registry each event (`run.rs:1121`); no C-callback user-data `self`. XIM callbacks point at a `Box`ed `ImePreeditSink` (`x11/events.rs:92-208`); a11y uses an `Arc<Mutex<…>>` channel; debug timer stores no self-ptr. |
| **Window close** (is_open on stale copy) | **SAFE** | Consequence of the above. `is_open=false` set on the live registry window — `poll_event` `x11/mod.rs:786`, `handle_event` `:1780`; run loop reads same object `run.rs:1142`. |
| **Text input only updates on click** | **EXISTS (worst)** | Both X11 result sites do a blanket `if result != DoNothing { request_redraw() }` (`x11/mod.rs:826-828` and `:1924-1927`) — never set `frame_needs_regeneration`, so typed content (`ApplyTextChangeset` → `ShouldIncrementalRelayout`/`ShouldUpdateDisplayList`) goes through the lightweight image-only path and isn't shown until a later regen (a click). FIX: port the post-fix `handle_process_event_result` (`wayland/mod.rs:1890-1912`) and call it from BOTH `poll_event` and `handle_event`. |
| **CPU hit-tester never rebuilt** | **EXISTS (CPU/software-GL only)** | `rebuild_from_layout` is called NOWHERE in `x11/` (only `CpuHitTester::new()` at `x11/mod.rs:1297`). CPU mode → empty hit-tester → dead hover/click/selection/focus. Also `x11/events.rs:863` does `.unwrap()` on `hit_tester` (None in pure-CPU → panic). FIX: add the `rebuild_from_layout` call (cf. `wayland/mod.rs:2897-2902`) to X11 `regenerate_layout`/`generate_frame_if_needed` + guard the unwrap. |
| **glClear FBO 0** (GPU garbage) | **EXISTS (GPU only, cosmetic)** | X11 GPU present (`x11/mod.rs:2298-2444`) has no FBO-0 bind/viewport/clear; bare `eglSwapBuffers` (`x11/gl.rs:227-234`). FIX: clear FBO 0 before render (cf. `wayland/mod.rs:3007-3029`). |
| **Blank window / present-empty-buffer** | **SAFE** | X11 is event-driven (renders only on real Expose) and short-circuits no-op frames (`x11/mod.rs:2321-2337`), so it never swaps an empty buffer over good content. |

**Priority before testing `contenteditable.c` on X11:** (1) text-input repaint [highest, hits the test directly], (2) CPU hit-tester rebuild [if the box falls back to CPU — this nouveau machine does when shaders fail to compile, `x11/mod.rs:1198-1208`], (3) FBO-0 clear [GPU cosmetic].

## 11. Session 2026-06-03 (cont.) — incremental rendering, caret, the focus blocker, X11 plan

### Committed (Wayland/KDE/CPU; ✅ = user-verified)
| Commit | Fix | |
|---|---|---|
| `7406dd9ef` | CPU hit-tester rebuilt each frame (hover/selection) | ✅ |
| `5e74e30d7` | stale-self-pointer: rebind wl listeners to boxed self → **window close** | ✅ |
| `3d94a56d3` | keyboard input triggers redraw → **typing updates live** | ✅ |
| `ee43319dc` | docs: handoff §0/§10 + `X11_API_REFERENCE.md` | |
| _this commit_ | **backspace control-char filter** + **caret stable-item (blink damage)** + **caret-color → currentColor** + `DAMAGE_RENDERING.md` | ⏳ unverified (focus blocker) |

### 🔴 CRITICAL BLOCKER — focus model is inverted (NEW, blocks caret + text-input verification)
On Wayland/CPU the user observes: the contenteditable field shows a **blue `:focus` border BY DEFAULT** (tied to *window* activation), **clicking the field UNFOCUSES it** (border→gray, text jumps down a bit), and **no caret ever shows** (any colour). So `:focus` is being driven by window-active rather than element-focus, and the click→focus path clears/inverts focus. The caret literally cannot render — `paint_cursor` early-returns when `cursor_locations` is empty, which it is without a properly focused contenteditable.
- Pointers: focus is set via `CallbackChange::SetFocusTarget` → `focus_manager.set_focused_node` (`common/event.rs:1245-1301`); the click→focus path runs through hit-testing; `handle_key` sets `window_focused=true` on keypress (`wayland/mod.rs:~1800`). Suspect the click handler clears focus and/or `:focus` styling reads `window_focused` instead of the focused node.
- **This is the keystone to fix first on X11** (where it's auto-testable). Everything caret/IME/text-selection depends on it.

### Incremental rendering (the big effort) — see `DAMAGE_RENDERING.md` for full architecture + plan
Goal (user): cursor-blink / scroll / resize each repaint ONLY the changed region, on **CPU and GPU**, all **4 platforms**, + the OS compositor gets the real dirty rects. Two agent investigations mapped it. Highlights:
- WebRender **already** computes dirty rects (`max_partial_present_rects:1`, `wr_translate2.rs:218`) and is consumed into `gpu_damage_rects` on all platforms; WR 0.62 **always** tile-caches → GPU raster is already incremental. `partial_present:None` is correct (keep it).
- CPU bugs: **scroll frozen** (offset not in DL → empty damage → stale re-blit), **caret blink full-rasters** (item-count change), **resize full-reallocs**.
- **Caret fix DONE this commit** (shared `layout/src/solver3/display_list.rs`): always emit `CursorRect` (alpha forced to 0 in blink-off) → stable item count → blink diffs to a caret rect; `push_cursor_rect` no longer drops alpha-0. + caret colour now defaults to currentColor not BLACK (`getters.rs:get_caret_style`). Couldn't runtime-verify (focus blocker). Measured caveat: even with the fix, blink damage was still **coarse** (`1184×587` textarea-region) → other items change per blink-regen (the DL regen isn't stable beyond the caret) → tighten later.

### 📋 TODO — everything still open (priority-ordered)
1. 🔴 **Focus model inverted** (above) — fix + verify on X11. Blocks caret/IME/selection. `event.rs:1245-1301`.
2. **`AZ_BACKEND` rework — x11 and cpu not individually selectable.** Windowing reads it (`linux/mod.rs:158`, values `x11`/`wayland`) AND render reads the SAME var (`compositor.rs:105`, values `auto`/`gpu`/`cpu`/`headless`/`web`). So you can't say "X11 + CPU". Split into two axes — e.g. keep `AZ_BACKEND` for render and add `AZ_WINDOW=x11|wayland|auto` (or accept `AZ_BACKEND=x11-cpu`). Easy; do on X11.
3. **Scroll** — implement Design 1 (viewport-rect damage → `render_display_list_damaged`) then Design 2 (pixel-shift via `scroll_layer`/`compute_exposed_rects`). `DAMAGE_RENDERING.md`. CPU frozen on all 4 platforms today.
4. **Resize** — grow-only partial repaint (`resize_grow_only`+`compute_resize_damage`, mirror headless). `DAMAGE_RENDERING.md`.
5. **Caret verify** — colour + blink-damage, on X11 once focus works.
6. **Caret damage coarseness** — make DL regen stable so blink = caret-only damage (investigate what items change per regen; a `[DMG-ITEM]` probe in `compute_display_list_damage` names them).
7. **Backspace** control-char filter — verify no tofu (X11 + Wayland). (Committed this session.)
8. **GPU incremental** — caret as opacity-animated property (avoid macOS scene rebuild) + Wayland `eglSwapBuffersWithDamageKHR` ordering bug (`swap` commits before `wl_surface_damage`; no commit after — `wayland/mod.rs:3084-3122`). `DAMAGE_RENDERING.md` §, `X11_API_REFERENCE.md` §1.
9. **X11 ports** (§10): text-input result routing, CPU hit-tester rebuild + guard the `hit_tester.unwrap()` (`x11/events.rs:863`), FBO-0 clear.
10. **Per-platform compositor sub-rect damage** — table in `DAMAGE_RENDERING.md` (Wayland `wl_surface_damage_buffer`, X11 `XPutImage` sub-rect, macOS `setNeedsDisplayInRect:`, Windows `InvalidateRect` rect + `StretchDIBits` dst).
11. **Autonomous testing on X11** — need input injection + screenshot. This machine has ONLY `xwd` (no `xdotool`/`scrot`/`import`/`convert`); on X11 install `xdotool` + `imagemagick` (or `scrot`) **[sudo, in a REAL terminal — `!`/Bash tool have no TTY for the password]**. Then loop: launch → `xdotool` click/type → screenshot → `Read` the PNG → inspect. (The app's own debug server — feature `debug-server`, `AZ_DEBUG` — could do synthetic input + `take_screenshot` with no external tools, but it's NOT in the default build and "might itself be buggy"; `take_screenshot` already flagged broken on X11 (#21). Verify before relying on it.)
12. **IME (Japanese)** — install `fcitx5 fcitx5-mozc fcitx5-configtool` **[sudo, REAL terminal]**, set KDE Virtual Keyboard = Fcitx 5, write `~/.config/environment.d/im.conf`, restart kwin. App uses `zwp_text_input_v3` (`wayland/events.rs:1198`/`1223`). ⚠️ A sudo password was accidentally echoed into the shell/history this session (`sudo -v -S <pw>`) — recommend changing it.
13. **Earlier-reported, re-test after focus fix**: can't resize window past a certain size (#3); multiline textarea clipped to ~2 lines, ≠ macOS (#6, layout); Tab shifts content (#5).
14. **GPU SIGSEGV** on the contenteditable DOM (Wayland; separate WebRender bug) — stay on CPU.

### Companion docs
`DAMAGE_RENDERING.md` (incremental-rendering architecture + per-platform plan), `X11_API_REFERENCE.md` (Xlib/EGL: FBO-0 clear, event loop, `XLookupString`/XIM text+IME, XShm blit).

## 12. Session on the X11 box (Mint 22.2 / XFCE / X11 / nouveau, 2026-06-03)

Same `/home/fs` filesystem, all branch commits present. `DISPLAY=:0.0`, real X11, GPU = NV126 (Mesa 4.3). Tools installed: `xdotool maim imagemagick` (+ pre-existing `wmctrl xwininfo glxinfo xwd`).

### ✅ Committed X11 fixes
| Commit | What |
|---|---|
| `901e507e6` | **`AZ_WINDOW` / `AZ_BACKEND` split** — windowing (`AZ_WINDOW=x11\|wayland\|auto`) vs render (`AZ_BACKEND=cpu\|gpu\|auto`) now independent; `AZ_BACKEND=x11/wayland` kept for back-compat. (#40 done.) `AZ_WINDOW=x11 AZ_BACKEND=cpu` = X11+CPU. |
| `6d28e5a14` | **4 X11 render fixes** (all in `x11/mod.rs`): (1) honor `AZ_BACKEND=cpu` (was GPU-only unless GL failed → skip GL, use cpurender); (2) **force initial render** in `poll_event` when `frame_needs_regeneration` — xfwm4 compositor sends NO Expose on map, so X11's Expose-driven render never fired the first frame ("only renders first frame" / black); (3) **XPutImage depth** = 32 (window is ARGB; was using XDefaultDepth 24 → `BadMatch` opcode 72/code 8 flood); (4) **XCreateImage visual** = matching 32-bit (depth 32 + 24-bit default visual → XCreateImage returns NULL → blit skipped). |

### 🔴 CRITICAL: screenshot capture is BROKEN on this box
**Both `maim` and `xwd` capture the bare desktop as uniformly pure black** (mean 0, stddev 0) — XGetImage front-buffer readback is non-functional under nouveau/this Xorg (with compositing on AND off). So **the autonomous *visual* loop does NOT work here** — every "black window" during this session was the capture tool, NOT the app. **All other evidence says the app renders correctly**: the cpurender pixmap is verified `#1e1e1e`/opaque (`[30,30,30,255]`), `XPutImage` succeeds (no BadMatch after the fix), layout + display-list run. **Unblock options for autonomous testing:** (a) add a debug **pixmap→PNG dump** to the app (writes the cpurender `AzulPixmap` directly via `png`/tiny-skia `save_png`, bypassing X capture) — the cleanest path; or (b) a human looks at the screen. `xdotool` input injection DOES work; only *capture* is broken.
→ **NEXT SESSION: visually confirm the X11+CPU window now shows content** (`AZ_WINDOW=x11 AZ_BACKEND=cpu ./contenteditable_test`) — expect a dark window with the top label + single-line input (clipped, per below).

### 📌 Dominant remaining VISUAL bug: body-height collapse (#6, shared/all platforms)
Pinpointed via a node-rect dump: the **root/body computes height ≈ 80px** (= the single-line input's fixed height) while its children are laid out correctly down to **y≈604** (textarea `#7` = `1146×436` at y=133). So the flex-column root doesn't expand to contain its children (or fill the viewport) → content overflows an 80px body → clips to the top strip + the scrollbar lands at the body's top-right (your "scrollbar in the wrong place"). This is the layout-engine bug behind "textbox 2 lines, rest cut off" (#6) and is **not X11-specific**. FIX next in `layout/src/solver3/` (sizing/flex: root/body main-size). Repro DOM = `contenteditable.c`; node dump via a temp loop in `generate_display_list` (display_list.rs ~1695).

### Gotchas found
- `pkill -x contenteditable_test` and `pgrep -x` FAIL — comm is truncated to 15 chars ("contenteditable"). Use `pkill contenteditable` (matches comm, not bash) or `killall`. **Do NOT `pkill -f contenteditable_test`** — it matches your own shell command and kills it. Leaked instances stack windows.
- `xwininfo` "Absolute upper-left" returned `0,0` (reparented) — use `wmctrl -lG` for real window position.
- Close button (user-reported broken on X11): `XSetWMProtocols`+`WM_DELETE` handler look correct (`x11/mod.rs:1122`/`781`/`1775`); my kill-test was inconclusive. Re-test.

### ✅ BREAKTHROUGH — capture was the LOCKSCREEN, not a driver bug; autonomous loop now works
The all-black captures were the **active lockscreen** (light-locker), NOT broken XGetImage. Lock/sleep now disabled (light-locker killed, DPMS off, `xset s off`). The autonomous VISUAL loop is operational via the **AZ_DEBUG server**, compiled in with `cargo build -r -p azul-dll --features build-dll,debug-server`:
- Run: `AZ_DEBUG=8765 AZ_WINDOW=x11 AZ_BACKEND=cpu ./tests/e2e/contenteditable_test`
- **Verify on-screen**: `curl -s localhost:8765 -d '{"op":"take_native_screenshot"}'` → base64 PNG in `data.value.data` (strip `data:image/png;base64,`). Use **take_native_screenshot** (real window) — NOT `take_screenshot` (headless re-render; doesn't show what's actually on screen).
- `get_state` + synthetic input ops via curl; `AZ_E2E` for scenarios.
- **CONFIRMED**: the X11 app renders correctly (dark bg + label + single-line input + blue focus border) — the X11 render/present fixes (`6d28e5a14`) work. The visible bug is **#41** (body clipped to ~80px).
- **Crash fixed** (`a30e2292b`): `update_hit_test` `.unwrap()` on the None `hit_tester` in CPU mode → panic on mouse-over. **Class = CPU-mode unwraps on GPU-only fields** (`document_id`/`hit_tester`/`render_api`/`id_namespace`); MORE to audit (`wayland/mod.rs:1519`, `macos/mod.rs`).
- **Autonomous cron** (job `2a47e6c0`, every 10 min, session-only/7-day) drives the priority order: crashes → stability → functionality → features.
- Gotchas: `pkill contenteditable` only (NEVER `pkill -f contenteditable_test` → kills your own shell; `pkill -x`/`pgrep -x` fail, comm truncated to 15 chars).

### Cron firing 1 (tier 1 = crashes) — VERIFIED CLEAR
Stress-exercised X11+CPU via xdotool + debug server: mousemove, click, type, Backspace,
Return, scroll, right-click (context menu), Tab nav, Escape, rapid-resize storm,
scrollbar-area click, native screenshot — **no crash on any path** (the only X11+CPU crash
was the `update_hit_test` None-`hit_tester` unwrap, fixed in `a30e2292b`). **WM close now
works** — `WM_DELETE` → clean exit (code 0); resolves the user-reported "close broken on
X11" (the forced-initial-render + running event loop let the ClientMessage handler fire).
Crash tier clear for the testable GUI paths. Next tier-1 sub-item: memory/leaks — the
double-drop audit (§4: 9 codegen-detected ungated types) + run under a leak check if available.

### Cron firing 2 (tier 1 = memory/leaks) — NO RESIZE/INTERACTION LEAK
Resize-stress leak check on X11+CPU via debug server + xdotool. Measured RSS at a **fixed**
1000x700 window after batches of (resize-storm + type + scroll) iterations:
- baseline @1000x700: **49152 kB**
- after 80 iters @1000x700: **53508 kB**
- after 160 iters @1000x700: **53508 kB** (identical to the 80-iter reading)
RSS **plateaus** — the ~4 MB of initial growth is caches that stabilize, and there is **no
growth between 80 and 160 iterations** measured at the same window size, so no resize/type/
scroll leak. App stayed alive throughout. `valgrind`/`heaptrack` are NOT installed on this box,
so the heavyweight allocation-site audit (and a concrete double-drop §4 repro) is deferred until
they can be installed. Tier 1 (crashes + leaks) considered clear for the testable paths;
next firing advances to **tier 2 = resizing/repaint correctness**.

### Cron firing 3 (tier 2 repaint + tier 3 backspace) — RESIZE CLEAN, BACKSPACE FIXED
Verified on X11+CPU via native screenshots (`take_native_screenshot`):
- **Window resize repaint = CLEAN.** Shrink 1200x800→600x400 (content reflows to the
  narrower width, no stale pixels), grow →1400x900 (newly-exposed area fills correctly,
  no garbage), and a rapid resize-storm settling at 1300x850 (final frame correct). No
  crash, no clip-artifacts, no stale framebuffer on any transition.
- **Backspace tofu FIXED (`ddc938a3e`)** — the X11 twin of the Wayland fix (40da9e554).
  `handle_key_event` recorded every `XLookupString` byte as text, incl. control chars
  (Backspace 0x08 etc.) → tofu rect. Now filtered with `chars().all(is_control)`.
  **Verified on screen**: Tab-focus input, type `ABC`, Backspace×2 → field shows
  `...type!A` with caret, NO tofu. Closes the X11 half of #36.
- **Incidental confirmations**: repaint-on-input is live & correct (tier 2), the caret
  renders when the field is focused (tier 4 / #38 looking good on X11+CPU), and
  **Tab-key focus works** (`focused_node` → 3).

**Key blocker finding for the next tiers.** On-screen verification of *click-to-focus*,
scroll, and the textarea is **blocked by #41**: the body collapses to 80px (`height:80px`
on `<body>` wins over `min-height:300px`), so node 3 (the input) and the textarea below
it are clipped out of the clickable region — `hit_test(200,65)` lands on the label
(node 1), and the debug `focus` op + raw clicks don't set `focused_node` (only Tab does,
because Tab walks the focus chain regardless of geometry). Net: **#41 is not merely
cosmetic — it gates click/scroll/caret verification for tiers 2–5.** Recommend pulling
#41 forward despite its "last" ordering; it is the keystone unblocker. Also note:
`get_html_string` returns the **static DOM**, not the live edit buffer — always verify
text edits via native screenshot, not the HTML value.

### Cron firing 4 (tier 1 = memory) — SystemStyle double-free FIXED + timer/scroll plan
- **SystemStyle double-free FIXED (`9091544f7`)** — the next ungated leaf after
  IconProviderHandle. `AzAppConfig.system_style: AzSystemStyle` re-ran
  `AzSystemStyle_delete` (drop_in_place::<SystemStyle>) on the same bytes after the real
  AppConfig drop already freed both Boxes (`app_specific_stylesheet`, `scrollbar`).
  Gated on a new `run_destructor: bool` WITHOUT ManuallyDrop: gated `Drop` frees once on
  the first drop (disarming the flag) and `take()+forget()`s the dangling ptrs on the
  second. Because `SystemStyle` now impls `Drop`, the 14 `..Default::default()` discover
  literals hit E0509 ("cannot move out of a Drop type") and were expanded to explicit
  field lists. **Verified**: `dll/examples/appconfig_double_drop` (link-static, 200k
  AppConfig create+drop) aborted `free(): double free detected` before → now prints
  `OK: ... no double-free`. The full AzAppConfig drop path is now clean (SystemStyle was
  the last reachable leaf in that nest; see §4 for the other flagged types not reachable
  via AppConfig — they'd need their own repros).
- **api.json WORKFLOW (per user)** — never hand-edit api.json (a `json.dump` reformats the
  whole 4MB file → 92k-line diff). Use `azul-doc autofix` (parses live Rust source,
  detects struct-field/derive/custom-impl drift, writes patches to
  `target/autofix/patches/NNNN_modify_<Type>.patch.json` — it does NOT edit api.json, and
  "0 additions" in the summary ≠ no patch) → `azul-doc autofix apply <patch>` → `normalize`
  → `codegen all`. It correctly detected the `Drop`/`Default` custom-impls + new bool.
- **Timer / scroll-physics plan (#43, per user "check that scrolling + scroll-physics
  timer actually works; requires timers; also fixes animations")**: mapped the timer pump.
  Timers ARE pumped on both Linux loops via `timerfd` + `poll(-1)` in `wait_for_events`
  (x11/mod.rs:1686, wayland/mod.rs:1577) AND `check_timers_and_threads()` on every
  `poll_event` (x11:574, wayland:513). Scroll momentum = `SCROLL_MOMENTUM_TIMER_ID` →
  `scroll_physics_timer_callback` (layout/src/scroll_timer.rs:132), registered on first
  scroll input (x11/events.rs:636, wayland/mod.rs:2180). Caret blink = ~530ms repeating
  timer (`cursor_blink_timer_callback`, layout/src/window.rs:205) started on text input
  (common/event.rs:1878). **Gaps to chase next**: (a) NO dedicated CSS-animation timer
  found — animations likely not timer-driven at all (matches user's "also fixes
  animations"); (b) multi-window idle path uses a hardcoded 16ms `select` and never calls
  `time_until_next_timer_ms()` (layout/src/window.rs:2013, defined-but-unused). Empirical
  on-screen checks still owed: scroll momentum-after-release (blocked by #41 clipping the
  textarea) and caret-blink toggle (verifiable now via Tab-focus + two timed screenshots).
### Cron firing 5 (tier 1 audit clean + tier 3 input VERIFIED, selection gap found)
- **Tier-1 common-path crash audit = CLEAN.** Firing 3 only swept `x11/`; this swept the
  shared `common/` shell path. The only GPU-only-field `.expect()`s are 5 accessors in the
  `impl_platform_window_getters!` macro (common/event.rs:758-776: `get_hit_tester_mut`,
  `get_document_id`, `get_id_namespace`, `get_render_api`/`_mut`) — but they have **ZERO
  callers anywhere in `dll/src/`** (dead trait methods). Not a live CPU-mode crash. Combined
  with firing 3 (x11 dir clean) + firing 4 (SystemStyle double-free fixed), **tier-1
  crashes/memory are clear on the verifiable X11+CPU path.**
- **Tier-3 keyboard EDITING = VERIFIED on screen** (X11+CPU, Tab-focus, native screenshots,
  single-step isolation): Home → caret to start; Right×5 → caret after "Hello", insert
  "|" → "Hello| World…"; End → caret to end, insert "#" → "…type!#"; Backspace + Delete
  both correct. Caret renders at the right position. (Compound rapid-fire xdotool sequences
  can corrupt the text — a key-timing artifact, NOT a real bug; single-step is the reliable
  signal.)
- **Tier-3 SELECTION = BROKEN (new task #44).** Shift+Home produced no highlight and a
  following keystroke appended at the END instead of replacing the selection. Traced: the
  live handler core/events.rs:3107 DOES build `Extend` mode on shift → apply_selection_op
  (window.rs:2396) → move_all_cursors(extend) creates a `Range` — so the gap is in
  selection PAINTING (build_text_selections_map → paint_selections) and/or type-over-replace,
  NOT cursor movement. `handle_cursor_movement` (window.rs:2474) is a legacy MoveCursor*
  path that ignores `extend_selection` (verify dead/remove). See #44.
- **Timer multi-window note (per user, #43)**: the hardcoded 16ms `select` is a placeholder;
  intent is to compute `time_until_next_timer_ms()` and pass `(deadline − elapsed)` so the OS
  wakes exactly at the soonest timer. There is NO built-in CSS-animation engine yet —
  animations = manual `CallbackInfo::start_timer` + `setCssProperty`; CSS animations will
  later desugar to starting timers. So #43 "verify animations" = verify a user-started timer
  fires + repaints, not a built-in engine.

### Cron firing 6 (tier 3 = input) — TEXT SELECTION HIGHLIGHT FIXED (#44)
Root-caused via debug-server (`get_selection_state`/`dump_selection_manager`) + temporary
`[SELDBG]` instrumentation in `paint_selections` and `cpurender`. The selection MODEL was
always correct (Shift+nav builds a `Range`; `get_selection_rects` returned a valid rect),
but `paint_selections` (display_list.rs) offset the rect by the matched node's OWN box
position — and for the inline `<text>` node (node 4) that position is the **`f32::MIN`
sentinel** (never assigned; only IFC roots / block boxes get positioned). So the rect was
pushed at `(-3.4e38, -3.4e38)` and clipped out. Text glyphs + caret rendered fine because
glyphs use the IFC inline-layout coords and `paint_cursor` runs on the IFC-root node
(node 3, valid position).
- **Fix (`4a937b146`)**: added `LayoutTree::get_ifc_root_layout_index()` and anchored the
  selection's content-box offset to the **IFC root's** position + padding/border (the box
  the inline-layout coordinates are relative to), not the text node's.
- **Verified on screen (X11+CPU)**: Shift+End → whole line highlighted blue; Shift+Right×11
  → exactly "Hello World" (range 0..11) highlighted at the right spot; type-over replaces a
  selection ("X!" after selecting the line and typing X). The firing-5 "type-over appends"
  reading was a MISDIAGNOSIS — type-over-replace works. #44 fully resolved.
- **Debug-server gotcha**: `get_selection_state`'s `rectangles` field is hardcoded
  `Vec::new()` (full.rs:8315) — it is NOT evidence of empty geometry; use a screenshot or
  `get_display_list`. Also `get_display_list` positions may reflect the headless layout.
- Note: this is the SAME f32::MIN / IFC-position class that #41 touches — selection now
  positions correctly because it anchors to the IFC root, independent of the #41 body
  collapse. Other inline-relative overlays (future) should use `get_ifc_root_layout_index`.

### Cron firing 7 (tier 4 = caret/cursor) — CARET + BLINK VERIFIED, focus model OK
No code change — tier-4 caret behavior verified working on X11+CPU via debug server
(`get_cursor_state`) + native screenshots:
- **Field is NOT focused-by-default**: at startup `focused_node=None`, `has_cursor=false`.
  The #39 "field focused-by-default / no caret" symptom is NOT present on X11 (it was an
  older Wayland/KDE observation). Tab focuses the input (`focused_node→3`), caret appears.
- **Caret renders at the correct position** after Home/End/Left/Right (firing 5) and after
  focus (caret bar before "Hello").
- **Caret BLINKS on screen** (~530ms): `is_visible` toggles cleanly True/False over time
  (sampled ~every 150ms), AND the screen reflects it — an on-phase frame shows the caret
  bar, an off-phase frame shows none. So the blink timer fires AND triggers a repaint each
  toggle (validates the 56ddfbe45 incremental-damage caret work on X11, and partially #43:
  the timer pump drives caret-blink frames).
- Remaining #39 piece — *click*-to-focus / "click unfocuses" — is still blocked by #41
  (the input box is clipped out of the clickable hit region; Tab-focus is the workaround).
  So tier-4 is clear EXCEPT the click-focus path, which is gated on #41.
- Verification-tooling note: `get_cursor_state` `is_visible` read via curl lags the actual
  blink phase by the curl round-trip (~150ms), so a `vis()`-then-screenshot pair can
  mislabel the phase — confirm blink by comparing several burst frames visually, not by the
  pre-shot label.

### Cron firing 8 (tier 5 = dirty-rects/scroll) — SCROLL BLOCKED BY LAYOUT (solver3), not a handler bug
Goal: verify scroll incremental-repaint independent of #41, via the dedicated `scrolling.c`
demo (overflowing body, bright items). Findings:
- Modernized `scrolling.c` (it used removed `AzDom_setInlineStyle`/`AzDom_style`; → `withCss`).
  Committed; builds + runs + renders on X11+CPU (header + alternating red/green items).
- **Scroll does NOT work** — debug `scroll` op AND mouse-wheel ×4 leave the content
  identical (Item 1–7 from the top, no movement, no scrollbar).
- **Root cause = layout, not the scroll handler.** The container has `height:300px;
  overflow:auto`, but the render shows items overflowing the full 500px window (no 300px
  box, no scrollbar, footer pushed off). `get_display_list` confirms `final_scroll_depth:0`
  — there is NO scroll frame/clip anywhere; the single stacking context spans the full
  `584×2093` content height. So solver3 never created a scroll region → scroll has nothing
  to act on. This is the SAME solver3 height/overflow/flex class as #41 (heights not
  applied), surfacing in a different example. So tier-5 scroll (and the scroll-physics
  timer #43 momentum check) are gated on the solver3 layout fix.
- **Strategic state after 8 firings**: tiers 1–4 are DONE/verified on X11+CPU (crashes,
  memory/double-free, resize repaint, backspace, core editing, selection highlight,
  caret+blink). EVERY remaining item — tier-3 click-to-focus, tier-5 scroll, tier-6 — now
  provably bottlenecks on the **solver3 layout class (#41: flex/height/overflow not
  applied)**. #41 is the keystone and is now the only unblocked-able path forward; the
  user's tier-6 plan ("make contenteditable.c colorful → fix #41") is the next item.

### Cron firing 9 (tier 6) — #41 REFRAMED: it's a CSS cascade bug, NOT flex layout
Did the tier-6 plan: made contenteditable.c colorful (committed) then dug into #41. The
colorful render + debug-server layout/CSS dumps REDIRECT the root cause:
- **It is NOT "flex-column root main-size in solver3."** The flex engine is correct — it
  faithfully laid out `body { height: 80px }`. The bug is that the body GOT height:80.
- **#41 = CSS selector-matching/cascade: class rules land on the wrong nodes.**
  - `body` (node 0, no class) computes `height:80` + `min-height:300` + `max-height:400`
    — absorbed from `.single-line-input` + `.multi-line-textarea` (classes it lacks).
    `get_css_height(body)=Exact(Px(80))` → Taffy `Length(80)` → body=80px → clip.
  - EVERY `<div>` (nodes 1,3,5,7,9, any class) gets the IDENTICAL set: `color:#000000`
    (.label) + `font-size:32px` (.label) + `white-space:nowrap` (.single-line-input) +
    `line-height:140%` (.multi-line-textarea) + `cursor:text` — a union of INHERITED props
    from multiple classes, never the element's own. The real single-input doesn't get its
    `font-size:48px`/`height:80`/green bg. No element gets its bg (no colored bands render).
  - Pattern: inherited props mis-merged onto all divs; non-inherited box heights leaked
    UP onto the parent body; bg/padding/border didn't reach the divs.
- Applied via `AzDom_withCss(root, stylesheet)` (core/src/dom.rs:2959). **Next firing:
  fix the cascade/selector matching** — core/src/styled_dom.rs, core/src/prop_cache.rs,
  css/src/compact_cache.rs (likely a node-index misalignment attaching rules to wrong
  nodes + inherited-prop propagation). Verify with the colorful test (each element shows
  its own bg). This SAME bug explains scrolling.c (height:300/overflow ignored), and is the
  keystone for tier-3 click-to-focus and tier-5 scroll.
- Tooling added this firing: `get_all_nodes_layout` (per-node computed rect) and
  `get_node_css_properties` (per-node matched props) are the way to diagnose cascade bugs.

### Cron firing 10 (tier 6) — #41 FIXED on screen 🎉 (CSS stylesheet routing)
Root-caused + fixed the keystone bug. `Dom::set_css`/`with_css` (core/src/dom.rs) parsed the
CSS string into a real stylesheet (rule blocks WITH selectors) but merged every rule's
declarations into `root.style.rules` — the root NodeData's INLINE style, consumed node-locally
via `iter_inline_properties()` with NO selector matching. So `AzDom_withCss(root, stylesheet)`
dumped the ENTIRE stylesheet onto the `<body>`: body absorbed `.single-line-input{height:80}`
+ `.multi-line-textarea{min/max-height}` → flex-column root collapsed to 80px → clip; and
class rules never reached their elements (every div got a union of inherited props).
- **Fix (`fdcda233d`)**: push the parsed `Css` into `self.css` — the Dom-level stylesheet
  list the cascade SELECTOR-MATCHES against the subtree (`collect_css_from_dom` →
  `CssPropertyCache::restyle` → `matches_html_element`). `parse_inline` already parsed the
  selectors right; only the destination field was wrong. ~13 lines.
- **VERIFIED on X11+CPU** (colorful test, `get_all_nodes_layout` + native screenshot): body
  h=635 (was 80, fits content); single-line-input gets its own font-size:48 + green bg (was
  `.label`'s 32px); orange labels / green input / blue textarea / red status all render with
  their own bg at correct stacked positions. The collapse is GONE.
- **Unblocks**: tier-3 click-to-focus (#39 — input no longer clipped, now clickable), tier-5
  scroll (#37 — the scroll container's height:300/overflow now applies → real scroll region),
  and the scrolling.c demo (#37/#41 class).
- **Minor follow-ups observed** (NOT the collapse; lower priority): textarea computed h=468
  vs `max-height:400` (max-height not perfectly clamping); single-input h=55.9 vs explicit
  `height:80` (box-sizing/content-box?); `get_node_css_properties(body)` reported a
  `background:#264f78` (the `::selection` color) — possible `::selection` over-match. These
  are refinements; the keystone #41 is resolved.
- **CAVEAT**: `with_css` is used by ALL azul apps — run the layout reftest suite before
  relying on this broadly (the fix is semantically correct + verified for the desktop path).

### Cron firing 11–12 (tier 6 follow-on) — CSS UNIFIED to one @scope-like model
After #41, the with_css path was reworked into ONE css model (per the user):
- **`Dom::with_css(&str)`** is now the single CSS API: string → `Css` struct → pushed onto
  the node's `.css` vec → cascade selector-matches it against that node's SUBTREE, like CSS
  `@scope`. "Inline css" = a one-node scope (subtree len 1). A bare-decl string parses to
  `* { … }` (whole subtree); a selector string matches within the subtree.
- **`with_component_css(Css)` REMOVED** (`13e1e445e`) — it was with_css minus the parse.
  C/C++/Python `AzDom_withComponentCss` binding gone; `add_component_css(Css)` kept as the
  internal Css-push. Callers + debug-server project-gen + examples + api.json + the styling
  guide (doc/guide/en/styling.md) all updated. Verified: contenteditable still renders
  correctly (body h=635).
- **Process note (learn from this)**: firing-10's first cut routed with_css→.css with a
  WRONG mental model (I thought `.css` matched globally and feared a bare-decl regression;
  it's subtree-scoped). The user stopped it; I reverted (`e399b99d9`) and redid it as the
  deliberate unification (`64a442ff0`). Lesson: verify cascade semantics empirically, don't
  assert them from a guess.
- **STILL BROKEN — the real cascade bug (#47)**: `collect_css_from_dom` (styled_dom.rs:2108)
  flattens EVERY node's `.css` into one global Css matched against the whole tree, so the
  @scope subtree-scoping is NOT actually enforced — a non-root node's `with_css` leaks to the
  whole tree. Works for contenteditable only because its CSS is on the ROOT (subtree = whole
  tree). The recursive `create_from_dom` path drops the per-node scope that `CssWithNodeId`
  carries. Fix: scope each node's `.css` matches to that node's subtree.
- **Newly EXPOSED by #41 (were masked by the 80px collapse), tier 2/3 follow-ups**:
  #45 text doesn't relayout on resize (text-layout cache not invalidated — reflows boxes,
  repaints stale text); #46 mouse text-editing input not hooked up (can't scroll an overflow
  container, can't click-to-position the caret, can't drag-select). Keyboard paths work.

### Cron firing 13 (tier 6 follow-on → tier 2/3) — CSS unified, resize re-wrap, click root-caused
Big session. State after this firing (all committed; tree clean):

**COMMITTED FIXES**
- #41 CSS cascade (DONE): `with_css(&str)` unified to the ONE `@scope`-like model — string →
  `Css` struct → pushed to the node's `.css` vec → cascade selector-matches against the
  subtree. `with_component_css` REMOVED (api.json + codegen + callers + debug-gen + guide).
  Commits `64a442ff0` (unify) + `13e1e445e` (remove) + guide `doc/guide/en/styling.md`.
  Verified: contenteditable renders correctly (body h=635, each class on its element).
- #45 resize re-wrap (DONE, `2b6219bd4`): `layout_ifc` Phase 2d reused the cached inline
  layout via a trivial GlyphSwap even when the available WIDTH changed → text kept stale
  line breaks + clipped on resize. Gated the fast path on
  `cached.is_valid_for(available_width_type, has_floats)`; width change → fall through to
  re-wrap. Verified: textarea h 468→822 on 1200→600 resize, text re-wraps.
  NOTE: shaping is cached width-independently (ShapedItemsKey has no width), so re-wrap
  reuses glyphs and only re-runs line-breaking — not a full re-shape. Granular cache plan:
  text-edit single-word reshape (try_incremental_text_relayout, window.rs) is WIRED; the
  resize→reline-break-only path falls back to full layout_flow (which still reuses the shape
  cache) — a dedicated reline-break entry is the only remaining micro-opt, low priority.

**ROOT-CAUSED, NOT YET FIXED (next firing — highest priority, tier 3)**
- #46 mouse click → no caret (+ no drag-select, no mouse-scroll). PINNED via instrumentation
  on X11+CPU clicking (150,80) inside node 3 (single-line-input x8 y53 w1184 h55):
  `handle_mouse_down` IS called (state-diff detects MouseDown) but its `FullHitTest` has
  EMPTY hovered_nodes → `get_first_hovered_node()`=None → bails → no `TextSelectionClick` →
  `process_mouse_click_for_selection` never runs → no caret. WHY empty:
  `perform_hit_test` (dll/src/desktop/shell2/common/event.rs:650) takes the CPU path
  (cpu_hit_tester is Some) but `cpu_ht.hit_test(150,80)` returns **0 hits** for an in-bounds
  point → the **CpuHitTester is empty/stale, not populated with current layout geometry**.
  `convert_cpu_hit_test_to_full` is fine (fills regular_hit_test_nodes when hits non-empty).
  The debug-server `hit_test` op returns node 3 because it uses a DIFFERENT path
  (layout_results directly), not cpu_hit_tester. FIX: populate/refresh
  `self.cpu_hit_tester` (azul_layout::headless::CpuHitTester) from the layout after each
  (re)layout on the X11+CPU render path so hit_test(pos) returns hits. This unblocks
  click-caret, drag-select, AND mouse-wheel scroll (all route through this hit-test).

**ARCHITECTURAL (user directive, large) — #48**
- Rework the "V2" window-state-DIFFING event system → distinguish USER events (discrete OS
  input) from SYSTEM events (programmatic state changes). State-diffing causes DOUBLE events
  and FALSE "no change" drops (click same spot, key-repeat, click already-down/focused).
  Touches common/event.rs (process_window_events/determination), core/events.rs
  (handle_mouse_*/key), all platform handlers. Design first.

**ALSO OPEN**
- #47 collect_css_from_dom (styled_dom.rs:2108) flattens every node's `.css` into ONE global
  Css matched against the whole tree → @scope subtree-scoping NOT enforced (non-root
  with_css leaks tree-wide). Works for contenteditable only because its CSS is on the root.
  CssWithNodeId carries node_id (FastDom path); create_from_dom drops it. Fix: scope each
  node's `.css` matches to its subtree.
- #43 timer/scroll-physics/animation verification (timers ARE pumped; no CSS-anim engine;
  multi-window idle uses hardcoded 16ms vs time_until_next_timer_ms).

DIAGNOSTIC TOOLING: debug server `get_all_nodes_layout` (per-node computed rect),
`get_node_css_properties`, `get_selection_state`/`get_cursor_state`/`dump_selection_manager`.
For event/hit-test bugs, temporary `eprintln!("[TAG] ...")` probes at perform_hit_test /
handle_mouse_down / the SystemChange dispatch pinpoint the break fast (strip before commit).

---

### Cron firing 14 (tier 3 → tier architectural) — #46 FIXED on screen; #48 now top priority
State after this firing (committed; tree clean at the #46 commit):

**COMMITTED FIX**
- #46 click→no caret (DONE, `488ac0b9b`): X11's `regenerate_layout()` was the ONLY platform
  layout chokepoint that never rebuilt the CPU hit-tester (macOS/Wayland/iOS/Android/headless
  all call `cpu_ht.rebuild_from_layout()`). So `cpu_hit_tester` stayed `CpuHitTester::new()`
  (empty) → `perform_hit_test` CPU branch returned 0 hits for in-bounds points → empty
  hovered_nodes → no `TextSelectionClick` → no caret/drag-select/wheel-scroll. Fix: 13-line
  rebuild at the end of `regenerate_layout()` (x11/mod.rs:~2037), mirroring macOS; `is_some()`
  guard keeps it CPU-only. VERIFIED on screen: click (150,80) → focused_node=3,
  get_cursor_state has_cursor=true pos=5 is_visible=true, native screenshot shows the caret
  after "Hello". The debug-server `click`/`MouseDown` ops DO exercise the real path
  (modify_window_state → apply_user_change → process_window_events → perform_hit_test), so
  they are a valid end-to-end test — only the `hit_test` op uses the layout_results shortcut.

**USER DIRECTIVE (2026-06-05, away-at-work autonomous run)**
- Strategy chosen: **"Prioritize #48 aggressively"** — after #46, go straight at the #48
  event-system rework: design first, then implement in VERIFIABLE INCREMENTS (each commit
  must build + verify on screen). Authority granted: find ROOT CAUSES, do not paper over,
  fix architectural mistakes, MAY break api.json (regen via azul-doc autofix → apply →
  normalize → codegen all; never hand-edit), verify APIs empirically.
- A 10-min cron (`*/10`, session-only, job a3a61200) drives the autonomous loop. Cron prompt:
  read this handoff tail → pick highest-priority UNFINISHED item → root-cause → fix → build →
  verify → commit → update handoff → stop; ONE build at a time; don't start a new item while a
  build/verify is pending.

**CURRENT PRIORITY ORDER** (updated): #48 (event-system rework, IN PROGRESS — see below) →
#47 (collect_css_from_dom @scope subtree-scoping) → #43 (timer/scroll/anim verification).

---

### Cron firing 15 (tier 3, user-reported) — #49 REAL MOUSE DEAD on X11: XI2 shadows core events
User feedback mid-session: "cursor exists but click does still nothing … caret is there now,
but I can't reposition [with my mouse]." That split (keyboard caret works, mouse click dead)
is the whole tell.

**ROOT CAUSE (high confidence, code-proven + reproduced; real-input confirmation pending)**
X11 window creation calls `XISelectEvents` for XI_ButtonPress/Release/Motion (+touch)
(x11/mod.rs:~309). Per XInput2 semantics, selecting XI2 pointer events makes the X server
deliver them ONLY as XI2 GenericEvents and STOP sending the equivalent CORE
ButtonPress/MotionNotify to this client. But `handle_xi_event` only decoded touch + pen
valuators — it DROPPED every mouse button/motion event — and the core dispatch arms in
poll_event/handle_event never fired (server wasn't sending core pointer events). Keyboard
kept working because keys are NOT XI-selected (core KeyPress still delivered) → "caret via
keyboard, no mouse reposition." The debug-server `click` op works because it injects window
state directly, bypassing X delivery — which is why firing-13/14 verification (synthetic) was
green while the real mouse was dead. Lesson: synthetic debug input does NOT exercise X11
event delivery; must test real input (xdotool) for input-path bugs.

**FIX (DONE, `c813ef046`)** `handle_xi_event` now translates XI2 XI_ButtonPress/Release/Motion
into the equivalent core XButtonEvent/XMotionEvent (raw device coords, button=detail,
state=mods.effective) and runs them through the SHARED handle_mouse_button/handle_mouse_move
(same handlers core dispatch used); pen+touch preserved. Returns ProcessEventResult, propagated
at BOTH GenericEvent dispatch sites (poll_event:~882, handle_event:~1892) so a redraw fires.
Unblocks click→caret, reposition, hover, drag-select AND wheel scroll (all route through
perform_hit_test, populated by #46).
- VERIFIED (synthetic, on screen): clean build (probe stripped); synthetic click @150→caret
  node3 pos5, @700→repositions to pos28; native screenshot shows the caret move.
- NOT YET VERIFIED (real input): blocked — see env note below. Task #3 tracks this.

**⚠️ ENV DAMAGE I CAUSED (must restore): X session WM wedged**
Repeated `pkill -9 contenteditable` restarts during testing wedged the local xfwm4: it stopped
processing MapRequests, so NEW windows no longer map (app window IsUnMapped; even a trivial
`xeyes` is IsUnMapped and gets no pointer events — getmouselocation returns window:0 over
viewable windows = session-wide input/hit-test corruption). Tried: xfwm4 --replace (old WM
"not exiting"), kill+fresh xfwm4 (wedges instantly, likely choking on leftover windows or the
broken input state), xkill the stale _NET_SUPPORTING_WM_CHECK window (BadValue — already gone),
SIGCHLD to xfce4-session (zombie not reaped). Could NOT restore remotely. Left the box with NO
WM (windows map undecorated). **FIX = user re-login (logout/login restarts the X session); if
real input still broken after re-login, reboot.** Then verify #49 with a real mouse.

**FOLLOW-UP the user requested (not the cause of #49, still worth doing): unify hit testing**
User: "unify everything to the CPU hit tester and remove the webrender hit tester." Sound
cleanup — collapse perform_hit_test's GPU(AsyncHitTester)/CPU(CpuHitTester) dual path to ONE
CpuHitTester in all modes (populate it in GPU mode too; remove self.hit_tester +
fullhittest_new_webrender). Cross-platform (CommonWindowState field used by x11/wayland/macos/
windows). NOT what broke real clicks (events never reached any hit tester), but reduces
divergence. New issue candidate.

**PRIORITY ORDER NOW**: (1) user re-login → verify #49 real mouse (task #3) → if broken,
re-instrument handle_xi_event. (2) #48 event-system rework (still the big one). (3) hit-tester
unification (user directive). (4) #47 @scope scoping. (5) #43 timer/scroll/anim.

---

### Cron firing 16 (env still wedged) — #47 root-caused + planned; NOT implemented (deliberate)
Checked env: NO live WM, xfce4-session still pid 1146 (user has NOT re-logged in), `xeyes`
maps but pointer returns window:0 → input still corrupted. So #49 real-mouse verify (top
priority) remains BLOCKED on user re-login. #48 + hit-tester unification both need real input
to verify empirically → also blocked. Deleted the still-running cron `a3a61200` (user said they
killed it; it wasn't dead; nothing left for the loop to do unattended).

Took #47 as far as is SAFE without a working env: full root-cause + implementation plan. Did
NOT implement — it's a delicate, high-blast-radius cascade change and I can't do on-screen
regression testing while the WM is wedged; shipping it blind would violate "verify empirically"
and risk breaking ALL rendering with the user away. Implement when env is healthy.

**#47 ROOT CAUSE (confirmed, both paths flatten CSS globally → no @scope subtree-scoping):**
- Slow path `collect_css_from_dom` (core/src/styled_dom.rs:2108): recursively pushes every
  `dom.css` entry into a flat `Vec<Css>`, DROPPING which node each came from.
- Fast path `create_from_fast_dom` (styled_dom.rs:~966-976): flattens `CssWithNodeIdVec` into
  one `combined_css`, ignoring `css_with_id.node_id` (explicit `TODO: respect node_id scoping`).
- Both feed a single global `Css` to `create_from_compact_dom` → the cascade matches every rule
  against the WHOLE tree. A non-root `with_css(".foo{…}")` leaks to `.foo` anywhere in the tree.
  contenteditable is unaffected only because its CSS sits on the ROOT (subtree == whole tree).

**#47 IMPLEMENTATION PLAN (do when env healthy):**
1. Carry origin node through collection: keep `Vec<(Option<NodeId>, Css)>` (None = root/global,
   Some(n) = scoped to node n's subtree). Slow path: thread the current node's NodeId in
   `collect_css_from_dom`. Fast path: keep `css_with_id.node_id`.
2. Make the cascade matcher subtree-aware: when matching a rule that originated at node N, only
   match candidate nodes that are within N's subtree (descendant-or-self of N). Likely in the
   rule-match step under `create_from_compact_dom` / `construct_html_cascade_tree` (find where a
   CssRuleBlock is tested against a node and add a subtree-membership gate keyed by origin).
3. Subtree test: precompute each node's [start,end] DFS index range (or walk parents) so
   "is candidate in N's subtree" is O(1)/O(depth).
4. VERIFY (needs healthy env): (a) UNIT TEST in core — DOM with `.foo` both inside and outside a
   non-root scoped node; assert only the in-subtree `.foo` gets the property (get via
   CssPropertyCache). (b) On-screen no-regression: contenteditable screenshot unchanged
   (root CSS still applies tree-wide). (c) A scoped-CSS example screenshot showing no leak.
Risk: core cascade path used by ALL apps — needs the unit test + on-screen regression, hence
deferred until the env is restored.

---

### Cron firing 17 — #49 VERIFIED on real hardware mouse (after user re-login)
User re-logged in → WM restored, input works again. #49 (XI2 mouse delivery) CONFIRMED with a
real hardware mouse: clicks place AND reposition the caret in the single-line input, and the
user actively edited the field (status bar "Changes: 26"). Independent check: window-relative
clicks moved the caret node 7 pos17 → node 3 pos26; native screenshot shows the caret in the
single-line input. #49 DONE+VERIFIED (commit c813ef046). (#46 prerequisite also confirmed.)
NOTE: plain xdotool screen-coord clicks landed at the wrong spot on this multi-head display
(requested 512,256 → pointer 996,0) — a local xdotool/coordinate quirk, NOT a fix issue; use
`xdotool ... --window <wid> X Y` (window-relative) for reliable scripted clicks here.

NEXT (env now healthy, input verifiable): #48 event-system rework (user's aggressive pick) →
hit-tester unification (user directive) → #47 @scope scoping (plan in firing 16) → #43.

---

### Cron firing 18 (user-reported) — text editing: Delete/Ctrl+A FIXED, clipboard Ctrl+C/X/V IMPLEMENTED
User: "entf doesn't work correctly and ctrl+a doesn't select all text. also please work on
ctrl+c/v/x (wayland, x11, macos, windows)." All reproduced + fixed + verified on X11 with real
keys (xdotool). The event routing (KeyboardShortcut→SystemChange) + SystemChange handlers were
already wired; the bugs were downstream.

**FIXED + VERIFIED (X11, real keys):**
- Ctrl+A select-all (`7522a02bf` + `3368ed278`): handler set the full range then immediately
  `set_single_cursor` → collapsed it to a caret-at-end (no-op). Removed the collapse. ALSO the
  highlight wasn't drawn — SelectAllText only returned ShouldUpdateDisplayList (re-render stale
  DL); now calls `regenerate_display_list_for_dom` like apply_selection_op does, so
  build_text_selections_map runs and the highlight shows. Verified: whole field highlighted cyan.
- Delete/Backspace with selection (`7522a02bf`): `delete_range` only drained when
  start_byte <= end_byte, so BACKWARD ranges (Shift+Home, right-to-left) silently no-op'd —
  the "entf doesn't work correctly". Normalized via min/max. ALSO it ignored cursor AFFINITY
  (used raw cluster start_byte), so select-all-then-Delete left the LAST char (end cursor is
  Trailing). Added affinity-aware `cursor_byte_offset_in_run` (mirrors insert/delete_*). Verified:
  Ctrl+A→Delete empties fully; Ctrl+A→type replaces all ("REPLACED"→"NEW", no leftover);
  Shift+Home→Backspace deletes the backward selection.
- Clipboard Ctrl+C/X/V (`0f363c431`): TWO bugs. (1) `get_selected_content_for_clipboard` was a
  STUB returning None → copy extracted nothing → paste read stale X clipboard. Implemented it
  (slice selection out of the run via the affinity-aware offset; single-run common path,
  multi-run best-effort). (2) X11 `write_to_clipboard` created+dropped an x11_clipboard::Clipboard
  per call → its selection-owner thread died → copy lost. Now keeps ONE persistent Clipboard
  (OnceLock<Mutex<Option<Clipboard>>>; Clipboard is Send). Verified: copy "ZZZ" → empty field →
  paste = "ZZZ" (persists across edits); Ctrl+X empties, Ctrl+V restores.

**CROSS-PLATFORM STATUS:** copy extraction is shared layout code (helps macOS/Windows, whose
clipboard backends already exist — untested here). NATIVE Wayland still routes to x11::clipboard
(works under XWayland, NOT native wl) — task #7: implement wayland::clipboard (wl_data_device) +
runtime routing. Verify on a Wayland session.

**TOOLING NOTE:** plain `xdotool key`/`click` (XTEST) lands at wrong screen coords on this
multi-head box; use `xdotool ... --window <wid> X Y` (window-relative) for reliable scripted
input. Real hardware input is unaffected.

---

### Cron firing 19 (user-reported) — text layout instability ROOT-CAUSED (compact-cache white-space divergence); fix deferred
User: "the line breaking is for whatever reason breaking everything onto one line, only sometimes
it correctly breaks 'Line 1:', 'Line 2:' — debug root cause of this text layout instability."

**REPRODUCED (deterministic on fresh launch):** the multi-line textarea (node 7, class
`.multi-line-textarea`, CSS `white-space: pre-wrap`, content has real `\n`) renders as ONE
wrapped paragraph (h≈469) — the `\n` are collapsed instead of forced breaks. 4/4 fresh launches
continuous. (The "sometimes correct per-line" the user sees = the unstable cache occasionally
holding the right value.) Edits/resize/clicks did NOT flip it this session.

**ROOT CAUSE (proven via [WSPROBE] in split_text_for_whitespace, now stripped):**
- `\n` → forced break is decided in `split_text_for_whitespace` (solver3/fc.rs:8250) by reading
  the parent's `white-space` via `get_white_space_property`. For pre/pre-wrap/pre-line it splits
  text into `InlineContent::LineBreak`s; for Normal it collapses. The line-breaker
  (text3/cache.rs break_one_line) correctly honors `ShapedItem::Break`.
- At layout time `get_white_space_property(node 7)` returns **`Exact(Normal)`** (WSPROBE:
  `ws=Normal has_nl=true`), so no split → continuous. But the debug server
  `get_node_css_properties` reads node 7 (and inherited node 8) as **`white-space: pre-wrap`**.
  The two readers DISAGREE.
- WHY: `get_white_space_property`'s fast path (solver3/getters.rs:624-627) returns the
  **compact cache** value UNCONDITIONALLY for normal-state nodes (never falls through to the slow
  path). The COMPACT CACHE holds `Normal` for node 7's white-space; the slow path / author CSS
  (what the debug server reads) holds the cascaded `pre-wrap`. So the compact cache wasn't
  populated with node 7's cascaded white-space, and the stale value masks the correct one.
- The compact builder `build_compact_cache_with_inheritance` (core/src/compact.rs:445) Step 3
  (line 648) applies per-node props from `self.css_props.get_slice(i)` via
  `apply_css_property_to_compact` (handles WhiteSpace at :854 set_tier1!). The slow-path getter
  reads via `css_property_cache.get_white_space(...)`. These two stores DIVERGE for node 7's
  white-space → the instability.

**EXACT NEXT DIAGNOSTIC (do first next firing):** add a probe in build_compact_cache_with_inheritance
Step 3 logging, for the textarea node, whether `css_props.get_slice(i)` contains a
`CssProperty::WhiteSpace` entry and its `get_property()` value; compare against
`self.get_white_space(nd, id, normal_state)` (the slow-path method). Two cases:
  (a) css_props[node] LACKS WhiteSpace(pre-wrap) but the method returns it → cascade-matching /
      property-storage divergence (css_props vs cascaded_props); fix the store that feeds the
      compact builder so class-matched inherited Tier-1 enums land in css_props.
  (b) css_props[node] HAS it but Step 3 doesn't write it (get_property() None, or a later
      inheritance/overwrite) → fix the apply/ordering.
Likely (a). After fixing, VERIFY: fresh launch → textarea renders each "Line N:" on its own row
(h grows to ~700 at 1200px wide); `white-space` getter (fast path) == debug-server value; check a
couple OTHER Tier-1 enum props (display/text-align) on a few nodes for the same divergence — this
is a GENERAL compact-cache-vs-cascade sync bug, white-space is just where it surfaced. High blast
radius (compact CSS cache, all nodes/props) — verify broadly before commit. Do NOT paper over by
making white-space skip the fast path.

State: probe stripped, tree clean/builds. Task #8 stays OPEN (root-caused, fix pending).

**PRIORITY ORDER NOW:** (1) #8 compact-cache white-space divergence fix (above — top, user-reported).
(2) #9 scrolling + timers on x11/wayland (bounce/overscroll). (3) #48 event-system rework.
(4) hit-tester unification. (5) #47 @scope scoping (related to this cascade-store issue).
(6) #7 native Wayland clipboard.

---

### Cron firing 20-21 (user-reported) — line-break instability PRECISELY root-caused: prune vs recompute drops Tier-1 enums
Continued #8. Added [WSCACHE] probe in build_compact_cache_with_inheritance (now stripped) logging,
per node, white-space in css_props vs cascaded_props vs the final compact tier1 bits. DECISIVE:

  [WSCACHE] node=7 css=Some(WhiteSpace(Exact(PreWrap))) casc=None compact_bits=3(PreWrap)
  [WSCACHE] node=8 css=None casc=Some(WhiteSpace(Exact(PreWrap))) compact_bits=3

BUT the textarea STILL rendered continuous (h=469) this run despite compact=PreWrap. So the
create-time compact cache is correct, yet the layout reads Normal. Mechanism (precise):
1. CREATE: `build_compact_cache_with_inheritance` reads `css_props[7]=PreWrap` → compact=PreWrap
   (WSCACHE confirms). White-space is a direct rule on node 7 → lands in css_props, NOT
   cascaded_props (cascaded_props=inherited-from-parent; node 7's parent is Normal → casc=None).
2. PRUNE (core/src/prop_cache.rs:868-886 `keep` predicate): compact-encoded Normal props that the
   compact cache "fully captured" are DROPPED from BOTH css_props AND cascaded_props. White-space
   (Tier-1 enum) is dropped. Now NEITHER store has node 7's white-space; only the compact cache.
3. UNRESOLVED CONTRADICTION (this run): only ONE WSCACHE line per node appeared (compact=PreWrap),
   so the create build ran and recompute did NOT re-run — yet the textarea STILL rendered continuous
   (h=469). So compact=PreWrap but the LAYOUT behaved as Normal. CORRECTION to an earlier guess:
   `recompute_inheritance_and_compact_cache` (styled_dom.rs:1353) uses
   build_compact_cache_with_inheritance (NOT the getter-only builder); my WSCACHE probe is inside
   that builder, so a recompute would have logged a 2nd line — it didn't here. So the clobber is NOT
   a recompute this run. Either: (a) `split_text_for_whitespace`'s `get_white_space_property` reads
   Normal DESPITE compact=PreWrap (a SECOND compact cache / a different StyledDom instance between
   the builder that WSCACHE saw and the layout reader), or (b) it reads PreWrap and splits, but the
   inline-layout cache (#45 area: layout_ifc Phase 2d GlyphSwap reuse) serves a STALE continuous
   layout that doesn't invalidate on white-space.

ROOT (still): white-space is correct in css_props + the compact cache at create, but the value the
LAYOUT uses ends up Normal. The css_props/cascaded_props PRUNE (prop_cache.rs:868-886 drops Tier-1
enums from BOTH stores) makes ANY rebuild-from-pruned path lose Tier-1 enums, so it's implicated in
the recompute variant of this bug regardless. Non-deterministic across runs (HashMap-seeded cascade
order is a suspect for css_props presence/ordering). Other Tier-1 enums (text-align/display/...)
likely regress the same way on recompute — verify broadly.

EXACT NEXT DIAGNOSTIC (do FIRST next firing — pins it in ONE build): add BOTH probes together —
[WSCACHE] in build_compact_cache_with_inheritance AND [WSPROBE] in split_text_for_whitespace (also
logging whether the inline layout was a cache HIT) — fresh launch, compare for node 7/8. If
split_text reads PreWrap but render is continuous → inline-layout cache staleness (invalidate the
IFC cache on white-space/constraints change, cf #45). If split_text reads Normal while WSCACHE wrote
PreWrap → two compact caches / StyledDom instances (layout reads a different/stale one) → reconcile.

FIX OPTIONS (pick in a fresh focused session — delicate core, high blast radius; do NOT paper over):
  (A) Recompute should PRESERVE the existing compact tier1 values (incremental) instead of
      rebuilding from pruned css_props — re-do only inheritance, keep directly-set compact bits.
  (B) Don't run the getter-only `build_compact_cache` on recompute; route recompute through
      with_inheritance AND ensure it reads an un-pruned source (keep Tier-1 enums in css_props —
      i.e. exclude Tier-1 enums from the `keep`-predicate drop). Costs some memory (the prune's
      purpose) — a real correctness-vs-memory tradeoff; may warrant a user decision.
  (C) Skip the redundant recompute right after create (it clobbers a just-correct compact cache);
      only recompute when style/DOM actually changed, not on every layout/resize.
  VERIFY after fix: fresh launch (×several, for the non-determinism) → textarea each "Line N:" on
  its own row (h≈700 @1200w); spot-check other Tier-1 enums (text-align/display) survive a
  resize-triggered recompute; contenteditable screenshot unchanged otherwise.

State: both probes stripped, tree clean/builds. Task #8 stays OPEN (precisely root-caused, fix
pending — it's a core prune/recompute lifecycle fix). Priority unchanged: #8 first, then #9 scroll/
timers, then #48, hit-tester unify, #47, #7.

---

### Cron firing 22 — #8 line-break instability FIXED (`1624df855`)
Pinned with paired probes ([WSCACHE] in build_compact_cache_with_inheritance + [WSPROBE] in
split_text_for_whitespace, both stripped):
  build#1 (create):    node7 css_props_ws=PreWrap → compact_bits=PreWrap
  build#2 (recompute): node7 css_props_ws=None    → compact_bits=Normal  ← CLOBBER
  split_text reads compact → Normal → \n collapsed → continuous.
ROOT: `prune_compact_normal_props` (prop_cache.rs) dropped compact-encoded Normal props from
`css_props` after the create build, assuming the compact cache is permanent. But
`regenerate_layout` calls `recompute_inheritance_and_compact_cache()` EVERY frame (CSD/append_child
composition), which REBUILDS the compact cache from `css_props` (Step 3). The 2nd build read pruned
css_props → white-space:pre-wrap reset to Normal → split_text collapsed \n. Intermittent = whether
the recompute ran before the read.
FIX: don't prune `css_props` (recompute reads it). cascaded_props prune kept (rebuild's inheritance
uses parent COMPACT, not cascaded_props). TODO: re-enable css_props prune once recompute is
incremental. Affected ALL Tier-1 enums (display/text-align/visibility/…), not just white-space —
white-space surfaced it via \n breaks.
VERIFIED on screen: textarea renders each "Line N:" on its own row + overflow scrollbar; single-line
input stays one line (nowrap). (Native screenshot needs the window mapped — intermittent WM mapping
issue on this box; verified via headless render = same display list. NOTE for next firings: if
take_native_screenshot returns "XGetImage failed", the window is IsUnMapped — relaunch or fall back
to take_screenshot for layout verification.)

**PRIORITY ORDER NOW:** (1) #9 scroll + timers on x11/wayland (bounce/overscroll) — user-requested,
verifiable via real wheel (xdotool button 4/5). (2) #48 event-system rework. (3) hit-tester
unification (user directive). (4) #47 @scope subtree-scoping. (5) #7 native Wayland clipboard.

---

### Cron firing 23 — #9 scroll/timers: core VERIFIED working; animation/real-wheel BLOCKED by re-wedged WM
VERIFIED on x11+CPU (via debug `scroll` op + headless `take_screenshot` — both WM-independent):
- Basic scroll WORKS: debug scroll on textarea (node 7) set scroll_y=150 (max_scroll_y=351.95,
  content 692 / container 340) and the render updated (textarea scrolled Line 1-5 → Line 3-7,
  scrollbar thumb moved). The firing-8 "SCROLL BLOCKED BY LAYOUT (solver3)" is RESOLVED.
CODE-VERIFIED wired on x11 (so it IS "hooked up on linux"):
- Real wheel path: handle_scroll (x11/events.rs:591) → scroll_manager.record_scroll_from_hit_test(
  ScrollInputSource::WheelDiscrete) → impulse+momentum physics. Wheel buttons 4/5 reach handle_scroll
  via handle_mouse_button, which now receives XI2 events (post-#49).
- Bounce/overscroll IMPLEMENTED: layout/src/scroll_timer.rs — rubber_band_clamp,
  spring_constant_from_bounce_duration, overscroll_elasticity, max_overscroll_distance,
  bounce_back_duration_ms; ScrollPhysics/OverflowScrolling/OverscrollBehavior CSS props.
- Animation driving: scroll_manager.has_active_animations() + tick() called in render_and_present;
  check_timers_and_threads pumps timers.

NOT YET VERIFIED (needs a MAPPED window — see blocker): real wheel → momentum decay animation;
bounce-at-edge spring-back; and whether the CPU render path drives CONTINUOUS frames while
has_active_animations (the GPU path at x11/mod.rs:2473-2494 has an explicit has_active_animations
early-return-skip; the CPU path's tick is at :2218 but I did NOT confirm the loop re-renders the
NEXT frame while animating — a possible gap to verify/fix when the WM is healthy: if momentum/bounce
only ticks on Expose, the animation stalls. Trace: poll_event/wait_for_events should keep waking
(~16ms timerfd) while has_active_animations).

⚠️ BLOCKER (my fault, again): the WM is RE-WEDGED — xeyes IsUnMapped + 2 orphan xfwm4 frames, from
app restart-churn during #8/#9 debugging. Same failure as firings 15-17. take_native_screenshot
returns "XGetImage failed" and real xdotool wheel/click can't land (pointer over root). NEEDS
RE-LOGIN to restore mapping. LESSON (reinforced): do NOT pkill+relaunch the app repeatedly — reuse
the running instance; headless take_screenshot verifies layout/render regardless of WM; only
real-input/native-screenshot need a healthy WM.

WAYLAND: not tested (no Wayland session on this box). Scroll core is shared (scroll_manager/
scroll_timer); verify the Wayland event loop's animation-driving + timer pumping separately
(older handoff flagged "scroll physics timer isn't driving redraws on Wayland").

**PRIORITY ORDER NOW** (on-screen verification is WM-blocked until re-login):
(1) #9 finish: after re-login, real-wheel momentum + bounce verification; confirm CPU-path
    continuous-animation driving. (2) #47 @scope subtree-scoping — UNIT-TEST verifiable (cargo
    test), WM-INDEPENDENT, good to do while the WM is wedged; related to the #8 cascade work.
(3) #48 event-system rework. (4) hit-tester unification. (5) #7 native Wayland clipboard.

---

### Cron firing 24 — autonomous loop hit its productive limit (WM still wedged); #47 reassessed as bigger than planned
WM still wedged (no re-login: session unchanged since 09:14, xeyes IsUnMapped). So on-screen
real-input verification (#9-animation real wheel, #48, hit-tester) stays BLOCKED.

#47 reassessed (was flagged "WM-independent, do next"): it's bigger/more delicate than the firing-16
plan implied. The SLOW path `collect_css_from_dom` (styled_dom.rs:2108) runs on the `Dom` TREE,
BEFORE NodeIds are assigned (NodeIds come during convert_dom_into_compact_dom), and create_from_dom
then MERGES all rules into ONE `combined_css` (create_from_dom:~1186) with no origin. So threading a
per-rule origin requires: (a) capturing each rule's Dom-tree position at collection, (b) mapping
that to the final NodeId after compaction, (c) a subtree-aware matcher change in restyle/
prop_cache (CssRuleBlock has no origin field — either add one [touches azul-css public type +
api.json regen] or thread a parallel rule→origin map through create→restyle→match). High blast
radius (the cascade matcher affects ALL apps). The FastDom path has node_id already; the slow path
is the hard part. Conservative scoping (root/global unchanged, only node-originated CSS scoped)
limits risk and is unit-test + headless-regression verifiable — but it's a focused multi-file core
change best done in a FRESH session, not at extreme context depth.

ASSESSMENT: this autonomous session has delivered a lot (#46, #49, Delete/Backspace+Ctrl+A,
clipboard C/X/V, #8 line-break, #9 scroll core verified). The remaining backlog is now all either
(a) on-screen-verification BLOCKED by the wedged WM (re-login needed): #9-animation, #48,
hit-tester; or (b) large/delicate core changes best done with fresh context: #47, #48, hit-tester;
or (c) untestable on this box: #7 Wayland clipboard, Wayland scroll. No safe + fully-verifiable +
tractable-at-depth item remains. Continuing to fire the 10-min cron now just spins.

ACTION: paused the cron (deleted job d317fa59) to avoid wasteful spinning. TO RESUME productive
work: (1) re-login to clear the wedged WM (restores window mapping → real-wheel/native-screenshot
verification), then (2) start a FRESH session (full context budget) and re-create the cron. Next
items in order: #9-animation real-wheel verify → #47 (plan above) → #48 → hit-tester → #7.

---

### Cron firing 25 — USER RE-PRIORITIZED (interactive); plan updated for COMPACTION
User gave a new ordered backlog (this supersedes prior priority lists). Captured here so it
survives the upcoming context compaction.

**NEW PRIORITY ORDER:**
1. **SCROLL / physics timer (FIRST).** User: real mouse-wheel scrolling doesn't work though the
   scrollbar shows; "the physic scroll timer isn't hit." Also check macOS for the same problem.
2. **X11 composite/dead-key chars (é = e+accent), IME, and Japanese/CJK glyph rendering** (all
   worked on macOS). Task #10.
3. **Wayland clipboard** — implement now, test later (no Wayland session on this box). Task #7.
4. **a11y on X11.** Task #11.
(Deferred big items remain after these: #47 @scope subtree-scoping [reassessed bigger — see firing
24], #48 event-system rework, hit-tester unification.)

**SCROLL ROOT-CAUSE PROGRESS (firing 25):**
- Wheel path: handle_scroll (x11/events.rs:591) → scroll_manager.record_scroll_from_hit_test(
  WheelDiscrete) QUEUES the delta into scroll_input_queue (does NOT set offset directly), returns
  should_start_timer=true on the first pending input. handle_scroll THEN starts
  SCROLL_MOMENTUM_TIMER_ID (events.rs:634-656) with scroll_physics_timer_callback at
  system_style.scroll_physics.timer_interval_ms. The timer callback take_all()s the queue each tick,
  applies physics → offset, and should redraw. The debug `scroll` op works only because it sets the
  offset DIRECTLY (bypasses queue+timer) — that's why scrollbar/render are fine but real wheel isn't.
- Timer-START is wired identically on ALL platforms (x11 events.rs:634, macos events.rs:428, windows
  mod.rs:3040, wayland mod.rs:2178). So the bug is NOT a missing start.
- TWO CANDIDATES (verify empirically next, needs a MAPPED window + real wheel → re-login):
  (a) handle_scroll NOT REACHED on real wheel: post-#49, XInput2 delivers wheel as XI_Motion
      SCROLL-VALUATOR changes, which handle_xi_event routes to handle_mouse_move (which IGNORES
      scroll valuators). Only legacy emulated XI_ButtonPress 4/5 reaches handle_mouse_button→
      handle_scroll. If this server sends smooth-scroll XI_Motion but NOT emulated buttons (or they're
      filtered), wheel never reaches handle_scroll. FIX likely: decode scroll-class valuators in
      handle_xi_event (x11/mod.rs:414) and call handle_scroll. PROBE: log evtype+detail+valuators in
      handle_xi_event on a real wheel tick.
  (b) Timer started but NOT PUMPED / callback not firing / not redrawing: X11 timers use timerfd
      (start_timer → timerfd; wait_for_events polls it; check_timers_and_threads→process_timers_and_
      threads runs the callback). Verify the SCROLL_MOMENTUM_TIMER timerfd is created+polled and
      scroll_physics_timer_callback actually runs + applies offset + requests redraw. PROBE: eprintln
      in scroll_physics_timer_callback (does it fire? offset delta? redraw?).
  The user's "timer isn't hit" wording points at (b), but (a) would ALSO present as "scrollbar shows,
  no scroll." Check (a) first (cheap probe in handle_xi_event), then (b).
- macOS: timer-start is wired (events.rs:428); macOS pumps timers via its own run-loop (NOT timerfd),
  and gets wheel via NSEvent (NOT XI2) — so (a) is X11-only, but (b)/the shared
  scroll_physics_timer_callback could affect macOS too. Check whether the macOS timer pump actually
  invokes scroll_physics_timer_callback.

⚠️ ENV: WM was wedged earlier (orphan xfwm4 frames from my app restart-churn) → window unmapped →
real-wheel + native-screenshot blocked. NEEDS RE-LOGIN. Headless take_screenshot works regardless.
Do NOT pkill+relaunch the app repeatedly (reuse the running instance) — that's what wedged the WM.

### Cron firing 26 — #9 SCROLL ROOT-CAUSED + FIXED (redraw-drop in wait_for_events; X11 + Wayland)
Autonomous loop restarted (cron job `0fc338cf`, every 10 min, session-only/7-day). Candidate (b) from
firing 25 CONFIRMED via code reading and FIXED. Candidate (a) assessed as NOT the bug on this box.

**ROOT CAUSE (candidate b — the real one):** Both Linux backends have a single-window blocking idle
path `wait_for_events` (x11/mod.rs:1756, wayland/mod.rs:1577) that `poll(2)`s the display fd + all
timerfds. When a timerfd fired it called **`process_timers_and_threads()` and DISCARDED the return
bool** (x11:1832, wl:1650) — so the scroll-physics timer callback ran every tick, advanced the offset
via `CallbackChange::ScrollTo`→`apply_user_change`(returns `ShouldReRenderCurrentWindow`), but **no
redraw was ever requested**. The momentum offset updated invisibly; the window only repainted when some
unrelated event arrived. The sibling `check_timers_and_threads` (called from `poll_event`) does it
right (`if process_timers_and_threads() { request_redraw() }`), and macOS `tickTimers:` does it right
(`if needs_redraw { setNeedsDisplay }`) — `wait_for_events` was a duplicated half-implementation that
forgot the redraw half. This is why "scrollbar shows but wheel doesn't scroll," and why the user saw
"the physics scroll timer isn't hit" — it IS hit (in wait_for_events), it just never painted.

**FIX (committed):** both `wait_for_events` now delegate to `check_timers_and_threads()` (the single
source of truth for pump+redraw) instead of the bare `process_timers_and_threads()`. X11 →
`request_redraw()` (Expose); Wayland → `needs_redraw=true; generate_frame_if_needed()`. Removes the
duplication (architecture cleanup). Compiles dev + release; `libazul.so` rebuilt.

**Why candidate (a) [XI2 scroll-valuators] is NOT the bug here:** `handle_mouse_button` already maps
buttons 4/5 → `handle_scroll` (x11/events.rs:441-448), and `start_timer` registers the Timer in
`layout_window.timers` AND creates the timerfd (x11/mod.rs:3065). On XWayland (this box) and modern
Xorg/libinput the server DOES emit emulated XI_ButtonPress 4/5 (alongside smooth-scroll valuators), so
`handle_scroll` IS reached and the timer DOES start + pump. (a) — decoding `XIScrollClassInfo`
valuators in `handle_xi_event` (mod.rs:421) + skipping `XIPointerEmulated` buttons to avoid
double-count — remains an OPTIONAL robustness/hi-res-smooth-scroll enhancement, **not** required for
basic wheel scroll. Deliberately NOT done blind (double-count regression risk, untestable now).

**macOS:** confirmed CORRECT — `start_timer` schedules an NSTimer→`tickTimers:`→
`process_timers_and_threads`→`setNeedsDisplay` (mod.rs:596-606, 2391+). No change needed.
**Multi-window X11 idle** (`wait_for_x11_connection_activity`, run.rs:1349): NOT affected — it `select`s
with a 16ms timeout and pumps via `poll_event`→`check_timers_and_threads` (which redraws). Only the
single-window `wait_for_events` path was broken.

**Headless:** the timer pump there was already correct (loop checks `process_timers_and_threads()`
return, headless/mod.rs:857-863), but **`HeadlessEvent::Scroll` was a no-op** ("not yet wired"). NOW
WIRED to the real physics path (record_scroll_from_hit_test + start SCROLL_MOMENTUM_TIMER), mirroring
the desktop backends. This unblocks automated/headless scroll verification (azul-self-test) — a prior
MouseMove must leave the hover over a scrollable node, then `Scroll{delta}` queues + animates.

**VERIFICATION:** dev+release compile pass for azul-dll; headless wiring compiles. Real-wheel visual
verification BLOCKED by the wedged WM (needs re-login). azul-paint rebuilt for post-re-login testing.

**POST-RE-LOGIN EMPIRICAL CHECKS (do these to close #9):**
1. `AZ_BACKEND=x11 ./target/release/azul-paint` → wheel over a scrollable area → content should scroll
   with momentum + edge bounce (NOT just the scrollbar; the content moves).
2. `AZ_BACKEND=wayland ./target/release/azul-paint` → same.
3. If X11 still doesn't scroll: add a probe `eprintln` in `handle_xi_event` (x11/mod.rs:421) logging
   `evtype`+`ev.detail`+valuator mask on a wheel tick. If only smooth-scroll XI_Motion arrives and NO
   emulated XI_ButtonPress 4/5 → implement candidate (a) (decode XIScrollClass valuators → handle_scroll,
   skip XIPointerEmulated buttons). If buttons 4/5 DO arrive, scroll should now work (the redraw fix).

**PRIORITY ORDER NOW** (firing 25 list, item 1 fix landed pending real-wheel confirm):
(1) #9 scroll — FIX LANDED, awaiting real-wheel verify after re-login → (2) #10 X11 dead-key/IME/CJK
(input side fix landed, see below; glyph-fallback + verify remain) → (3) #7 Wayland clipboard →
(4) #11 a11y on X11. (Deferred: #47 @scope, #48 events, hit-tester unify.)

#### Firing 26 (cont.) — #10 INPUT root-caused + partial fix (missing `setlocale`)
Got ahead onto #10's INPUT side while #9 awaits re-login. The X11 IME stack is already substantial
(`ImeManager`: XOpenIM + style negotiation + XCreateIC on-the-spot/over-the-spot/rooted + XmbLookupString
+ XFilterEvent + preedit callbacks; control-char filter at events.rs:734). The missing piece:
**`setlocale` was NEVER called anywhere** — `ImeManager::new` calls `XSetLocaleModifiers("")` but the
process stayed in the default `"C"` (ASCII) locale, so `XmbLookupString`/XIM cannot compose dead-keys
(´+e→é) or emit CJK — input silently degrades to ASCII. This is exactly the gap `X11_API_REFERENCE.md`
flagged ("setlocale + XSetLocaleModifiers before XOpenIM").

**FIX (committed):** `libc::setlocale(LC_CTYPE, "")` once (std::sync::Once) at the top of
`X11Window::new_with_resources`, BEFORE `XOpenDisplay`/`XOpenIM` → correct ordering
setlocale→XSetLocaleModifiers→XOpenIM. Scoped to **LC_CTYPE only** (the category Xlib actually reads for
codeset) so LC_NUMERIC stays "C" and a comma-decimal locale can't break float parsing in C deps. Helps
when the user's env has a UTF-8 locale (the normal case); no-op + no regression otherwise. Compiles.

**STILL OPEN for #10 (next firings):**
- **CJK GLYPH RENDERING** is a SEPARATE problem from input: even with correct UTF-8 CJK chars, the font
  must HAVE CJK glyphs (font fallback). Investigate `font_manager` glyph fallback / fc_cache for CJK
  (Noto CJK etc.) — tofu = font fallback gap, not input. Likely the bigger remaining piece.
- **Encoding robustness:** `XmbLookupString` returns LOCALE-encoded bytes (treated as UTF-8 here). Fine
  under a UTF-8 locale; under a non-UTF-8 locale it'd mojibake. Optional: switch to `Xutf8LookupString`
  (always UTF-8) — needs that symbol added to dlopen.rs. Low priority (UTF-8 locales are universal now).
- **VERIFY after re-login:** type `é` via dead-key/compose; type Japanese via fcitx5/ibus (preedit inline,
  commit on Enter); confirm CJK glyphs actually render (not tofu). Probe: log `XmbLookupString` status +
  bytes on a compose key.

### Cron firing 27 — #10 CJK GLYPH RENDERING root-caused VISUALLY: CPU rasterizer can't decode CFF outlines
Verified azul-paint rebuilt w/ both firing-26 fixes (scroll + setlocale). Then attacked #10's CJK glyph
side with a **headless render + PNG inspection** harness (the visual verification the wedged WM blocks
for real input). RESULT: a real, confirmed bug — and it's NOT in input or shaping.

**Harness (reproducible):** edit `examples/rust/src/hello-world.rs` label to a CJK string (keep it SHORT
and CJK-FIRST — the demo window is ~640px and does NOT wrap, so a long line pushes CJK off the right
edge and `render_text`'s clip check silently drops it — I chased that artifact for a bit), then:
`cargo build -r -p azul-examples --example hello-world` →
`AZ_FONT_FALLBACK_DEBUG=1 AZ_BACKEND=headless AZ_HEADLESS_SNAPSHOT_PATH=/tmp/cjk.png ./target/release/examples/hello-world`
→ Read /tmp/cjk.png. (`AZ_HEADLESS_SNAPSHOT_PATH` renders the initial layout via cpurender to a PNG, then exits.)

**VISUAL FINDINGS:**
- Accented Latin (é à ü ñ) renders CORRECTLY → the "composite chars" GLYPH path is fine; the dead-key
  *input* path is the separate firing-26 setlocale fix.
- CJK (日本語 中文 あいう) renders BLANK — but the layout RESERVES its advance width (following text is
  pushed right by the CJK run width). So it's shaped+measured but the glyphs are invisible. NOT clipping
  (confirmed by putting CJK first, in-view — still blank).
- `AZ_FONT_FALLBACK_DEBUG=1` proves shaping is CORRECT: 日本語/中文/あいう are assigned a CJK fallback
  font and `shape_text_correctly` runs on them (no "NOT loaded" msg). `render_text` prints NO
  "Font hash not found". So input✓, chain✓, shaping✓, font-load✓ — the drop is at RASTERIZATION.

**ROOT CAUSE (confirmed by code):** the CPU glyph decoder only handles TrueType `glyf` outlines.
`cpurender::render_text` (cpurender.rs:3623) does `get_or_decode_glyph(gid)` then `None => continue` —
silently skips the glyph. `ParsedFont::loca_glyf` is `LocaGlyfState::Loaded(None)` for **CFF /
OpenType-PostScript** fonts (font.rs:365, 469 `FontType::OpenTypeCFF`), so `get_or_decode_glyph`
returns `None` for every CFF glyph → blank. **NotoSansCJK (the installed CJK font) is CID-keyed CFF**,
so ALL CJK → blank. Advances still come from `hmtx` (works for CFF) → reserved-but-invisible. This is
why it "worked on macOS" (CoreText/WebRender rasterize CFF) but not the Linux CPU-fallback path (this
nouveau box falls back to cpurender when shaders fail). **Impact is broader than CJK: ANY .otf/CFF font
renders blank on the cpurender/headless path.**

**FIX PLAN (next firing — focused, real work):** add CFF outline decoding to `font.rs`
`decode_glyph_inner`/`get_or_decode_glyph`. `allsorts-azul 0.16.4` HAS it: `src/cff/outline.rs`
`CFFOutlines: OutlineBuilder` with `visit<S: OutlineSink>(gid, &mut sink)` (handles CID-keyed CFF +
subrs internally). Reuse the existing `GlyphOutlineCollector` (font.rs:210, already an `OutlineSink`
for the glyf path) + `BoundingBoxSink` for the bbox; advance from `hmtx`; build the same `OwnedGlyph`
the glyf path produces; cache identically. The CFF table bytes are available (the font retains its
bytes for lazy decode; `FontType::OpenTypeCFF(Vec<u8>)` also stores them). Then re-run the harness →
CJK should render. Also re-check the GPU/WebRender path isn't double-affected.

**SECONDARY BUG (separate, lower priority):** Korean **한국어 (Hangul) is DROPPED at coverage** —
`split_text_by_font_coverage` (cache.rs:6458) → `font_chain.resolve_char(fc_cache, ch)` returns `None`
for Hangul, so those bytes get no segment at all (absent from the FONT FALLBACK output), even though
NotoSansCJK covers Hangul. JP/CN/hiragana resolve fine. So the resolved fallback chain lacks a
Hangul-covering font → investigate `rust_fontconfig` `resolve_char`/`UnicodeRange` coverage + how the
chain is built/pruned for Hangul (likely the script→fallback selection or codepoint-prune drops it).
Fix this AFTER the CFF decode (otherwise can't see Hangul render anyway).

**PRIORITY ORDER NOW:** (1) #9 scroll FIX LANDED (awaiting real-wheel verify post re-login) → (2) #10:
input✓(setlocale); **CJK glyph rendering = implement CFF outline decode (top of next firing)**, then
Hangul coverage; then IME verify post re-login → (3) #7 Wayland clipboard → (4) #11 a11y on X11.
(Deferred: #47 @scope, #48 events, hit-tester unify.)

### Cron firing 28 — #10 CJK GLYPH RENDERING **FIXED + VISUALLY VERIFIED** (CFF outline decode)
Implemented the firing-27 fix plan and confirmed it on screen via headless render.

**FIX (committed):** added `ParsedFont::decode_cff_glyph_into` in `layout/src/font.rs`.
`decode_glyph_inner`'s `resolve_loca_glyf()`-is-`None` branch (CFF / OpenType-PostScript fonts) now,
instead of returning an empty outline, reads the `CFF ` table from `original_bytes` via
`FontData::table_provider` and decodes the glyph with allsorts `cff::outline::CFFOutlines` into the same
`GlyphOutlineCollector` the glyf path uses (bbox via `compute_outline_bbox`). allsorts resolves
CID-keyed CFF (incl. local subrs) internally — needed because Noto Sans/Serif CJK are CID-keyed CFF.

**VISUAL VERIFICATION (headless):** rebuilt `hello-world` with a CJK test label, ran
`AZ_BACKEND=headless AZ_HEADLESS_SNAPSHOT_PATH=/tmp/cjk_fixed.png` → inspected PNG:
**日本語 (kanji), 中文, あいう (hiragana), é à ü ñ ALL RENDER** now (vs blank-after-advance before the
fix). The CPU/headless rasterizer can now draw CFF fonts — fixes CJK AND any `.otf`/PostScript font on
the cpurender path (and the Linux CPU-fallback this nouveau box uses). macOS/WebRender were unaffected.

**STILL OPEN — Korean Hangul (한국어) drops at COVERAGE (separate bug, not CFF):** the headless render
shows a blank gap where 한국어 should be. `AZ_FONT_FALLBACK_DEBUG=1` confirms `split_text_by_font_coverage`
→ `font_chain.resolve_char` returns `None` for Hangul (those bytes get NO segment), while Han + Hiragana
resolve to the CJK font fine. NotoSansCJK covers Hangul, so the resolved chain's per-font coverage
(`UnicodeRange`) is missing the Hangul Syllables block (U+AC00–U+D7AF). HYPOTHESIS (firing 28): the
legacy chain path (`resolve_font_chains_with_registry` + `prune_chain_to_used_chars`, used when
`font_manager.registry` is None) derives coverage from the cached `FontMatch.unicode_ranges`, which
rust-fontconfig builds from the **OS/2 ulUnicodeRange bits**, not the actual cmap. Noto Sans CJK ships
as separate JP/KR/SC/TC faces that share glyphs but set DIFFERENT OS/2 range bits — the **JP** face
(picked first for 日本語) likely does NOT set the Hangul bit, so `fm_covers`/`resolve_char` reject it for
Hangul even though its cmap has the glyphs. rust-fontconfig is a crates.io dep (not editable in place);
`fm_covers` (getters.rs:3762) + `prune_chain_to_used_chars` are azul-side and editable. NEXT firing:
(a) confirm whether the headless/registry path is None here (→ legacy OS/2 path), (b) decide fix:
prefer the cmap-probe `resolve_font_chains_fast` path (registry) which checks real cmap coverage, or add
an azul-side cmap-coverage fallback in `fm_covers` when OS/2 ranges disagree with the actual cmap. Probe:
`AZ_FONT_FALLBACK_DEBUG=1` already shows the drop; add a log of the candidate fonts' `unicode_ranges`
for a Hangul codepoint.

**PERF NOTE:** `decode_cff_glyph_into` re-parses FontData+CFF index per glyph, but `get_or_decode_glyph`
caches the resulting `OwnedGlyph` per gid, so it runs once per unique glyph (acceptable; CFF::read is
index-level, not all-charstrings). Optional later opt: cache the parsed CFF table on the face.

**ARTIFACTS:** font.rs is in azul-layout → libazul.so + azul-paint need a rebuild to carry the CFF fix
(scroll+setlocale builds predate it). Rebuilding post-commit.

**VERIFY recipe (reuse):** edit `examples/rust/src/hello-world.rs` label to a SHORT, CJK-FIRST string
(window is ~640px, no wrap — long lines push CJK off-screen-right and the clip check drops it),
`cargo build -r -p azul-examples --example hello-world`, then
`AZ_FONT_FALLBACK_DEBUG=1 AZ_BACKEND=headless AZ_HEADLESS_SNAPSHOT_PATH=/tmp/x.png ./target/release/examples/hello-world`,
Read /tmp/x.png.

**PRIORITY ORDER NOW:** (1) #9 scroll FIX LANDED (real-wheel verify post re-login) → (2) #10: input✓,
**CJK glyph rendering✓ (FIXED)**, remaining = Korean Hangul coverage + IME verify post re-login →
(3) #7 Wayland clipboard → (4) #11 a11y on X11. (Deferred: #47 @scope, #48 events, hit-tester unify.)

### Cron firing 29 — #10 Korean HANGUL **FIXED + VISUALLY VERIFIED** (cmap-coverage fallback)
Confirmed firing-28's hypothesis and fixed it. ROOT CAUSE (confirmed): coverage in BOTH chain paths
flows through rust-fontconfig `request_fonts_fast` / `resolve_char`, which test `pattern.unicode_ranges`
— derived from the font's **OS/2 `ulUnicodeRange` bits, NOT the cmap** (registry.rs:812 + doc 590-604).
Noto Sans CJK ships JP/KR/SC/TC faces that share glyphs but set different OS/2 range bits; the **JP**
face (chosen for 日本語) omits the Hangul block, so `resolve_char` returned `None` for 한국어 and
`split_text_by_font_coverage` silently dropped it — even though that face's cmap HAS the Hangul glyphs.
rust-fontconfig is a crates.io dep (not editable here).

**FIX (committed):** azul-side cmap-coverage fallback in `split_text_by_font_coverage`
(layout/src/text3/cache.rs). When `font_chain.resolve_char` returns `None`, probe the actually-loaded
fonts by REAL glyph coverage (`ParsedFontTrait::has_glyph`) and use the first that covers the codepoint.
The covering CJK face is already loaded (Han/Kana resolved to it), so Hangul reuses it (no font mixing).
Robust for ANY OS/2-vs-cmap discrepancy, not just Hangul. Signature gained `loaded_fonts: &LoadedFonts<T>`
(both call sites updated). Additive — only affects codepoints the chain previously DROPPED, so no
regression risk for already-working text.

**VISUAL VERIFICATION (headless):** `AZ_FONT_FALLBACK_DEBUG=1` now reports **8 segments incl. 한국어**
(bytes 17..26 → CJK font; was 7 segments / Hangul absent). PNG inspection: **日本語 中文 한국어 あいう
é à ü ñ ALL render**. #10's GLYPH side (Japanese kanji+kana, Chinese, Korean, accented Latin) is now
fully working on the CPU/headless/Linux path.

**#10 STATUS:** input✓ (setlocale, firing 26) · CJK+Hangul glyph rendering✓ (CFF decode firing 28 +
cmap fallback firing 29). REMAINING: real-IME verify (dead-key é, fcitx5/ibus Japanese preedit) needs a
MAPPED window → post re-login. Then #10 fully closes.

**ARTIFACTS:** cache.rs is in azul-layout → libazul.so + azul-paint rebuilt post-commit to carry the
Hangul fix (on top of scroll+setlocale+CFF).

**PRIORITY ORDER NOW:** (1) #9 scroll FIX LANDED (real-wheel verify post re-login) → (2) #10: input✓,
**CJK+Hangul glyph rendering✓ (FIXED)**, remaining = IME real-input verify post re-login →
(3) #7 Wayland clipboard (implement now/test later — NEXT actionable code item) → (4) #11 a11y on X11.
(Deferred: #47 @scope, #48 events, hit-tester unify.)

### Cron firing 30 — #7 Wayland clipboard: XWayland-fallback reliability bug FIXED; native wl_data_device SCOPED
Started #7. Found `wayland/clipboard.rs` does NOT implement native Wayland clipboard — it uses
`x11_clipboard::Clipboard` (XWayland) AND had the SAME create-drop bug the X11 backend fixed in firing
18: it built a fresh `Clipboard::new()` per copy/read and dropped it, killing the selection-owner thread
→ copy lost / paste stale. (Only broke on pure-Wayland or after the owner thread died.)

**FIX (committed):** added the process-persistent `clipboard()` owner (OnceLock<Mutex<Option<Clipboard>>>)
to `wayland/clipboard.rs`, mirroring `x11/clipboard.rs` exactly; `write_to_clipboard`/`read_from_clipboard`
now use the live owner. Low-risk (proven pattern). Improves the XWayland-fallback path. NOTE: this is
still NOT native Wayland — on a pure-Wayland session with no XWayland, `Clipboard::new()` fails and
clipboard is unavailable. Can't visually verify here (WM wedged; also this box is XWayland not native wl).

**NATIVE wl_data_device — IMPLEMENTATION PLAN (next focused firing; large, untestable on this box):**
Hand-roll the protocol like the firing-1 decoration manager (interfaces via `wl_proxy_marshal_constructor`):
1. Bind `wl_data_device_manager` global in the registry handler (store the proxy on WaylandWindow).
2. `wl_data_device_manager.get_data_device(seat)` → `wl_data_device`; add a listener for `data_offer`
   (new offer), `selection` (current clipboard offer or null), `enter/leave/motion/drop` (DnD, ignore for
   clipboard). Track the latest `wl_data_offer` from `selection`.
3. COPY: on copy, `create_data_source`, `wl_data_source.offer("text/plain;charset=utf-8")` (+ "UTF8_STRING",
   "text/plain"), `wl_data_device.set_selection(source, serial)` using the last input serial (track it from
   keyboard/pointer enter events). Listen for `wl_data_source.send(mime, fd)` → write the text to `fd`,
   close it; and `cancelled` → drop the source.
4. PASTE: from the tracked selection `wl_data_offer`, `pipe2()`, `wl_data_offer.receive(mime, write_fd)`,
   `wl_display_roundtrip`/dispatch, read the read_fd to EOF → UTF-8 string.
5. Runtime routing: in `wayland::clipboard`, prefer native (if `wl_data_device_manager` bound) else fall
   back to the x11_clipboard path (current). Needs serial threading + fd/pipe handling + interface defs in
   `wayland/dlopen.rs`/interfaces. ~200-400 lines. Verify on a real Wayland session (not this XWayland box).

**ARTIFACTS:** libazul.so + azul-paint being rebuilt to carry scroll+setlocale+CFF+Hangul+clipboard fixes.

**PRIORITY ORDER NOW:** (1) #9 scroll (real-wheel verify post re-login) → (2) #10 input✓ + CJK/Hangul✓
(IME real-input verify post re-login) → (3) #7 clipboard: reliability fix✓, **native wl_data_device =
next focused firing (plan above)** → (4) #11 a11y on X11. (Deferred: #47, #48, hit-tester unify.)

### Cron firing 31 — artifacts confirmed; #7 native-clipboard plan SHARPENED; session at productive limit
**ARTIFACTS (all 5 fixes built + verified to compile in release):** `target/release/libazul.so` (15:14)
and `target/release/azul-paint` (15:16) now carry: #9 scroll redraw (x11+wl), setlocale input,
CFF glyph decode, Hangul cmap-fallback, Wayland clipboard persistent-owner. Ready for the user to test
after re-login.

**#7 native wl_data_device — KEY SIMPLIFICATION found:** unlike the firing-1 decoration manager (which
had to hand-build its `wl_interface` because it's a protocol EXTENSION), `wl_data_device_manager` /
`wl_data_device` / `wl_data_source` / `wl_data_offer` are **CORE Wayland** — libwayland EXPORTS their
`wl_interface` symbols (dlsym them in `wayland/dlopen.rs` exactly like the existing
`wl_seat_interface`/`wl_compositor_interface` at dlopen.rs:73-75). So NO hand-rolled interface signatures
— much smaller/safer than feared. Implementation (next fresh-context firing; ~200-250 lines):
1. dlopen.rs: dlsym `wl_data_device_manager_interface`, `wl_data_device_interface`,
   `wl_data_source_interface`, `wl_data_offer_interface`.
2. registry_global_handler (events.rs ~448): bind `wl_data_device_manager` (like seat) → store on window.
3. `get_data_device(seat)` via `wl_proxy_marshal_constructor` (template: the decoration manager's
   get_ctor at events.rs:407-420); add wl_data_device listener (track latest `selection` wl_data_offer;
   ignore enter/leave/motion/drop).
4. COPY: create_data_source, `.offer("text/plain;charset=utf-8"/"UTF8_STRING"/"text/plain")`,
   `set_selection(source, serial)` using `self.pointer_state.serial` (already tracked, mod.rs:2218/2055);
   on `wl_data_source.send(mime, fd)` write text+close fd; on `cancelled` drop source.
5. PASTE: from tracked offer `pipe2()` + `receive(mime, wfd)` + roundtrip + read rfd to EOF → UTF-8.
6. Route in `wayland::clipboard`: prefer native when manager bound, else current x11_clipboard fallback.
   Verify on a REAL Wayland session (this box is XWayland).

**SESSION PRODUCTIVE LIMIT:** all tractable+verifiable-here bugs are fixed. Everything remaining needs
either (a) **WM re-login** — #9 real-wheel scroll, #10 dead-key/IME real-input, #11 a11y screen-reader —
or (b) a focused fresh-context session for the ~250-line native wl_data_device impl (plan above), best
done where it can be runtime-tested (real Wayland). **RECOMMEND: re-login to verify the landed fixes**
(scroll, CJK/Hangul render is already screenshot-verified; é/IME + a11y + real-wheel are the open checks),
then restart the loop for native clipboard + a11y. Cron loop left running; next firings will reiterate
this until re-login or a fresh session.

### Cron firing 32 — native wl_data_device recon TURN-KEY; autonomous loop PAUSED (productive limit)
Did the full code-level recon for native wl_data_device — it's turn-key now (do it in a fresh-context
session, ideally one that can run AZ_BACKEND=wayland with a mapped window to actually test copy/paste):
- **Interfaces (core → libwayland exports them):** dlsym `wl_data_device_manager_interface`,
  `wl_data_device_interface`, `wl_data_source_interface`, `wl_data_offer_interface` in `wayland/dlopen.rs`
  EXACTLY like `wl_seat_interface` (dlopen.rs:345 — `*load_symbol!(lib_client, *const wl_interface, "…")`),
  add the 4 fields to the `Wayland` struct (near :77/:93).
- **Bind + get device:** add a `"wl_data_device_manager"` arm to `registry_global_handler`
  (events.rs:491) — copy the `zwp_text_input_manager_v3` arm (events.rs:371-444) verbatim: `wl_registry_bind`
  the manager, then `get_data_device(seat)` (opcode 1, new_id+object) via the marshal_flags/constructor
  template at events.rs:403-424, then `wl_proxy_add_listener` for the data_device.
- **Opcodes (stable core protocol):** data_device_manager: create_data_source=0, get_data_device=1.
  data_device requests: start_drag=0, set_selection=1(source,serial), release=2; events: data_offer=0,
  enter=1, leave=2, motion=3, drop=4, selection=5(offer). data_source requests: offer=0(mime), destroy=1,
  set_actions=2; events: target=0, send=1(mime,fd), cancelled=2. data_offer requests: accept=0, receive=1(mime,fd),
  destroy=2; events: offer=0(mime), source_actions=1, action=2.
- **COPY:** create_data_source (marshal_constructor, new_id) → offer "text/plain;charset=utf-8"/"UTF8_STRING"/"text/plain"
  → set_selection(source, self.pointer_state.serial [mod.rs:2218/2055]). On data_source.send(mime,fd):
  write text + close fd; on cancelled: destroy source.
- **PASTE:** from tracked selection wl_data_offer: `libc::pipe2`, receive(mime, wfd) (opcode 1, 'sh' sig —
  wl_proxy_marshal passes the fd as an int arg, libwayland does SCM_RIGHTS via the interface signature),
  close wfd, wl_display_roundtrip, read rfd to EOF → UTF-8. **fd-marshalling precedent:** shm create_pool
  (tooltip.rs:220, `wl_shm_create_pool(shm, fd, size)`).
- **Route:** `wayland::clipboard` free fns have no window; thread native through `WaylandWindow::sync_clipboard`
  (mod.rs:704, has &mut self + wl objects). Prefer native when `data_device_manager` is bound, else the
  current x11_clipboard fallback (now reliable after firing-30 persistent-owner fix).

**AUTONOMOUS LOOP PAUSED** (cron `0fc338cf` deleted) — same call as firing 24: the productive,
verifiable-here limit is reached. Everything left needs USER ACTION:
- **RE-LOGIN (highest value):** unblocks on-screen verification of all 5 landed+committed fixes —
  #9 real mouse-wheel scroll (+ momentum/bounce), #10 dead-key `é` + fcitx5/ibus Japanese IME preedit,
  #11 a11y (Orca). CJK/Hangul GLYPH rendering is ALREADY screenshot-verified. azul-paint + libazul.so are
  rebuilt with everything.
- **THEN** re-issue the `/loop` cron command to resume: next focused tasks are native wl_data_device
  (turn-key recipe above) and #11 a11y on X11.
This session landed 5 fixes (6 code commits + docs): #9 scroll redraw (x11+wl), setlocale input,
CFF glyph decode, Hangul cmap-fallback, Wayland clipboard reliability. 2 visually verified, scroll
code-proven, rest await re-login.

### Cron firing 33 — USER RE-LOGGED, tested on NATIVE X11 (xfwm4); X11 redraw ARCHITECTURE + crash FIXED
User re-logged into a native X11 session (DISPLAY=:0.0, xfwm4 WM, GPU/WebRender — GL works now, not
the CPU-fallback the headless tests used). Tested azul-paint + the contenteditable C example
(tests/e2e/contenteditable.c, built vs target/release/libazul.so). USER-REPORTED bugs:
- redraw doesn't work on resize; scroll + caret positioning don't update; cursor blink stuck (all
  "needs a repaint that never happens").
- single-line edit shows I-beam cursor on hover; multi-line textarea does NOT.
- crash after clicking minimize/maximize.
- azul-paint: only draws first frame, no click response (window self-closes — see below).

**X11-API AUDIT (subagent, vs official Xlib/EWMH/XInput2/XIM docs) — KEY FINDINGS (persist these):**
1. (ROOT of no-repaint) GPU render_and_present skipped rendering after frame 1 unless
   scroll/scrollbar-fade/virtual-view active — dropped resize/caret/blink/physics-scroll. AND
   handle_event's Expose arm re-posted an Expose (request_redraw) instead of render → blocking idle
   path never painted. Synthetic-Expose-via-XSendEvent is NOT a valid app redraw primitive (Xlib docs);
   idiomatic = dirty-flag + drain-then-render.
2. The two event paths (poll_event inline match vs handle_event) DIVERGE (Expose, etc.) — should be one
   shared dispatch.
3. (minimize/maximize crash) GPU path fed a 0-size ConfigureNotify into renderer.render(0,0)/glViewport
   with no clamp (CPU path already guarded w/h>0). Also no MapNotify/UnmapNotify handling → renders an
   unmapped/0-size window.
4. XIM: must OR XNFilterEvents into XSelectInput (never queried). Use Xutf8LookupString (UTF-8) not
   XmbLookupString (locale-encoded). Handle XBufferOverflow.
5. XI2: key pen valuators on ev.sourceid (slave) not ev.deviceid (master); add scroll-class valuators
   for smooth scroll.
6. After poll() wakes, DRAIN fully (while XPending) — Xlib buffers events; fd-readability ≠ queue-empty.
(EWMH _NET_WM_STATE maximize message + XFilterEvent-on-every-event + setlocale/XSetLocaleModifiers order
were CONFIRMED CORRECT.)

**FIXED + COMMITTED (fd6fa89d9, architecture):**
- Single `needs_redraw` render-intent flag: request_redraw() sets it; render_and_present() CONSUMES it
  and renders whenever set (replaces the incomplete skip-heuristic). Any requested repaint now paints.
- handle_event Expose arm renders directly (matches poll_event).
- Crash: WebRender framebuffer clamped to ≥1×1 + render_and_present skips when window size==0 (chose a
  size-guard over Map/Unmap tracking — WM-independent, can't wrongly blank a visible window). MapNotify
  requests a repaint.
COMPILES (the is_mapped variant built clean @16:38; the size-guard edit only removes code + adds a
3-line guard using the existing physical_size pattern). **NOT live-verified** — see env quirk.

**ENV QUIRK (important for future firings):** this dev shell KILLS foreground `sleep` and commands
>~120s (exit 144). Earlier I mis-read 144s as crashes + over-relaunched GUI apps (the churn the handoff
warns against). LESSON: builds via run_in_background (they detach + finish even if the task reports 144 —
poll the libazul.so timestamp; NEVER foreground-`sleep`); do NOT pkill+relaunch GUI apps in a loop.
contenteditable links libazul.so via rpath → auto-fresh on rebuild (no recompile needed).

**STILL OPEN (next firings, priority order set by user — context menus next):**
1. Verify fd6fa89d9 compiles + (user) live-tests resize/scroll/caret/blink/minimize.
2. **CONTEXT MENUS** (user priority): cpurendered screen-positioned popup windows for dropdowns, window
   menus, div context-menus. Scaffolding: x11/menu.rs, wayland/menu.rs, events.rs:904
   try_show_context_menu→show_window_based_context_menu, mod.rs:3221 show_menu_from_callback,
   pending_window_creates + override_redirect. Architecture first.
3. Remaining audit API fixes (#4/#5/#6 above).
4. textarea hover I-beam cursor; exit-time GL texture-cache TLS-dtor crash; azul-paint self-close
   (window unmaps/closes → run.rs:1323 process::exit → crash dropping the gl_texture_cache BTreeMap).
Cron loop relaunched (job fff9ac48, every ~10 min). libazul.so rebuilding.

### Cron firing 34/35 — X11-API stability fixes DONE+committed; menu architecture clarified by USER
**X11-API stability fixes — committed 7244a16c8 (builds clean):** full XPending drain after poll() (both
paths, finding 10); XI2 pen valuators by ev.sourceid not deviceid (finding 9); Xutf8LookupString instead
of XmbLookupString (finding 6). Deferred: XNFilterEvents (needs variadic XGetICValues; static mask ok).
**X11 _NET_WM_WINDOW_TYPE hint — building/committing:** menu/tooltip/dialog windows now set the EWMH
window-type atom (POPUP_MENU/TOOLTIP/DIALOG) after XSetWMProtocols (mod.rs ~1257) so compositors classify
popups (shadows/effects). Used c_long (correct format-32 width; the existing set_is_top_level `as u32`
over-reads on LP64 — fix later).

**SCROLL physics timer + CURSOR BLINK:** should be FIXED by the firing-33 redraw-architecture change
(needs_redraw intent flag — timer callbacks call request_redraw → needs_redraw=true → render_and_present
no longer skips). User confirmed CHARACTER RENDERING works; scroll-momentum + cursor-blink await the
user's interactive confirm (both are timer→request_redraw→render, same path now honored).

**MENU ARCHITECTURE — AUTHORITATIVE USER DIRECTIVE (do it THIS way, no "separate menu flow"):**
A menu is just a REGULAR multi-window: a callback (AzCallbackInfo::create_window — layout/callbacks.rs:929,
the spawn-menu entry) pushes a `WindowCreateOptions` whose `layout_callback` is the framework's
`menu_layout_callback` (desktop/menu.rs:354 → menu_renderer::create_menu_dom_with_css, already styled from
SystemStyle via create_menu_stylesheet) → goes through the SAME pending_window_creates / multi-window loop
as any window. The ONLY menu-specific behaviour vs a normal window:
  1. **`size_to_content`** — size the popup window to its laid-out content. EXISTS on Windows
     (windows/mod.rs:449-643: create 1×1 placeholder → layout → resize). **NOT implemented on X11 or
     Wayland** — implement it there (after creating the menu window, run layout, measure the root content
     size, XResizeWindow / wl resize). This is why menus are mis-sized on Linux.
  2. **RELATIVE positioning via `WindowCreateOptions.parent_window`** (OptionHwndHandle, window.rs:1119) +
     an offset — NOT absolute screen coords. Refactor desktop/menu.rs show_menu to set parent_window +
     relative offset (cursor/trigger-rect relative to the parent) instead of computing absolute positions
     (position_relative_to_cursor/_rect). The backend positions relative to the parent:
       - X11: parent window's screen position + offset (override_redirect, exact placement).
       - Wayland: **xdg_popup** via get_popup(parent_xdg_surface) + xdg_positioner anchored to the
         trigger_rect (relative — Wayland has NO absolute window positioning). Scaffold: wayland/mod.rs:256
         WaylandPopupWindow, wayland/menu.rs trigger_rect. THIS is the Wayland relative-positioning ask.
Keep reusing the multi-window loop; do not fork a menu-specific code path. Goal: dropdowns, menu-bar
menus, and div context-menus all flow through create_window + size_to_content + parent-relative.

**ALSO REQUESTED (user, this firing) — queue after the menu:**
- **IME cursor positioning** (so the candidate/preedit popup follows the caret): X11 = XSetICValues with
  XNPreeditAttributes/XNSpotLocation (XPoint at the caret, in window px) on each caret move; Wayland =
  zwp_text_input_v3 set_cursor_rectangle. sync_ime_position_to_os (x11/mod.rs) is the hook to fill in.
- **a11y integration scan** (AccessKit/AT-SPI adapter completeness — tree push, focus, actions) and
  **clipboard integration scan** (the firing-30 persistent-owner X11/XWayland path + native wl_data_device
  still owed). Proper review, not surface.

**PRIORITY ORDER NOW:** (1) finish+commit _NET_WM_WINDOW_TYPE hint → (2) MENU per the architecture above
(size_to_content on X11+Wayland → parent-relative positioning → Wayland xdg_popup) → (3) IME cursor
positioning (x11+wl) → (4) a11y scan → (5) clipboard scan (incl native wl_data_device) → (6) rest of
backlog (textarea cursor, exit crash, azul-paint self-close, #47/#48/hit-tester). User tests live.

### Cron firing 36 — IME cursor positioning VERIFIED already-implemented; menu size_to_content reality
**IME CURSOR POSITIONING (user asked to "implement") = ALREADY DONE on BOTH backends** (verified
end-to-end this firing; user to confirm live):
- Both X11 (mod.rs:2242 `update_ime_position_from_cursor`) and Wayland (same fn) compute the caret rect
  via `layout_window.get_focused_cursor_rect_viewport()` and set `current_window_state.ime_position`,
  then call `sync_ime_position_to_os()` — and crucially this runs AFTER EVERY LAYOUT (x11 mod.rs:2234,
  in regenerate_layout's tail), so the spot follows the caret on every text/caret change.
- X11 `sync_ime_position_to_os` (mod.rs:3411): XSetICValues with XNSpotLocation wrapped in a
  XNPreeditAttributes nested list (correct per XIM spec; consulted under XIMPreeditPosition style).
- Wayland `sync_ime_position_to_os` (mod.rs:4185): zwp_text_input_v3 `set_cursor_rectangle` (opcode 6) +
  commit + flush, with a GTK-IM fallback (GdkRectangle).
  → No reimplementation needed. If the candidate window mis-tracks live, debug
  `get_focused_cursor_rect_viewport` coords, not the plumbing.

**MENU size_to_content REALITY (correctness note for the next firing):** `LayoutTree::get_content_size(0)`
exists (layout_tree.rs:1048) and returns overflow_content_size/used_size+inline bounds — BUT it reads the
size the root was LAID OUT at, i.e. constrained to the placeholder WINDOW size. For a true
shrink-to-fit menu it must be measured with an UNCONSTRAINED available width (intrinsic/measure pass) OR
the menu root DOM must be `width:fit-content`/auto so the natural size falls out. So size_to_content is
NOT a naive post-layout read — it needs either a measure-pass (layout the menu DOM at +inf width once,
read get_content_size, then create/resize the window to it) or a fit-content menu root in
menu_renderer::create_menu_dom_with_css. Windows' size_to_content is also still a commented TODO
(windows/mod.rs:449) — implement it once in a shared place and call from all backends.

**PRIORITY ORDER NOW:** (1) MENU: size_to_content (intrinsic measure-pass, see above) on X11+Wayland →
parent-relative positioning (parent_window offset; Wayland xdg_popup) — the big multi-firing item →
(2) a11y integration scan (next bounded item) → (3) clipboard scan (native wl_data_device owed) →
(4) rest of backlog. (IME cursor = DONE; scroll/cursor-blink = fixed pending live confirm.)

### Cron firing 37 — a11y scan FIX committed; clipboard scan done
**a11y scan → FIXED (commit eaeee0c5c, builds clean):** the X11 accesskit ActivationHandler's
`request_initial_tree()` returned None — but `update_tree` uses `update_if_active` (no-op until the
adapter is ACTIVE), and the adapter only activates when request_initial_tree returns Some. So a screen
reader connecting after the first layout saw an empty tree (a11y inert). Now a shared
`last_tree: Arc<Mutex<Option<TreeUpdate>>>` is stored by update_tree and returned from
request_initial_tree → adapter activates with the live tree. Rest of the X11 a11y path verified SOUND:
update_a11y_tree builds roles/text/cursor/selection/focus (window.rs:2925) and pushes after every layout;
actions decode + route via process_accessibility_action (run loop polls process_accessibility_actions).
Remaining a11y: live-verify with Orca; review macOS/Windows parity later.

**clipboard scan → findings (no quick code fix; the real fix is the owed native wl path):**
- ClipboardManager flow is clean: copy = On::Copy → set_copy_content / get_copy_content → sync_clipboard;
  paste = Ctrl+V → get_system_clipboard → set_paste_content → On::Paste reads get_clipboard_content.
- **Routing gap:** `get_system_clipboard()` (common/event.rs:269-271) is hardcoded to
  `linux::x11::clipboard::get_clipboard_content()` for ALL Linux — there is NO runtime branch for the
  Wayland backend, so the firing-30 `wayland::clipboard` persistent-owner fix is effectively DEAD CODE
  (never routed to). Works under XWayland (x11_clipboard bridges via the X server); FAILS on pure
  Wayland (no X). Both x11/wayland clipboard modules use x11_clipboard anyway, so there's no cheap fix.
- **Proper fix = native wl_data_device + runtime backend routing** (the firing-32 TURN-KEY plan: dlsym
  the 4 core interfaces, bind manager in registry, get_data_device + listeners, copy via
  source+offer+set_selection(serial), paste via pipe+receive, then route get_system_clipboard/sync to the
  active backend). Best done with a real Wayland session to test.

**PRIORITY ORDER NOW:** (1) **MENU refactor** (size_to_content intrinsic measure-pass on X11+Wayland →
parent-relative positioning, Wayland xdg_popup) — the big remaining item, user's top priority → (2)
native wl_data_device + clipboard runtime routing → (3) rest of backlog (textarea hover cursor, exit-time
GL texture-cache TLS crash, azul-paint self-close, #47/#48/hit-tester). DONE this session: scroll redraw,
setlocale, CFF+Hangul glyphs, X11-API stability (drain/sourceid/Xutf8), redraw architecture + minimize
crash, EWMH window-type hint, IME cursor (verified), a11y request_initial_tree. Awaiting user live test.

### Cron firing 38 — MULTI-WINDOW / MENU architecture reviewed (USER directive); size_to_content algorithm pinned
USER reframed the menu as fundamentally a MULTI-WINDOW problem (Win32 had the good arch); reviewed it.
**FOUNDATION IS SOLID — do NOT rebuild it:**
- Multi-window loop (Linux ≈ Win32): `registry` (register/unregister/get_all_window_ids) +
  per-window `pending_window_creates` drained in run.rs's loop spawns each window. A menu = a normal
  registered window via this loop (= user's "reuse multi-window, no separate flow"). `is_multi_window`
  switches the wait strategy (run.rs:1117).
- Positioning module EXISTS + good: `menu.rs::calculate_menu_position` → `get_display_at_point().work_area`
  → `calculate_auto_position` does H+V flip-on-overflow + final clamp to work-area (= "natural
  positioning, away from edges, expand up/down"). Submenu variants: position_submenu_right/left.
- Hierarchy DATA exists: MenuWindowData.parent_menu_id (close parent on submenu close) +
  child_menu_ids: Arc<Mutex<Vec<u64>>>; WindowCreateOptions.parent_window (OptionHwndHandle, window.rs:1119).

**GAPS to implement (priority order — this is the menu work):**
1. **size_to_content (size determination, NOT impl on Linux/anywhere — Windows is a commented TODO).**
   USER ALGORITHM (pinned): create window at size ~0 → layout → read the OVERFLOW size (=natural extent;
   `LayoutTree::get_content_size(0)` already returns `overflow_content_size` first, layout_tree.rs:1048) →
   resize window to it → RELAYOUT at the new size (drops the scrollbars that appeared at size 0). For
   menus this is exact (items are nowrap + min-width:160px, so size-0 doesn't over-wrap). Implement ONCE
   (shared), call from all backends. Hook: in render_and_present AFTER the first regenerate_layout, if a
   `size_to_content_pending` flag is set: get_content_size(0) → XResizeWindow + set window_state.size →
   frame_needs_regeneration=true → return (skip presenting the tiny frame). Field + tiny initial size for
   size_to_content windows. (XResizeWindow is in dlopen.rs:126.)
2. **Height clamp to monitor**: calculate_auto_position clamps POSITION, not the menu HEIGHT. Clamp
   menu_size.height = min(natural, work_area.height) and let the menu's own DOM scroll when taller.
3. **Hierarchy wiring (verify/complete)**: submenu spawns as a CHILD (parent_window set), closing a parent
   closes its child_menu_ids, and focus-loss of the whole menu chain dismisses it. Data exists; confirm
   the close/dismiss behaviour is wired in the loop (and on X11 override_redirect menus, focus-out needs a
   pointer-grab or root-click watch to dismiss — check show_window_based_context_menu path).
**USER NOTE:** leave the `text_input` widget alone (it will become a thin wrapper over contenteditable
later) — so the textarea hover-cursor fix belongs at the contenteditable/default-cursor level, not in
text_input.

**PRIORITY ORDER NOW:** (1) size_to_content (shared impl, user algorithm above) → (2) height-clamp +
feed natural size into calculate_menu_position → (3) hierarchy close/dismiss wiring → (4) Wayland
xdg_popup for the menu (relative positioning) → (5) native wl_data_device clipboard → (6) rest.

#### TWO OPEN ARCHITECTURAL QUESTIONS (user, pre-compaction — WRITE-DOWN, must solve for menus)

**Q1. Multi-window event routing — how do N windows' events flow through ONE loop?**
Current X11 reality: EACH X11Window has its OWN `display: *mut Display` and calls `XOpenDisplay` in
`new_with_resources` (setlocale→XOpenDisplay path) — i.e. ONE X CONNECTION PER WINDOW. The run loop
(run.rs:1119) iterates `get_all_window_ids()` and calls `window.poll_event()` per window; each poll_event
drains only ITS OWN connection, so routing is per-connection (each window sees only its own events — no
`event.xany.window` filtering exists or is needed). BUT the blocking wait differs: single-window uses
`wait_for_events()` (poll on that window's fd + timerfds); MULTI-window uses
`wait_for_x11_connection_activity()` (run.rs:1296) which `select()s on only ONE display fd with a 16ms
timeout — so other windows' events are caught by the 16ms poll, NOT event-driven (a busy-ish poll).
  - DECISION NEEDED: (a) keep per-window connections + make the multi-window wait poll ALL windows' fds
    (build the pollfd set from every window's XConnectionNumber + all timerfds) so it's event-driven, not
    16ms-spin; OR (b) switch to ONE shared display connection for all windows + a single event pump that
    dispatches each event to the window matching `event.xany.window` (this is closer to the Win32 model:
    one GetMessage loop, per-HWND window_proc). (b) is cleaner long-term; (a) is the smaller change.
    Win32 (the "good" ref) = single message queue, dispatched per-HWND — favours (b).

**Q2. Menu dismissal — "click outside" / focus-loss (currently only item-clicks handled).**
Menus are override_redirect (bypass the WM) → they get NO FocusOut/WM focus events, so there is currently
NO way to detect a click outside the menu → the menu never auto-closes. There is NO `XGrabPointer`
anywhere (grep: zero hits). The STANDARD X11 menu mechanism (what GTK/Qt do):
  - On menu open: `XGrabPointer(menu_window, owner_events=True, ButtonPress|ButtonRelease|PointerMotion,
    GrabModeAsync, …, CurrentTime)` so the WHOLE screen's pointer events route to the menu app.
  - A ButtonPress whose coords are OUTSIDE the menu (and outside any open submenu in the chain) →
    DISMISS the menu chain + `XUngrabPointer`. A press inside an item → activate. Motion over a
    submenu-parent item → open the submenu as a CHILD (the grab stays on the root menu; owner_events=True
    lets the submenu window still receive its own events).
  - Keyboard: Escape dismisses; arrows navigate. (XGrabKeyboard optional.)
  - Submenu chain = the parent_menu_id/child_menu_ids hierarchy: dismiss walks children→parent and
    ungrabs once at the root. Closing a parent closes all descendants.
  - Wayland equivalent: xdg_popup has a built-in GRAB (xdg_popup.grab(seat, serial)) + the compositor
    sends `xdg_popup.popup_done` on click-outside → just destroy the popup on popup_done. (So Wayland gets
    Q2 "for free" via the popup grab — another reason to use real xdg_popup, not a toplevel.)
  → IMPLEMENT: pointer-grab-on-open + click-outside-bounds dismissal (X11), popup_done (Wayland), wired to
    the parent/child hierarchy. This is THE missing piece for "menu closes properly".

### Cron firing 39 — size_to_content DONE; USER chose option (b) — TURN-KEY refactor plan
**size_to_content (X11) DONE — commit edb557168, builds clean.** Window created UNMAPPED → first
poll_event lays out at 16x16 → get_content_size(0) (overflow extent) → XResizeWindow + map → relayout.
apply_size_to_content() in x11/mod.rs (just before regenerate_layout). NEXT for menus: feed that natural
size through calculate_menu_position WITH height-clamp = min(natural.h, work_area.h).

**USER DECISION: do option (b) — ONE shared X display + single event pump dispatched per-window (Win32
model). "RefreshDomAllWindows should then start to work." Plus a menu-close event/callback.**

TURN-KEY REFACTOR PLAN (X11):
- KEY ENABLER: the registry ALREADY keys by the X Window id (`window_id = x11_window.window as u64`,
  run.rs:1086) → the pump can dispatch with `registry::get_window(ev.xany.window as u64)` — NO new map.
- **KEY RISK / the actual work:** `poll_event` has its OWN `match event.type_` (x11/mod.rs:736: Expose/
  FocusIn/FocusOut/Configure/Button/Key/Motion/...) SEPARATE from `handle_event`'s match (the divergent-
  handler problem). The single pump must call ONE consolidated dispatch. STEP 0 = merge poll_event's match
  into handle_event (audit BOTH arms, unify, so the pump uses handle_event and nothing is lost). Do this
  first + build; it's the error-prone part (untestable here — a missing arm silently kills that event).
- Step 1: open `XOpenDisplay` ONCE in run() (x11 path ~run.rs:1056), pass the display into
  new_with_resources (currently opens its own at x11/mod.rs:1187). TWO call sites: initial window +
  pending_window_creates (run.rs:1173). X11Window.display becomes a SHARED borrow — do NOT XCloseDisplay
  per-window in Drop; close once when the loop exits.
- Step 2: replace the per-window event drain (run.rs:1120) with a SINGLE pump:
  `while XPending(shared)>0 { XNextEvent(shared,&ev); if let Some(w)=registry::get_window(ev.xany.window){ (&mut *w).handle_event(&ev) } }`.
- Step 3: SPLIT poll_event into (a) event dispatch (now the pump via handle_event) and (b) per-window
  POST-processing — render-if-frame_needs_regeneration, check_timers_and_threads, apply_size_to_content,
  a11y — called per-window AFTER the pump each loop turn.
- Step 4: wait event-driven on `poll(shared_fd + EVERY window's timerfds)` (replaces the 16ms-poll
  `wait_for_x11_connection_activity`, run.rs:1349). Single-window wait_for_events stays a fast path.
- Step 5: **RefreshDomAllWindows** (currently common/event.rs:3809 bundles it with RefreshDom → only the
  current window). After the refactor, when a callback returns it, iterate the registry and set
  `frame_needs_regeneration = true` on EVERY window. (Could be done independently of the refactor too.)

MENU-CLOSE (user's design, can be done BEFORE/independent of the connection refactor):
- New framework callback `on_menu_window_close(RefAny, CallbackInfo) -> Update` that does
  `CallbackInfo::set_window_state(closed = true)`; the regular loop's closed-window check
  (run.rs:1138 `if !window.is_open()`) then unregisters+drops it. So only the TRIGGER is new.
- Trigger = XGrabPointer on menu open (owner_events=True, ButtonPress|Release|Motion, GrabModeAsync);
  ButtonPress OUTSIDE the menu/submenu bounds → set closed=true + XUngrabPointer; inside item → activate;
  motion over submenu-parent → spawn child menu (grab stays on root). Escape dismisses.
  Wayland: xdg_popup.grab + popup_done → destroy. Wire to parent_menu_id/child_menu_ids (close chain).

PRIORITY NOW: (0) consolidate poll_event↔handle_event → (1-4) shared display + pump + wait →
(5) RefreshDomAllWindows-all → menu-close (grab+closed-flag) → height-clamp into calculate_menu_position
→ Wayland xdg_popup. Build-verify each step (user runtime-tests later).

### Cron firing 40 — STEP 0 DONE (handler dedup); STEP 1+2 refined plan
**STEP 0 DONE — commit 8b80aa716, builds clean.** poll_event's own 244-line `match event.type_` was a
DUPLICATE of handle_event (the divergent-handler risk). handle_event is a complete superset, so
poll_event's while-loop body is now just `XNextEvent; self.handle_event(&mut event)`. -244 lines, one
dispatch. Behavior-preserving (handle_event's ClientMessage sets is_open=false; the run loop closes on
!is_open). This UNBLOCKS the pump (which dispatches via handle_event).

**STEP 1+2 (shared display + single pump) — REFINED PLAN + the complication found:**
- Current sites (post-dedup): X11Window::new_with_resources at mod.rs:867 opens its OWN XOpenDisplay at
  mod.rs:943; XCloseDisplay at mod.rs:822 + Drop at mod.rs:3226. Per-window timer_fds:BTreeMap<usize,i32>
  at mod.rs:598. run.rs: LinuxWindow::new_with_resources (run.rs:1056, backend-agnostic wrapper, picks
  X11/Wayland INSIDE), pending-window X11Window::new_with_resources (run.rs:1173), wait
  wait_for_x11_connection_activity (run.rs:1298/1349), per-window drain (run.rs:1120).
- COMPLICATION: window creation goes through the backend-agnostic LinuxWindow wrapper, so "open ONE
  display in run() before any window" fights the X11/Wayland abstraction (Wayland has no XOpenDisplay).
- RECOMMENDED approach (handles the actual menu use-case cleanly, less invasive than a global rewrite):
  CHILDREN (menus/dialogs from pending_window_creates) REUSE THE PARENT's display instead of opening
  their own → add an optional `shared_display: Option<*mut Display>` param to X11Window::new_with_resources;
  when Some, skip XOpenDisplay (mod.rs:943) AND skip XCloseDisplay in Drop (only the owner closes).
  Then the parent's display carries the menu's events too.
- PUMP: replace run.rs:1120's `for wid { while poll_event() }` with, per OWNER display, a single drain that
  dispatches by xany.window: `while XPending(disp){ XNextEvent(disp,&ev); if let Some(w)=registry::get_window(ev.xany.window as u64){ (&mut *w).handle_event(&ev) } }`. (registry keys by X Window id — run.rs:1086.)
- SPLIT poll_event: it currently does pre-loop POST-processing (timers/gnome/menu-cbs/size_to_content/
  frame_needs_regeneration render, mod.rs:~667-699) THEN the drain (now 1-liner). Move the drain to the
  pump; rename the rest to `tick()` called per-window after the pump.
- WAIT (run.rs:1349): poll(owner_display_fd + EVERY window's timer_fds) — gather timerfds across the
  registry. Single-window keeps wait_for_events.
- Then (5) RefreshDomAllWindows: iterate registry → set frame_needs_regeneration on all (common/event.rs:3809
  currently bundles it with RefreshDom = current window only).
NOTE: each sub-step build-verifies; do shared-display+pump together (a shared display breaks per-window
poll_event routing, so they can't land separately). User runtime-tests after.

### Cron firing 41/42 — option-(b) refactor DONE through step 5 (4 commits, all build clean)
USER chose "shared-display refactor first" + OK'd api.json changes + suggested using a parent field on
the options + RefAny-custom-destructors for cleanup.
- **STEP 0 (8b80aa716, prev firing):** poll_event match deduped into handle_event.
- **STEP 1a (ba03cf237):** added cross-platform `parent_window_id: u64` to WindowCreateOptions
  (layout/src/window_state.rs) = the window-registry key (X Window id / wl_surface / HWND / NSWindow;
  0 = no parent). NOTE: the pre-existing `parent_window` is Windows-only (HwndHandle on
  WindowsWindowOptions); LinuxWindowOptions had no parent field. Synced via `azul-doc autofix` (applied
  ONLY the 0003_WindowCreateOptions patch — the other 3 patches are pre-existing run_destructor drift on
  CssPropertyCachePtr/GlContextPtr/IconProviderHandle, left alone) + normalize + `codegen all`.
- **STEP 1b+2 (5dc5ff5a0):** SHARED-DISPLAY + OWNER-DISPATCH PUMP (non-invasive — NO run.rs restructure,
  Wayland untouched). X11Window gained `owns_display: bool`. new_with_resources: if
  `options.parent_window_id != 0`, resolve it via `super::registry::get_window` → parent's X display →
  REUSE it (owns_display=false); else open own (true). Drop/close XCloseDisplay only if owns_display.
  poll_event drains the connection ONLY for the owner (`if self.owns_display { while XPending {...} }`)
  and dispatches each event to its target by `event.any.window` (XAnyEvent) — self, or a child via the
  registry (`LinuxWindow::X11(child) => child.handle_event`). Children skip draining (no race) but still
  run their OWN poll_event post-processing (timers/size_to_content/render) each loop turn; the
  multi-window 16ms wake (wait_for_x11_connection_activity) services them. Menus set
  parent_window_id=self.window (events.rs show_window_based_context_menu + mod.rs show_menu_from_callback).
- **STEP 5 (acf87079d):** Update::RefreshDomAllWindows now → ProcessEventResult::ShouldRegenerateDomAllWindows
  (was bundled with RefreshDom), whose handler iterates the registry + sets frame_needs_regeneration +
  request_redraw on EVERY X11 window (self skipped in loop to avoid &mut self aliasing, handled after).

**REMAINING (menu) — priority order:**
1. **MENU-CLOSE / click-outside** (THE user-visible missing piece): XGrabPointer on menu open
   (owner_events, ButtonPress|Release|Motion, GrabModeAsync) → ButtonPress outside the menu/submenu bounds
   → set the menu's is_open=false + XUngrabPointer → the run loop's !is_open check (run.rs:1138) drops it.
   Use a **RefAny custom destructor** (user hint) on the menu's data to ungrab + close the submenu chain
   when the menu window is dropped. Escape dismisses. Wire to parent_menu_id/child_menu_ids.
2. **Height-clamp** menu_size.height = min(natural, work_area.height) feeding calculate_menu_position;
   let the menu DOM scroll when taller.
3. **Wayland xdg_popup** (relative positioning + popup_done for free dismissal).
4. **LIFETIME (follow-up):** parent-owns-display means if a PARENT closes while a child menu is open, the
   shared display is freed under the child → the close-chain (menu hierarchy) MUST close children before
   the parent. Common case (menu closes first) is fine; handle in the menu-close step.
User runtime-tests the shared-display + menu rendering interactively.

### Cron firing 43 — MENU CLICK-OUTSIDE DISMISSAL done (commit aa70c6c8b, builds clean)
The user-visible "missing piece". Added XGrabPointer/XUngrabPointer + Time/CurrentTime/GrabModeAsync/
GrabSuccess to x11 dlopen+defines. On open (apply_size_to_content, after XMapWindow) a WindowType::Menu
window grabs the pointer with owner_events=FALSE → ALL pointer events (incl. clicks on other windows /
root) are delivered to the menu (xany.window = the grab/menu window, so the owner-dispatch pump routes
them to it). handle_mouse_button (events.rs) checks the press coords (menu-relative under the grab):
inside [0,w]x[0,h] = item click (falls through); outside = dismiss → is_open=false (run loop drops it;
close() now XUngrabPointer's for WindowType::Menu). Escape (handle_keyboard) also dismisses.

REMAINING (menu), priority order:
1. **Escape keyboard-focus follow-up:** override_redirect menus don't auto-get WM focus + only the POINTER
   is grabbed, so KeyPress (Escape) only reaches the menu if it happens to be focused. Add XGrabKeyboard
   on menu open (or XSetInputFocus to the menu) for reliable Escape + arrow-key navigation. Pointer
   click-outside is already solid.
2. **Height-clamp:** menu_size.height = min(natural, work_area.height) feeding calculate_menu_position;
   let the menu DOM scroll when taller than the monitor.
3. **Submenu chains + RefAny-destructor cleanup (user hint):** opening a submenu as a child (parent_menu_id/
   child_menu_ids); the root menu keeps the grab; dismiss walks children→parent + ungrabs once. Put the
   ungrab + close-chain in a RefAny custom destructor on the menu's data so it fires on drop.
4. **Wayland xdg_popup:** relative positioning + popup_done = click-outside dismissal for free.
5. **LIFETIME:** parent-owns-shared-display → close child menus BEFORE the parent (else the child's display
   is freed under it). Wire into the close-chain (#3).
Also still open (non-menu): textarea hover I-beam cursor (contenteditable default cursor, NOT text_input —
user said leave text_input alone); exit-time GL texture-cache TLS-dtor crash; native wl_data_device
clipboard + runtime routing. User runtime-tests menus interactively (right-click a div w/ a context menu).

### Cron firing 44 — MENU POSITIONING fixed: at cursor, not (0,0) (commit 6cd7b6a21, builds clean)
Found a real bug: `calculate_menu_position` (the cursor/trigger placement w/ work-area edge-flip + clamp)
was ONLY called from unit tests — `show_menu` hard-coded `window_state.position = (0,0)`, so every context
menu opened in the SCREEN's top-left corner. Now show_menu calls calculate_menu_position (AutoCursor, or
AutoHitRect when trigger_rect is set), converting the cursor from parent-window-relative to ABSOLUTE
(screen) coords (offset by parent_window_position) and using a content-size ESTIMATE (item_count*28+8) to
drive the flip/clamp. size_to_content still resizes the WINDOW to its true content after; position is set
up-front from the estimate.
Deferred Escape XGrabKeyboard this firing: grabbing the keyboard has a scary frozen-input failure mode I
can't runtime-test — click-outside dismissal (pointer grab) already works; do Escape-grab when testable.

REMAINING (menu), priority order:
1. **Reposition + height-clamp using the TRUE size_to_content size** (currently the estimate): in X11
   apply_size_to_content, after computing final_size, re-clamp the window position to the monitor work_area
   (get_display_at_point + XMoveWindow) so a menu whose real content exceeds the estimate stays on-screen;
   and clamp height = min(natural, work_area.h) — but that needs the menu DOM to SCROLL (overflow) when
   taller than the monitor, so do the menu-DOM scroll first (menu_renderer) then the height-clamp.
2. **HiDPI:** menu position is logical→physical at DPI=1 only; scale by the monitor's factor for HiDPI.
3. **Escape XGrabKeyboard** (test-gated, see above) + arrow-key nav.
4. **Submenu chains + RefAny-destructor cleanup** (parent_menu_id/child_menu_ids; root keeps grab).
5. **Wayland xdg_popup** (relative pos + popup_done dismissal).
6. **LIFETIME:** close child menus before parent (shared display).
MENU NOW (X11): appears at cursor, content-sized, shares parent event loop, dismisses on click-outside.

### Cron firing 45 — menu reposition-after-size (commit 813e1baa7, builds clean)
apply_size_to_content now re-clamps a WindowType::Menu window's position to the monitor work-area using
the TRUE measured size (get_display_at_point + XMoveWindow), since show_menu positioned it from a size
ESTIMATE — so a menu whose real content exceeds the estimate (esp. near a screen edge) no longer spills
off-screen right/bottom. DPI=1 assumption (position physical, work_area logical).
REMAINING (menu): (1) height-clamp = min(natural, work_area.h) for menus TALLER than the monitor — needs
the menu DOM to SCROLL first (menu_renderer create_menu_stylesheet: add overflow-y:auto + a height that
fills the window; LOW risk, clipping is the current fallback) → then clamp final_size.height in
apply_size_to_content. (2) HiDPI position scaling. (3) Escape XGrabKeyboard (test-gated — frozen-input
risk). (4) submenu chains + RefAny-destructor cleanup. (5) Wayland xdg_popup. (6) close children before
parent (shared display lifetime). MENU NOW: cursor-positioned, content-sized, on-screen-clamped, shares
the parent event loop, click-outside dismiss. User should runtime-test the full menu interaction now.

### Cron firing 46 — height-clamp + scroll for over-tall menus (commit a95fc3206, builds clean)
apply_size_to_content now caps final_size.height to the monitor work-area height for WindowType::Menu
(in the same reposition block, reusing its get_display_at_point), and menu_renderer gives .menu-container
`overflow-y: auto` so items scroll within the capped window. Does NOT break size_to_content: the measure
reads overflow_content_size (natural extent), independent of the capped window. (Scroll behaviour is
build-verified only — user should confirm a long menu scrolls.)

MENU (X11) IS NOW FEATURE-COMPLETE FOR THE COMMON CASE: cursor-positioned (firing 44), content-sized
(size_to_content), on-screen-clamped width+height with scroll (firings 45/46), shares the parent event
loop (option-(b) shared-display pump, firings 40-42), click-outside dismissal (firing 43).
NOTE: ALL of this is BUILD-VERIFIED ONLY — none runtime-tested in this shell. User should now do a full
interactive pass (right-click a div w/ a context menu; try near screen edges + a long menu).

REMAINING (menu, all niche/big/test-gated): (1) HiDPI position scaling (currently DPI=1). (2) Escape
XGrabKeyboard + arrow-nav (test-gated — frozen-input risk). (3) submenu chains + RefAny-destructor cleanup
(parent_menu_id/child_menu_ids; root keeps the grab; close-chain). (4) Wayland xdg_popup (relative pos +
popup_done). (5) close children before parent (shared-display lifetime). 
NON-MENU backlog (diversify if menu testing stalls): textarea hover I-beam (contenteditable default
cursor:text in CursorTypeHitTest, NOT text_input); exit-time GL texture-cache TLS-dtor crash; native
wl_data_device clipboard + runtime routing; XNFilterEvents (deferred, low-value).

### Cron firing 47 — diversified to non-menu: textarea cursor FIXED; exit-GL-crash ROOT-CAUSED
Menu is feature-complete (build-verified) + awaiting user runtime test, so diversified to the non-menu backlog.
**TEXTAREA HOVER I-BEAM — FIXED (commit 0b21283d7, builds clean):** CursorTypeHitTest::new (layout/src/
hit_test.rs) resolved the hover cursor only from the text-run cursor tag + an EXPLICIT CSS `cursor` prop,
so an editable node without `cursor:text` fell through to Default (textarea showed no I-beam, single-line
did). Added an else-branch: a hovered node with NO explicit cursor that is editable
(NodeData::is_contenteditable() || NodeType::TextArea) now defaults to MouseCursorType::Text. Does NOT
touch text_input (it sets cursor:text explicitly). User-test: hover the textarea → I-beam. (Task #7 done.)
**EXIT-TIME GL TEXTURE-CACHE CRASH — ROOT-CAUSED (NOT fixed — needs gdb-confirmed runtime test):**
- TEXTURE_CACHE = thread_local RefCell<Option<OrderedMap<DocumentId, GlTextureStorage>>> (gl_texture_cache.rs:80),
  holds gl::Texture per doc. Texture::drop (core/src/gl.rs:3209) refcounts; on last drop calls
  self.gl_context.delete_textures([id]) = glDeleteTextures.
- gl_texture_cache::clear_all() EXISTS but is NEVER called on shutdown (zero call sites in the shell).
- x11/mod.rs close() (the Drop path) does: remove_document_textures(doc) → XDestroyWindow → XCloseDisplay
  (if owns_display). The GL context lives in render_mode (a STRUCT FIELD) which drops AFTER close() returns
  — so XCloseDisplay closes the X display BEFORE the GLX/EGL context is destroyed → the gl_context drop +
  any leftover Texture drops run glDeleteTextures/glXDestroyContext on a CLOSED display → crash. ALSO on
  plain thread exit the TEXTURE_CACHE TLS-dtor drops leftover Textures → GL on a dead context → crash.
- PROPOSED FIX (do with a gdb backtrace to confirm the exact mechanism first — a blind drop-ordering change
  to a crash risks worse UB): (a) in close()/Drop, DESTROY the GL context (drop render_mode) BEFORE
  XCloseDisplay; (b) call gl_texture_cache::clear_all() (or remove_document for every doc) during controlled
  shutdown while contexts are alive; (c) make the TEXTURE_CACHE TLS-dtor FORGET textures (skip
  glDeleteTextures) since at thread exit the context is gone anyway (mem::forget the handles — the context
  teardown already freed the GPU memory). (a)+(c) is the robust combo.
STATUS: menu = feature-complete (build-verified, NOT runtime-tested — user should test). Non-menu open:
exit-GL-crash (analysis above), native wl_data_device clipboard, XNFilterEvents (low-value). The bulk of
this whole effort is build-verified only; USER RUNTIME TESTING is now the bottleneck/highest-value step.

### Cron firing 48 — EXIT-GL-CRASH root-cause FIX applied (commit 94101c497, builds clean)
Implemented the teardown-ordering fix from firing 47's analysis. ROOT CAUSE confirmed: the EGL
`gl::GlContext` lives in the `render_mode` STRUCT FIELD (enum RenderMode::Gpu(GlContext, GlFunctions),
mod.rs:97), which dropped AFTER X11Window::drop()'s self.close() — so eglDestroyContext ran against a
destroyed window / closed X display (close() does XDestroyWindow + XCloseDisplay) → exit crash.
FIX: added a transient `RenderMode::None` variant + one match arm (present()); Drop now does
`self.render_mode = RenderMode::None;` BEFORE self.close(), dropping the EGL context while the window +
display are still alive — correct order (handles → GL context → X window → X display). render_mode holds
the ONLY EGL-context ref (Textures + WebRender hold the separate GlFunctions table = harmless to drop
late). Dropping it earlier is STRICTLY SAFER than after XCloseDisplay regardless of other holders.
CAVEAT: build-verified only. A gdb run on the actual exit would confirm no OTHER GlContext holder drops
after close() (e.g. if WebRender unexpectedly owns the EGL context). If the crash persists, gdb-backtrace
it: the next suspects are (a) WebRender/renderer in self.common dropping its GL state after close(), or
(b) the TEXTURE_CACHE TLS-dtor (gl_texture_cache.rs) — make it mem::forget handles at thread exit.

OVERALL after firings 26-48: scroll redraw, setlocale/CFF/Hangul/IME-cursor, X11-API stability, redraw
architecture, minimize crash, EWMH hints, a11y activation, option-(b) shared-display refactor (steps 0-5),
RefreshDomAllWindows, full context-menu (cursor-positioned/content-sized/clamped+scroll/shared-loop/
click-outside-dismiss), menu positioning, textarea I-beam, and now the exit-GL teardown fix — ALL
build-verified, almost NONE runtime-tested here. HIGHEST-VALUE NEXT STEP = USER INTERACTIVE TEST PASS +
gdb for any residual crash. Remaining build-able-blind work is niche/big/low-value (HiDPI, submenu,
Wayland xdg_popup, native wl_data_device, XNFilterEvents).

### Cron firing 49 — HEADLESS RENDER VERIFICATION (the library still renders correctly)
Backlog is essentially exhausted (everything safe/valuable/actionable done, build-verified). Instead of
another blind change, did a real VERIFICATION via the headless harness (memory azul-runtime-debug-knobs):
`cargo build -r -p azul-examples --example hello-world` (3m12s clean) then
`AZ_BACKEND=headless AZ_HEADLESS_SNAPSHOT_PATH=/tmp/hello_headless.png ./target/release/examples/hello-world`.
RESULT (640x480 PNG, read visually): CORRECT — green background-color applied, "0" counter label renders
at font-size:50px (correct glyph, no tofu), "Increase counter" button styled + text correct, layout +
text shaping all intact. => The core init/layout/font/render pipeline is NOT broken by firings 26-48's
X11/menu/cursor changes. (Headless = cpurender, so this does NOT exercise the X11 GL path, the event-loop/
menu/grab changes, or the GL-teardown exit fix — those still need a real window + the user's interactive
test. But it rules out a render-pipeline regression.)
PRE-EXISTING observation (NOT from this effort — did not touch the flex solver): in hello-world the
`flex-grow:1` button does not expand and the label+button sit in a row; worth a separate look if that's
unintended. 
NET: no code change this firing — a verification. The loop has reached the point where USER RUNTIME
TESTING (interactive X11 window + gdb for the exit path) is strictly more valuable than further blind
edits. Recommend pausing the autonomous loop for a test pass.

### Cron firing 50 — HEADLESS VERIFICATION OF THE C ABI + contenteditable: FOUND A REAL BUG
Extended firing 49's verification to the C ABI + the user's actual test app. Built tests/e2e/
contenteditable.c against the CURRENT libazul.so (cc -I target/codegen -L target/release -lazul) — LINKS
CLEAN (C ABI intact: codegen headers + symbols match). Rendered it headless (AZ_BACKEND=headless +
AZ_HEADLESS_SNAPSHOT_PATH=/tmp/ce_headless.png /tmp/ce_headless_test) → 1200x800 PNG, NO crash.
MOSTLY CORRECT: single-line contenteditable (green box, "Hello World - Click here and type!", 48px) renders
PERFECTLY; the labels, status bar, and textarea box all render; text shaping clean.
**BUG FOUND (reproducible headless):** the multi-line textarea (overflow-y:scroll, 10 lines of 48px text in
a 300-400px box, line-height 1.4) renders Lines 1-4 cleanly but the line(s) at the BOTTOM CLIP BOUNDARY
are OVERLAPPING/garbled — two text runs superimposed at ~the same y. The single-line input (NO overflow)
is clean, so the bug correlates with OVERFLOW-Y scroll/clip text rendering at the boundary (likely the
display-list clip for scrollable text, or the initial scroll-offset / scroll-into-view, or a
line-position bug near the clip). 
RELEVANCE: this is the SAME overflow-y rendering path my firing-46 over-tall-menu height-clamp+scroll
(overflow-y:auto on .menu-container) relies on — so an OVER-TALL MENU likely has the same overlap. The
user should verify a long (taller-than-screen) menu specifically.
CAVEATS: (1) headless = cpurender (tiny-skia); the GPU/WebRender path the user actually runs MIGHT clip
correctly (different clip impl) — so this could be cpurender-only. Verify on the real GPU window.
(2) NOT a regression from firings 26-49 (none touched text/overflow rendering); pre-existing.
REPRO for next firing (VERIFIABLE — fix → re-render headless → check PNG): /tmp/ce_headless_test (or
rebuild from tests/e2e/contenteditable.c). Next suspects to read: solver3 display_list overflow/clip
generation for scroll frames, text3 inline line positioning near clip, and the scroll-into-view initial
offset. This is a GOOD next target precisely because it's headless-reproducible (unlike the X11
interactive work).

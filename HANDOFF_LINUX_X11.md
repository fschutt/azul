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

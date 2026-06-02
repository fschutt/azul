# Azul Linux GUI — Session Handoff (→ Linux Mint Cinnamon / X11)

Branch: `mobile-ios-android`. This doc hands off the native-Linux desktop-shell
work (KDE Plasma / Wayland / nouveau, software-GL-capable) to a fresh session on
**Linux Mint Cinnamon** (X11 by default, Muffin WM) to re-test the same things on X11.

---

## 1. What was done this session (committed)

All on `mobile-ios-android`, NOT pushed (no push without explicit ask).

| Commit | What |
|---|---|
| `0f6af1810` | **fix(core): CssPropertyCachePtr double-free** — `ManuallyDrop<Box>` + gate free on `run_destructor`. Repro `dll/examples/css_double_drop.rs` (SIGSEGV before, clean after). |
| `6bf17075a` | **fix(desktop/wayland): render content + idle events + server-side decorations** — bundles 4 fixes (see below). |
| `8e3cac256` | **feat(desktop/wayland): honor AZ_BACKEND=cpu + prefer cpurender over software GL** — Mesa-free CPU path; PSS 64→36 MB. |
| `06ff067b6` | **fix(core): IconProviderHandle double-free** — `ManuallyDrop`+`run_destructor` gate; api.json field + codegen regen. PROVEN via `appconfig_double_drop.rs` (crash moved past `icon_provider`). **But see §4: AzAppConfig nests MORE ungated double-drop fields → systemic #23 needed.** |

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

### Systemic alternative (#23)
A codegen-generator fix (emit `ManuallyDrop` mirror fields, or `ptr::read` in `_delete`) would close the whole double-drop class without per-type `run_destructor`. Bigger/riskier; the per-leaf fixes have handled every *known* case (GlContextPtr, InstantPtr, CssPropertyCachePtr, IconProviderHandle).

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

- DONE: garbage(#11), blank(#25), close/idle(#27), decorations(#26), CssPropertyCachePtr(#15), force-CPU Wayland(#28), diagnostics removed(#13), commits(#14), double-drop scan(#29).
- IN PROGRESS: IconProviderHandle fix(#29/#16 — see §4).
- OPEN (needs decision/hardware/next session): GPU partial-present(#30), pen/touch hardware test(#31), X11 force-CPU(§4), mic-input pipeline(#17, no hw), text-input IME marshals(#18), azul-paint flex-row CSS bug(#19), AZ_DEBUG synthetic events(#20), X11 screenshot(#21), device enumeration(#22), systemic codegen fix(#23), memory trim(#24).

---

## 7b. Input / text / IME / a11y research — toward "azul-paint + contenteditable.c fully working"

User report: clicking azul-paint buttons does **nothing** (no callback response) on the Wayland window; suspects mouse-move/text-input also don't fully work; no IME; no file-drop. Goal: azul-paint fully interactive AND `tests/e2e/contenteditable.c` works (set element `contenteditable`, receive text-editing input, activate IME, position the IME cursor). Each item below = research + status + where to dig.

### A. Mouse click / hover → callback dispatch (HIGH — blocks azul-paint)
- Path: `pointer_button_handler` → `handle_pointer_button` (mod.rs ~1995): reads position from `current_window_state.mouse_state.cursor_position` (set by motion/enter), sets `mouse_state.{left,right,middle}_down`, then `process_window_events(0)` → `handle_process_event_result`. Motion → `handle_pointer_motion` sets cursor_position + cursor changes (confirmed working).
- Symptom: physical clicks don't fire button callbacks → **same root as #20** (synthetic AZ_DEBUG mouse_down/up didn't register canvas strokes). So it's NOT synthetic-only — the **hit-test → callback dispatch** itself is the gap.
- Prime suspects to debug (next): (1) the WebRender async hit-tester (`AsyncHitTester::Requested`) may never resolve/refresh so `FullHitTest` returns no node → no `On::MouseDown/Click`; (2) the display list may lack hit-test tags for the buttons; (3) `last_hovered_node` not set so click maps to nothing; (4) `process_window_events` state-diff not detecting the press. ACTION: log the hit-test result + hovered node on click; compare X11 (which uses the same `process_window_events`) to isolate Wayland-specific vs shared. Likely a shared `process_window_events`/hit-tester issue (so fixing it helps both backends).

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

### Priority for "fully working azul-paint": A (click dispatch) → B (typing) → C (IME/contenteditable) → D (file drop) → E (a11y verify). A is the keystone (hit-test/callback dispatch); it likely also unblocks #20 and is shared with X11.

## 8. Key codebase pointers

- Wayland backend: `dll/src/desktop/shell2/linux/wayland/{mod,events,defines,dlopen,gl}.rs`
- X11 backend: `dll/src/desktop/shell2/linux/x11/mod.rs`
- Backend resolution + software-GL detect: `dll/src/desktop/shell2/common/compositor.rs` (`AzBackend::resolve`, `check_gpu_blacklist`, `query_gpu_info`)
- WebRender translate / renderer options: `dll/src/desktop/wr_translate2.rs` (`default_renderer_options`, `generate_frame`, `build_image_only_transaction`)
- CPU render: `azul-layout` `cpurender` (tiny-skia `AzulPixmap`)
- Double-drop convention: every FFI leaf owning a `Box`/resource must gate its free on `run_destructor` (via `ManuallyDrop`) or a refcount/destructor-enum, else the codegen `_delete`+drop-glue double-frees when the wrapper is nested. Fixed leaves: GlContextPtr, InstantPtr, CssPropertyCachePtr, IconProviderHandle.
- Run loop: `dll/src/desktop/shell2/run.rs` (Linux `run` ~line 992; tears down `!is_open()` windows).

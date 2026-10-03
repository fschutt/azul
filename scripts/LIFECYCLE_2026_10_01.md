# LIFECYCLE - report (2026-10-01)

Branch `wt/lifecycle`, base `39092feee`. Nothing was compiled (house rule):
every touched `.rs` file parses under `rustfmt --check` (only pre-existing
formatting diffs), and the spring / ABI-hash arithmetic was checked in Python
(f32-rounded spring integration, FNV-1a test vectors, WCAG ratios).

## What was built

### 1. ABI guard (codegen + dll)
`doc/src/codegen/v2/abi_guard.rs` computes ONE hash per api.json: FNV-1a 64
over a canonical, SORTED, doc-free text of the IR - every struct (repr,
generics, fields in order with type + ref kind), enum (repr, union-ness,
variants in order with payloads), alias (target, generic args, monomorphized
layout), callback typedef, C function signature and constant. Docs, argument
names, `fn_body`, derives, modules and api.json's class order do not enter it.
Emitted from that one value:
- `dll_api_internal.rs` (libazul): `AZ_ABI_HASH` + the export
  `AzAbi_getHash() -> u64` (`no_mangle` under `cabi_export`);
- `dll_api_external.rs` / `azul.rs` (the Rust `azul` crate in `link-dynamic`
  mode AND the pre-rendered release crate, which ships the same file):
  `AZ_ABI_HASH`, the declaration and `az_abi_check()`, called by EVERY
  constructor / static-method wrapper and by `From<&str|String> for AzString`
  (RefAny::new's way in) - so the FIRST call into libazul is checked
  (`AppConfig::create` runs before `App::create`); one relaxed atomic load
  after the first comparison;
- `azul.h` (C, and C++ through it): `AZ_ABI_HASH`, the declaration,
  `AzAbi_check()` and its load-time call (GCC/Clang `constructor`; a static
  object in C++ elsewhere; MSVC C has none - call `AzAbi_check()`),
  `AZ_NO_ABI_CHECK` opts out. The block sits in `#ifndef`, so the LuaJIT/PHP
  cdef strippers drop it and keep only the declaration.
On a mismatch: `azul: ABI mismatch - this app was built against the azul ABI
<app>, but the libazul it loaded has the ABI <lib> (...) - rebuild the app
against this libazul.` and `abort()`.
Python: the extension IS libazul (same crate), it cannot disagree.

### 2. Threads of removed widgets
`layout/src/managers/thread_owner.rs` (`LayoutWindow.thread_owners`). ONE
rule, `binds_threads_to_node`: a thread added by a callback answering the
node's OWN `AfterMount` / `NodeResized` / `Updated` belongs to that node; the
dispatcher binds it (`dispatch_events_propagated`, next to the CapturePointer
seat binding; `CallbackChange::AddThread` gained `owner: Option<DomNodeId>`).
Click / timer / write-back / `BeforeUnmount` threads stay the app's (a
download outlives its button). `LayoutWindow::remap_node_ids` (the unmount
path that drops VirtualView / scroll / focus state) remaps owners with their
nodes and ORPHANS the threads of unmounted nodes - also of nodes in the
child DOM of an unmounted VirtualView (`VirtualViewManager::
nested_doms_dropped_by`) - and sends each `TerminateThread`.
`run_all_threads` drops an orphan's write-backs and retires it with the
ordinary `RemoveThread` once it finished (join returns at once) or, past 2 s,
after detaching its handle - the UI thread never waits in the 2 s join.
All workers (video stream.rs, camera, screencap, microphone, map) already
honour `TerminateThread`.

### 3. Idle leftovers
- (a) FLIP springs: `SpringCurve::is_settled` asked `|x| < 0.06 && |v| <
  0.06`; on a spring's tail `|v| ~ omega |x|` (omega 13), so the velocity
  term demanded `|x| < 0.005` and a converged state like FB3's
  `(0.016, -0.2)` counted as moving. Now: settled when the spring's energy
  bounds its future excursion under the epsilon, `x^2 + (m/k) v^2 < eps^2`
  (a fast zero crossing is not settled). Plus: a Move / Enter whose node left
  the rebuilt tree is dropped (`AnimationManager::drop_unplaced`; exits stay,
  their zombie holds the node).
- (b) `RenderImageCallback`: `LayoutWindow::image_callback_inputs` remembers
  each canvas's inputs (declared callback ImageRef hash, box bits, hidpi); a
  frame skips a canvas whose inputs are unchanged. It renders again on a new
  box / scale, a rebuild of its DOM (`remap_node_ids` drops that DOM's
  entries), or `update_image_callback` / `update_all_image_callbacks` (now
  `invalidate_*` in the shells and the e2e runner).
- (c) Debug server: `announce_debug_request` (called after each send) wakes
  every registered loop - `loop_waker::wake` (macOS NSEvent, X11/Wayland wake
  fd) and the headless condvar; `PlatformWindow::serve_debug_request_wake`
  (top of `process_timers_and_threads`, and the app-event collector) re-arms
  the debug poll at the busy rate. `DEBUG_POLL_IDLE_MS` 250 -> 2000 (safety
  net). Headless Phase 5 waits until the next timer (never under a frame)
  instead of one frame when only timers are live. The CPU `screenshot` op
  renders on the UI thread (`CallbackInfo::render_screenshot`) and sends
  `DebugResponseData::PendingScreenshot`; `into_ready` encodes the PNG on the
  receiving thread (the HTTP thread).
- (d) Monitor change: `LayoutWindow::frame_drivers_off_pace` +
  `PlatformWindow::repace_frame_drivers` (first thing in
  `arm_animation_drivers_if_needed`, which every pass ends with) re-register
  the running CSS driver / caret tween / scroll physics at the current frame
  interval. macOS root cause found on the way: `windowDidChangeScreen:` never
  detected the new monitor (only `windowDidChangeBackingProperties:`, sent
  only on a SCALE change, did) - a move between two 2x displays kept the old
  monitor id, display link and frame interval. It now runs `handle_dpi_change`.

### 4. Segmented "Dark" segment
Flat's dark selected pair is `system:accent` under `system:accent-text`: the
accent is the user's (Graphite = a neutral grey, (140,140,144) in the dark
appearance), the ink AppKit reports is always white -> 3.4:1, "white text on
a light-grey face" (light mode uses a fixed blue, hence dark-only).
`SystemColorRef::resolve_for_theme` now resolves `system:accent-text` as a
PAIR (`readable_accent_ink`): the platform ink when it reads on the accent
(4.5:1 on a neutral accent, 3:1 on a coloured one), else black/white,
whichever reads better. Fixes every accent pair at once. VERIFY the
hypothesis on the user's Mac: `defaults read -g AppleAccentColor` = -1 is
Graphite. Visible side effect: black ink on KDE Breeze #3daee9 (2.5:1) and
Ubuntu orange (2.8:1) accents.

## Commits (base 39092feee)
RED -> GREEN pairs; `chore(lifecycle): progress` commits omitted.
- 9d8d69e45 / 3ba642c65 - ABI guard
- e379b2fb4 / 6c3ffd0da - threads of unmounted nodes; ea93eb0bf e2e manager
  accounting (thread_owner classified)
- ee3c8f0fc / cead23ffb - spring settle; 147c624e4 / b53b691f8 - unplaced FLIPs
- 104fa4da4 / b84a61c82 - RenderImageCallback memo
- 44199da3b / 91800c36b + 52450b5ec + 4a7ee6d5a - debug wake
- d9d76151d (refactor) + 57867bb64 / 86fb64407 - PNG off the UI thread
- dd959d321 / ac42c3b6f - monitor change re-paces drivers (+ macOS screen move)
- 2239d3f18 / 75dd9c370 - readable accent ink (segmented)

## api.json
No api.json change. Do NOT add `AzAbi_getHash` to api.json: it is generated
for every output by the codegen, and an api.json entry would change the hash
it reports. New Rust-only items: `CallbackInfo::render_screenshot` (returns
`AzulPixmap`, not FFI), `LayoutWindow::{add_thread_owned_by,
invalidate_image_callback, invalidate_all_image_callbacks,
frame_drivers_off_pace, image_callback_inputs, thread_owners}`,
`azul_layout::e2e::{add_debug_request_waker, announce_debug_request,
take_debug_request_wake, DebugResponseData::{pending_screenshot, is_pending,
into_ready}}`, `azul_css::props::basic::color::readable_accent_ink`.

## Least sure to compile
- `doc/src/codegen/v2/abi_guard.rs` tests: `super::super::bug_classes::ir()`
  (made `pub(super)`); `CodegenConfig` imported both by glob and explicitly.
- Generated Rust: the binding-side `extern "C" { pub fn AzAbi_getHash() }`
  block next to the big one; `::std::format!` / `::std::eprintln!` inside the
  `dll` module (std is in the extern prelude of azul-dll and the pre-rendered
  crate). `az_abi_check` / `az_abi_mismatch_message` may be dead-code-warned
  in `memtest.rs`.
- `azul.h`: the C++ anonymous-namespace guard object inside `extern "C" {`.
- `dll/src/desktop/shell2/common/event.rs` dispatcher: the guarded move
  pattern `AddThread { thread_id, thread, owner: None } if binds_threads =>`.
- `layout/src/window.rs`: `invoke_image_callbacks_into_overlay` inserting
  into `image_callback_inputs` while `info` borrows `image_cache` (disjoint
  fields); `thread_owners.remap_node_ids(.., &dropped_child_doms)`.
- `layout/src/managers/thread_owner.rs::poll_orphan`:
  `inner.thread_handle.take()` on `Box<Option<JoinHandle<()>>>`.
- `layout/src/e2e/full.rs`: `PendingScreenshot` (`Arc<Mutex<Option<
  AzulPixmap>>>` in a `#[derive(Debug, Clone)]` enum sent over mpsc - needs
  `AzulPixmap: Send`, which `PixBuf`'s unsafe impl gives); the new
  `PendingScreenshot` arms in the four consumers.
- `dll/src/desktop/app_events.rs`: `#[cfg]` on a struct-literal field.
- `dll/src/desktop/shell2/headless/mod.rs` test: `Thread::create(.., ThreadCallback::new(probe_worker))`
  with `azul_layout::thread::ThreadSender` in the worker signature.

## Test commands for the parent
```
cargo run -r -p azul-doc codegen all                               # regenerate first
cargo test --release -p azul-doc abi_guard
cargo test --release -p azul-doc bug_classes                       # emitter lint + exports
cargo test --release -p azul-dll --lib --features build-dll abi_guard_tests
cargo test --release -p azul-dll --lib --features build-dll the_worker_of_a_node_that_unmounts
cargo test --release -p azul-layout --lib thread_owner
cargo test --release -p azul-layout --lib e2e_manager_accounting
cargo test --release -p azul-layout --lib -- debug_request_wake_tests pending_screenshot_tests debug_poll_pace
cargo test --release -p azul-core --lib animation
cargo test --release -p azul-css --lib the_accent_ink_reads_on_any_desktop_accent
cargo test --release -p azul-layout --test all a_render_image_callback_with_unchanged_inputs
cargo test --release -p azul-layout --test all a_window_paces_at_its_monitors_refresh_rate
cargo test --release -p azul-layout --test all flat_and_flora_widgets_follow_the_light_and_dark_theme
```
C side: `clang -fsyntax-only -std=c99 -x c azul.h` and the C++
header checks, then the C hello-world link (the load-time check runs).
RED pass: revert each GREEN commit above; the stub-based REDs compile.
E2E: `scripts/idle_cpu_probe.py --sample` on AzWidgets / AzReview at rest
(expect no frames: FLIPs settle, canvases are not re-invoked), and
`get_animations` -> `active: 0` after a second at rest.

## What is left
- ABI guard for the other managed bindings with a load path (Lua, Ruby, C#,
  Node, ...): they declare `AzAbi_getHash` (cdef) but do not compare it yet.
- Windows: `loop_waker::wake` does not reach the Win32 loop, so there the
  first debug request after a quiet second waits for the 2 s safety net.
- The NATIVE screenshot op still PNG-encodes inside the dll hook (UI thread).
- `ScrollPhysicsState::scroll_physics.timer_interval_ms` keeps its armed
  value after a re-pace (only the first-step fallback reads it).
- X11 / Win32 / Wayland monitor moves: they write `monitor_id` already; the
  re-pace runs at the end of the pass that follows (not verified on device).
- The 476 FLIPs were measured headless (FB3); the settle fix explains values
  that converged but never finished. Re-measure with `get_animations` to
  confirm nothing else re-seeds them.
- Item 4 is a hypothesis about the user's accent (Graphite); the contrast
  rule fixes any light / neutral accent either way.
- Duplicates noticed: none new. `poll_orphan` deliberately reuses
  `ThreadInner`'s own finish check instead of a second one.

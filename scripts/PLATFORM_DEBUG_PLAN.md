# Platform-layer debug + self-test plan (0.2.0 post-release)

**Status:** IN PROGRESS — Phases 0–3 done; the verifiable platform fixes are
committed (compile-verified, awaiting per-OS RUNTIME validation); Phase 4 (run
per-OS + compare logs) is the remaining step and needs a rebuild + a real
Linux/Windows box.
**Branch:** `mobile-ios-android` (never master; never force-push; always `--release`/`--profile`).

---

## CURRENT STATUS (update as of the platform-debug sessions)

### Fixed + compile-verified (need runtime confirmation on the rebuild)
| Item | What | Verified | Validate by |
|------|------|----------|-------------|
| **C1** | macOS demos aborted: `render_api.unwrap()` None in CPU mode → guarded | builds | run any macOS demo (should not abort on resize) |
| **B1** | Linux `libazul.so` ~80 undefined `Py*` → one dylib + weak Py stubs; CI `nm` gate + C-link test | C stubs compile | CI `nm`/link step green; link a C app vs released `libazul.so` |
| **F3** | Wayland `wl_display_get_registry` is an inline wrapper (not exported) → marshal it; + X11 fallback | linux x-check | run a demo under Wayland (`WAYLAND_DISPLAY` set) |
| **F4** | window drifted on its own: OS-reported geometry echoed back. `WindowStateSource{App,Os}` + `update_window_state`; wired X11/macOS/Windows; Wayland immune | linux+win x-check, mac build | window stays put on Mint/Cinnamon; no ConfigureNotify storm in the `[x11 ev]` trace |
| **R1** | CPU/llvmpipe "GLSL 1.50 not supported": SVG/FXAA shaders compiled in software mode → gate behind `RendererType::Hardware`; X11/Wayland detect software GL → `Software` | core check, linux x-check | run a demo on llvmpipe (no shader-compile error) |
| **R2** | X11 black/white flicker: skip-render fast-path had no pixmap to blit → guarded; + warn logs for pixmap-vs-window mismatch | linux x-check | resize a window smaller than content; watch the `[x11 cpu]` warns |
| **Phase 2** | `AzCapability_*` probe C-API (camera/mic/audio/udp/sensors/gamepad/geo/keyring/biometric/video) — typed `{available,backend,reason}`, never panics | codegen+build, table prints | the self-test capabilities table |

### Diagnostics added (Phase 1) — how to read the per-OS logs
Always-on `plog_*!` (route to the `log` facade; the self-test installs a logger).
Grep the self-test log / demo run (with logging enabled) for these tags:
- `[x11 ev] raw <name> (#n)` / `[win32 ev] raw <name>` / `[macos ev] raw <name>` — every raw OS event → spot event storms / wiring.
- `[x11 ev] ConfigureNotify x/y/w/h/send_event` — F4: `send_event=0` = frame-relative coords.
- `[x11 cpu] ...` — R2: pixmap-vs-window mismatch / fallback-paint.
- `[camera] ...` `[udp] ...` `[audio] ...` `[gamepad] ...` `[sensors] ...` — backend open/format/availability.
- `[cap] <subsystem> AVAILABLE|unavailable via <backend>` — the capability table.

### Still OPEN — need Phase-4 per-OS runtime data (NOT blind-fixable safely)
- **R3** Windows first-frame blank until resize — the initial present *does* run
  (`render_and_present(true)` at window create); it is an async GPU first-frame
  timing detail. Capture the `[win32 ev]` trace around startup.
- **R4/R5** fonts blocky / sans-serif missing (Windows) — font rasterisation /
  system-font-family wiring; needs a Windows run to see which face loads.
- **R6/R7** content offset / images don't update (Windows) — CPU framebuffer
  update path.
- **F5/F6** Windows text-selection / vault double-auth — Windows runtime.
- **C2/C3** Windows worker "threads should not terminate unexpectedly" — `panic="abort"`
  means it can't be caught, only LOCALISED via the backend logs (which call aborted).
- **C5** Linux gamepad `double free in tcache2` — inside gilrs/libudev (a C abort,
  uncatchable); the `[gamepad] initialising gilrs` log brackets the crash site.
- **M1** RAM 90–120 MB — needs runtime profiling.

### Phase-4 runbook (for the agent restarted on the Linux/Windows box)
1. Build + run `azul-self-test` (it self-describes on stdout). For the full device
   probes leave the window on; for a headless box set `AZUL_SELFTEST_NO_WINDOW=1`.
   Log goes to `$TMPDIR/azul-self-test.log` (override `AZUL_SELFTEST_LOG`).
2. To get azul's platform traces in a *demo* run, build with the `logging` feature
   and install an env_logger (the self-test already does); set `RUST_LOG=trace`.
3. Compare `selftest-<os>.log` across machines; the tags above localise each bug.
4. Fix the OPEN items against the real logs, then update this table.

## Source of truth (in-repo copies of the user's bug reports)

- [`scripts/problems/problems-windows.txt`](problems/problems-windows.txt)
- [`scripts/problems/problems-linux.txt`](problems/problems-linux.txt)
- [`scripts/problems/problems-macos.txt`](problems/problems-macos.txt) (387 KB — Apple crash dumps; the
  actionable part is the repeated panic at the top, the rest is symbolicated stacks)

These are the per-platform results of running the 0.2.0 demo binaries (azul-paint, azul-maps,
azul-vault, azul-camera-app, azul-screenshare-app, azul-video-app, azul-spirit-level, azul-gamepad,
azul-meet). The goal is to fix the **platform layer** — `dll/src/desktop/shell2/{macos,linux,windows}`
(windowing/compositor/events) + `dll/src/desktop/extra/*` (camera/audio/udp/sensors/gamepad/
geolocation/...). The `headless` window + `layout`/`core`/`cpurender` are 100% cross-platform and are
NOT the suspects.

## The user's four work items

- **(a) Investigate all the bugs** → the inventory below.
- **(b) Insert LOTS of logs** into the default apps, *especially the azul-dll platform layer*.
- **(c) Review bad wiring** — e.g. camera `NV12 → CPU RGB`, and **proper API return codes when a
  feature is unavailable on a target** (a desktop with no motion sensor must return an error, not
  crash/panic).
- **(d) Build `azul-self-test`** — a CLI that exercises every platform API, prints to stdout (mic
  packets as dots, etc.), logs to a file, and exits. Runs unattended on desktop AND mobile.

Then: run the self-test on each OS, **compare the log files**, and clean up the remaining
misconceptions/bugs.

> NOTE (user): "We probably need to validate and research the APIs again." → Phase 0 below.

---

## Bug inventory (categorized)

### P0 — crashes on startup (every demo unusable)

| # | Platform | Symptom | Root cause / location |
|---|---|---|---|
| C1 | **macOS** | EVERY demo aborts: `Option::unwrap() on None` | `dll/src/desktop/shell2/macos/events.rs:963` — `self.common.render_api.as_mut().unwrap()` in `handle_compositor_resize`; `render_api` is `None` in CPU mode → panic on first resize. **9 `render_api.unwrap()` sites** exist in `shell2/` — all latent. (Matches the 2026-05-13 `handle_compositor_resize` note.) |
| C2 | **Windows** | video/camera/vault/meet panic: "threads should not terminate unexpectedly" | a worker thread panics (likely camera/audio/video backend init) and the main loop's join detects it. Needs the worker to return `Result`/log, not panic. |
| C3 | **Windows** | azul-meet, azul-maps crash immediately, no output | TBD — likely same render_api/None or a platform-init panic. Self-test + logging will localize. |
| C4 | **Windows** | az-gamepad shows window 0.1s then crashes; az-spirit-level crashes fast | gamepad/sensor backend init panics when no device present → must return error (item c). |
| C5 | **Linux** | azul-gamepad: `double free in tcache2` | gamepad backend (desktop `gilrs`?) double-free; `dll/src/desktop/extra/gamepad/desktop.rs`. |

### P0 — build/packaging wiring

| # | Platform | Symptom | Root cause / location |
|---|---|---|---|
| B1 | **Linux** | `libazul.so: undefined reference to PyTuple_SetItem, PyObject_*, _Py_*` (~80 symbols) → C apps fail to link | the shipped default `libazul.so` is pulling **pyo3 / python-extension** symbols without linking libpython. The default `build-dll` must NOT include the python extension. Investigate feature unification (a workspace member or default enabling `python-extension`/pyo3). **Breaks ALL Linux C/C++ linking.** |
| B2 | **Linux** | every demo 130+ MB; "entire remill linked?" | NOT remill (link-static demos have no remill — verified earlier). It's **debuginfo**: demos build with the workspace `[profile.release]` which sets `debug = 1`. Build demos/mobile with `prod-release` (debug=0) or strip. (Mirrors the 1 GB `.a` fix.) |
| B3 | **Linux** | downloaded apps not executable | CI must `chmod +x` demo binaries before packaging / the release step must preserve the bit. |

### P1 — rendering / compositor (CPU backend)

| # | Platform | Symptom | Likely cause |
|---|---|---|---|
| R1 | **Linux** | `SVG/FXAA shader compile error: GLSL 1.50 not supported` (llvmpipe/swrast = GLES only) | azul is compiling **desktop GL shaders in CPU mode**. CPU mode must not touch GL at all (no SVG/FXAA GL path). Gate shader compile behind the GL backend. |
| R2 | **Win/Lin** | black↔white flicker; bg goes black when window < content | **uncleared backbuffer** flipping each draw. The CPU compositor must clear to the window bg every frame (and handle window-smaller-than-content). |
| R3 | **Windows** | first frame blank / white; UI only shows on resize | first-frame present path doesn't draw until a resize triggers `handle_compositor_resize`. Initial layout/present not firing. |
| R4 | **Win** | fonts blocky / bad AA / bad subpixel / hinting | CPU rasterizer AA quality (`cpurender`) — but note this is cross-platform code; verify it's not a platform glyph-cache wiring issue first. |
| R5 | **Win** | fonts don't *render* though layout works; only serif, not sans-serif | font **rasterization/loading** at the platform glyph layer: layout measures (font metrics load) but the rasterized glyphs don't reach the framebuffer; sans-serif face missing on Windows. |
| R6 | **Win/Lin** | body padding offset; window content offset | window origin / inset (titlebar/decoration inset) miscomputed on the platform side. |
| R7 | **Win** | images don't draw/update (paint, video) | image cache → CPU framebuffer dirty/update path on Windows. |

### P1 — platform features (the `extra/` layer)

| # | Area | Symptom | Location |
|---|---|---|---|
| F1 | camera | no video, white screen; NV12→RGB conversion suspect | `dll/src/desktop/extra/camera/{windows,v4l2,avfoundation}.rs` + the NV12→RGB wiring (item c). |
| F2 | feature absent | gamepad/motion/camera crash on a machine without the device | every `extra/*` backend must return a typed "unavailable on this target/device" error, surfaced as a proper API return code — NEVER panic (item c). Audit all `extra/*/mod.rs` entry points. |
| F3 | Linux Wayland | `Failed to load libwayland-client: wl_display_get_registry SymbolNotFound` | `shell2/linux/wayland` dlopen/symbol table — wrong symbol name or version; fall back to X11 cleanly. |
| F4 | Linux X11 | window moves randomly off-screen | `shell2/linux/x11` initial window placement. |
| F5 | text selection | not working on Windows (works macOS) | platform text-input/selection wiring on Windows. |
| F6 | vault | Windows PIN dialog requires "tap unlock again" (double-auth) | biometric/keyring `extra/biometric/windows.rs` returns before completing → app re-prompts. |

### P1 — resource usage

| # | Symptom | Suspect |
|---|---|---|
| M1 | RAM 90–120 MB (regression from <20 MB) on Win/Lin | leak or per-frame allocation in the platform compositor/present path; correlate with R2 (backbuffer realloc each frame?). Partly B2 (huge binary mapped). |

---

## Phase 0 — validate + research the platform APIs (per user)

Before fixing, re-validate each backend against current OS APIs (they drift), and confirm the
intended data path. Produce a short note per area in `scripts/problems/api-validation.md`:

- **camera**: AVFoundation (mac) / Media Foundation (win) / V4L2 (linux) frame formats; the
  NV12/YUYV → RGB(A) conversion the CPU compositor expects; who owns the conversion.
- **audio**: cpal (desktop) mic/sink packet format + sample rate; AAudio (android) / AVFoundation (ios).
- **udp**: `AzUdp` send/recv API shape + the self-test loopback contract.
- **sensors / gamepad / geolocation**: availability detection + the "unavailable" return path.
- **video_codec**: current stub status (it's a stub on all platforms — confirm the API contract).

---

## Phase 1 — logging (item b)

The shipped lean build's `log_*!` macros are **no-ops** (gated on `is_debug_enabled()`, which is false
without the debug server). So platform logging needs an **always-on** path that works in the lean
default build:

- Use the `log` crate (the `logging` feature is in `default`) for all new platform-layer traces:
  `log::info!`/`warn!`/`error!` in every `extra/*` backend entry/exit + the `shell2/*` compositor/
  present/resize/event paths. Tag with the subsystem (`[camera]`, `[udp]`, `[compositor]`, ...).
- `azul-self-test` (and optionally a `AZ_PLATFORM_LOG=1` env in the demos) installs a stderr+file
  logger so the traces actually print. Keep traces cheap (no per-pixel logging; per-frame is fine
  behind a counter).
- Add a one-shot "platform capabilities" dump at App startup (which backends initialised, which
  returned unavailable) — this is half of the self-test's value and helps the log comparison.

## Phase 2 — wiring + return codes (item c)

- **Audit every `extra/*/mod.rs`** public entry point: replace `unwrap`/`expect`/`panic` with a typed
  result; define/........use a `PlatformFeatureError` (or reuse the existing API error enum) variant
  for `Unavailable { feature, target }`. Desktop-with-no-motion-sensor → `Err(Unavailable)`.
- **Fix the camera NV12→CPU-RGB path** (F1): confirm the source format per platform, do the
  conversion once in a well-tested place, log dimensions+format.
- **Guard the 9 `render_api.unwrap()` sites** (C1): in CPU mode `render_api` is `None` — every site
  must early-return / use the CPU present path instead of unwrapping.
- **Worker threads must not panic** (C2): camera/audio/video worker loops return `Result` + log;
  the join site surfaces an error, never aborts the process.

## Phase 3 — `azul-self-test` CLI (item d)

New crate `examples/azul-self-test/` (bin + android-cdylib like azul-maps/azul-paint, so it runs on
mobile too). It runs a fixed sequence, prints human-readable progress to stdout, writes a structured
log to `azul-self-test.log` (path via `--out` / platform-appropriate default), and **exits with a
non-zero code if any required probe hard-fails** (unavailable ≠ fail; crash/panic = fail):

1. **capabilities** — enumerate which platform backends are present/available (no side effects).
2. **camera** — open default device, grab N frames, print `frame WxH fmt=NV12 → RGB ok` per frame;
   on no-camera → log "unavailable" and continue.
3. **microphone** — open default input, read M packets, print each packet's RMS as a row of dots
   (`....::::####`) to stdout so you can *see* audio without a UI.
4. **audio sink** — play a short tone (optional / gated, may be silent in CI).
5. **UDP loopback** — bind two ports on localhost, send a data packet + a synthetic "video frame"
   from A→B via `AzUdp`, verify B receives the bytes intact.
6. **sensors / gamepad / geolocation** — single read; print value or "unavailable on this target".
7. **keyring / biometric** — set+get a throwaway secret (gated; may prompt).
8. summary table + exit code.

Minimal interaction (it should run unattended in CI / on a plugged-in phone via the debugger). On
mobile it logs to a file the debugger can pull and then exits.

## Phase 4 — compare + fix

Run `azul-self-test` on Windows/Linux/macOS/android/iOS, collect the logs into
`scripts/problems/selftest-<os>.log`, diff them, and fix the divergences + the inventory above.

---

## Platform-layer file map (implementation anchors)

```
dll/src/desktop/shell2/macos/{events.rs,mod.rs,...}     # macOS windowing/compositor (C1 @ events.rs:963)
dll/src/desktop/shell2/linux/{x11,wayland}/...          # F3 (wayland symbol), F4 (x11 offscreen)
dll/src/desktop/shell2/windows/...                      # C2/C3/R2/R3/R5/F5
dll/src/desktop/shell2/common/compositor.rs             # R2 (backbuffer clear), R3 (first present)
dll/src/desktop/extra/camera/{windows,v4l2,avfoundation,android}.rs   # F1 + NV12→RGB
dll/src/desktop/extra/audio/{cpal_mic,cpal_sink,alsa,aaudio,avfoundation_*}.rs
dll/src/desktop/extra/udp/mod.rs                        # AzUdp (self-test loopback)
dll/src/desktop/extra/{sensors,gamepad,geolocation,keyring,biometric}/*.rs   # F2 return codes
dll/Cargo.toml + build.rs                               # B1 (python symbols), B2 (debuginfo)
.github/workflows/rust.yml (build_demos)                # B2/B3 (prod-release + chmod +x)
```

## Sequencing

Phase 0 (research) → Phase 1 (logging) + Phase 3 (self-test) in parallel → Phase 2 (return codes /
wiring, informed by the logs) → Phase 4 (compare + fix). The B1 (python-symbol) and C1
(render_api unwrap) bugs are independent P0s that can be fixed immediately and are the highest
leverage (B1 unblocks all Linux C linking; C1 unblocks all macOS demos).

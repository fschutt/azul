# AzMeet: high CPU while the camera / screen-share tile is live (macOS)

Investigation date: 2026-08-22. Read-only desk check of the worktree
`debug-slider-scroll-2026-08-22` (no build, no run). All line numbers refer to
that worktree.

## Symptom (user, verbatim)

> "AZMEET: High CPU usage — can't we request a smaller image from the camera
> already if we know how big we need the image? rescaling. Same for screenshare."

## Status

**Root cause confirmed by reading, magnitude needs measurement.** The user's
diagnosis is right and is only the first half of it:

1. The macOS camera backend ignores the requested size and never sets a
   session preset, so AVFoundation delivers its default `High` format
   (1280×720 on older FaceTime cameras, 1920×1080 on every Apple-silicon
   MacBook / Continuity Camera) for a tile that is 300×200 CSS px
   (600×400 device px on Retina). Nothing downscales until the very last step.
2. Every frame is then copied and/or channel-swizzled **six times at full
   resolution** between the capture callback and the pixmap, four of them on
   the main thread, two of them swizzles that undo each other
   (BGRA → RGBA → BGRA → RGBA). At 1080p that is ~8.3 MB per pass, 30 times a
   second.
3. The screen share is capped at 1280×720 by a hard-coded constant (so it is
   "only" 3.7 MB per pass) but goes through the identical pipeline, and when
   the shared display contains the AzMeet window itself it never goes idle
   (hall-of-mirrors feedback, see finding 13).

Back-of-envelope for a 1080p camera on the CPU backend (the desktop default,
`dll/src/desktop/shell2/common/compositor.rs:169-176`): ~15–25 ms of
main-thread work per frame plus ~5–8 ms on the capture dispatch queue plus two
8 MB memcpys on the worker thread, i.e. roughly one full core at 30 fps,
doubling with screen share. These are estimates from the code; see "How to
verify" for the measurement.

## Findings

### A. The requested size is dropped on the floor (macOS camera)

1. `dll/src/desktop/extra/camera/avfoundation.rs:214` —
   `pub fn open(index: u32, _width: u32, _height: u32)`: both size arguments
   are unused. `AVCaptureSession::new()` at `:233` gets no `setSessionPreset`,
   no `activeFormat`, and the only `videoSettings` key set (`:239-246`) is the
   pixel format. Consequence: AVFoundation's default preset
   (`AVCaptureSessionPresetHigh`) decides the resolution; the first-frame log
   line (`:89-92`) will show `1920x1080` on a modern Mac. `CameraConfig.fps`
   (`core/src/camera.rs:101`) is likewise never applied
   (`activeVideoMinFrameDuration` is not set), so the device runs at its own
   default rate.
2. The widget *does* send a size: `layout/src/widgets/camera.rs:132-136`
   (`frame_dims`, default 640×480 when the config says 0) →
   `camera.rs:180` `(backend.open)(0, w, h)`. The AzMeet demo uses
   `CameraConfig::default()` (`examples/azul-meet/src/lib.rs:120`), so 640×480
   is requested and ignored. The `CaptureVTable::open` contract
   (`layout/src/widgets/capture_common.rs:144-147`) documents the size as a
   request; macOS is the backend that breaks it.
3. The widget never tells the backend its *laid-out* size either. There is no
   `NodeResized` handler on `CameraWidget` / `ScreenCaptureWidget`
   (`camera.rs:120-127`, `screencap.rs:112-119` register only `AfterMount`),
   and `CaptureVTable` has no reconfigure entry. Compare `VideoWidget`, which
   already does exactly this: `layout/src/widgets/video.rs:163-170` registers
   `NodeResized`, `video.rs:306-329` (`video_on_resize`) sends
   `ThreadSendMsg::Custom(RefAny::new((w, h)))` to the worker, and the decode
   worker scales every frame to that target off the main thread
   (`dll/src/desktop/extra/video_codec/stream.rs:94`, `:186-199`, `:266-271`,
   scaler `scale_frame_bilinear` at `:298-340`). Camera and screencap were
   cloned from the same template but without that piece.
4. Other backends, for the record: Linux v4l2 honours the size
   (`camera/v4l2.rs:257-271`, `VIDIOC_S_FMT` at `:319-323`), Android honours
   it (`camera/android.rs:46-48`, `AImageReader_new` at `:83-85`), Windows
   ignores it **and** asks nokhwa for `AbsoluteHighestFrameRate`
   (`camera/windows.rs:34-36`) — the same bug class, potentially worse.
   iOS shares `avfoundation.rs` (`camera/mod.rs:14`) and inherits bug 1.
   Linux PipeWire screen capture ignores the size (`screencap/linux.rs:913`,
   full-display frames).

### B. Screen share: size is honoured, but from a constant

5. `dll/src/desktop/extra/screencap/macos.rs:272-378` does use
   `width`/`height`: `setWidth`/`setHeight` at `:367-368` (falling back to the
   display's point size at `:343-347`), BGRA at `:369`, `setQueueDepth: 5`
   at `:371`, `minimumFrameInterval = 1/30` at `:372-378`. But the widget
   always passes `DEFAULT_W × DEFAULT_H = 1280×720`
   (`layout/src/widgets/screencap.rs:34-35`, `:155`), never the tile size,
   and `ScreenCaptureConfig` (`core/src/screencap.rs:37-46`) has no
   width/height field at all. `config.source` and `config.fps` are ignored
   too: `screencap.rs:155` opens display `0` regardless of
   `ScreenCaptureSource::Display(n)`, and 30 fps is hard-coded in the backend.

### C. Six full-resolution passes per frame

Per frame of W×H (8.3 MB at 1080p, 3.7 MB at 720p):

6. **Capture callback (dispatch queue "azul.camera")** —
   `avfoundation.rs:74-85`: `vec![0u8; w*h*4]` (a fresh calloc every frame →
   page-faulting 8 MB of zero pages) then a scalar, bounds-checked per-pixel
   BGRA→RGBA loop; `:94` replaces the slot's previous `Vec` (free + alloc
   churn). Identical code in `screencap/macos.rs:186-197`.
7. **`read()` on the worker thread** — `avfoundation.rs:292-293`
   `out.extend_from_slice(&slot.rgba)` copies the frame *while holding the
   mutex* (the capture queue blocks on it meanwhile). Same at
   `screencap/macos.rs:480-481`.
8. **Worker → main hand-off** — `camera.rs:195-199` builds the `VideoFrame`
   with `buf.clone().into()` (a second 8 MB copy; `buf` is discarded and
   refilled by the next `read` anyway). Same at `screencap.rs:169-172`. The
   message goes through an **unbounded** `std::sync::mpsc` channel
   (`layout/src/thread.rs:192-199`); `run_all_threads` drains everything
   queued each tick (`layout/src/window.rs:10082-10105`), so if the main
   thread falls behind, every backlog frame is still fully processed (and
   each holds 8 MB until then).
9. **Writeback `present_frame`** — `capture_common.rs:99-106`:
   `frame.bytes.clone()` (third copy, main thread) into a `RawImage` tagged
   `RGBA8` with `premultiplied_alpha: false`. `invoke_on_frame`
   (`capture_common.rs:69`) deep-clones the whole frame again when an
   `on_frame` hook is set (AzMeet sets none for video, so this one is
   currently free). `get_node_id_of_root_dataset`
   (`layout/src/callbacks.rs:2768-2808`) scans every node of every DOM and
   clones each dataset `RefAny` per frame — O(nodes) atomics, minor.
10. **`ImageRef::new_rawimage` → `load_rgba8`** —
    `core/src/resources.rs:2297-2300` routes `RGBA8` to `load_rgba8`
    (`:2428-2466`), which swizzles **back to BGRA** and, because
    `premultiplied_alpha` is `false`, runs `premultiply_alpha` on every pixel
    (`:2453-2462`) — three integer multiply/divides per pixel for an image
    whose alpha is always 255 (every backend forces it:
    `avfoundation.rs:83`, `screencap/macos.rs:196`, `v4l2.rs` rgb24→rgba,
    `android.rs:195`, `linux.rs:1168-1190`). The zero-copy branch that
    exists for this exact case — `load_bgra8` with `premultiplied_alpha ==
    true`, `resources.rs:2627-2640` ("DO NOT CLONE THE IMAGE HERE!") — is
    never reached. Net: the capture callback's BGRA→RGBA swizzle (finding 6)
    is undone here.
11. **CPU rasterizer `render_image`** — `layout/src/cpurender/raster.rs:3684`.
    The stored descriptor is `BGRA8` (see 10), so the `BGRA8` arm at
    `:3735-3747` allocates another W×H×4 `Vec` and swizzles the **entire
    source** back to RGBA with per-byte `push`, on every repaint of the tile,
    before the nearest-neighbour blit (`:3797-3853`) samples only
    600×400 of those 2 M pixels. (The comment on the `RGBA8` arm at
    `:3721-3727` says capture frames arrive as RGBA8 — they do not, because
    of 10.) Nearest-neighbour at a 3.2× downscale ratio also aliases badly,
    so the tile looks worse than a 640×360 capture would.
12. **Present** — the damage machinery is fine: the ImageRef identity change
    makes `is_visually_equal` report the tile only
    (`layout/src/solver3/display_list.rs:1416-1425`), `apply_image_change`
    takes the Paint tier and patches the DL in place
    (`window.rs:4895-4926`), `render_frame` repaints just those rects
    (`dll/src/desktop/shell2/headless/mod.rs:371-`), and
    `CPUView::update_framebuffer` copies only the damaged rows
    (`dll/src/desktop/shell2/macos/mod.rs:2338-2420`). **But** `CPUView::drawRect`
    then memcpys the *whole* framebuffer into the `NSBitmapImageRep`
    (`macos/mod.rs:1612-1618`) and draws the full bounds (`:1620-1623`)
    regardless of the dirty rect: 2200×1440×4 ≈ 12.7 MB per presented frame
    for the demo's 1100×720 Retina window. Not capture-specific, but the
    capture turns it into a 30 Hz cost.

### D. Smaller things found on the way

13. **Screen-share feedback loop.** The `SCContentFilter` excludes no windows
    (`screencap/macos.rs:350-356`, `excludingWindows: []`). When the shared
    display shows the AzMeet window, each tile repaint is a screen change, so
    ScreenCaptureKit emits a new frame, which repaints the tile, … — a steady
    30 fps even on an otherwise idle desktop. (The doc comment at `:20-21`
    assumes screens "only produce frames on change".)
14. **8 ms polling in `read`.** `avfoundation.rs:284-298` and
    `screencap/macos.rs:472-486` poll the slot 120 × 8 ms. CPU-wise this is
    negligible (≤125 wake-ups/s) but it adds up to 8 ms of latency per frame,
    and the 960 ms bound turns a camera stall (sleep/wake, Continuity
    reconnect) into `(0,0)` → worker exit → tile frozen for good
    (`camera.rs:192-194`). The Linux screencap backend already does it right
    with a `Condvar` (`screencap/linux.rs:1126-1137`).
15. **Idle screen re-serve creates a new image every second.**
    `screencap/macos.rs:487-494` re-returns the previous frame on timeout as a
    *new* buffer → new `ImageRef` identity → a tile repaint (plus all of
    C.8-C.11) once a second for an unchanged picture.
16. **Memory in flight.** With the journal keeping 3 frames
    (`layout/src/overlay.rs:84`) plus slot, worker buffer, queued message,
    overlay and DL copies, a 1080p camera keeps ~60–70 MB of frame buffers
    alive at any time. Bounded, not a leak.

## Fix plan

Ordered by payoff per line changed. A+C alone should remove >8× of the
main-thread cost; B makes it size-correct for any tile; D and E are renderer
hygiene that every image benefits from.

### A. macOS backends: capture at the requested size, hand frames over without copying (dll only, ~1 day)

* `camera/avfoundation.rs::open`: honour `width`/`height`.
  1. Portable (macOS + iOS): `session.setSessionPreset(...)` with the
     smallest preset ≥ requested (`AVCaptureSessionPreset640x480`,
     `1280x720`, …). Needs the `"AVCaptureSessionPreset"` feature on both
     `objc2-av-foundation` dependency blocks in `dll/Cargo.toml` (`:242-257`
     and `:436-`; the crate gates it separately, see its Cargo.toml `:324`).
  2. macOS refinement: add `kCVPixelBufferWidthKey` / `kCVPixelBufferHeightKey`
     (both already available via the `CVPixelBuffer` feature of
     `objc2-core-video`) and `AVVideoScalingModeKey =
     AVVideoScalingModeResizeAspect` to the `videoSettings` dictionary, so
     the capture pipeline itself scales to the exact tile size. Apple
     documents width/height keys as macOS-only for
     `AVCaptureVideoDataOutput`; keep 1 as the fallback and log the first
     frame's real size (the existing `:89-92` line does).
  3. Apply `fps`: `lockForConfiguration` + `setActiveVideoMinFrameDuration`
     (a 300×200 tile does not need 30 fps; 15 halves everything below).
     `CaptureVTable::open` has no fps argument today — extend it (or pass a
     small `CaptureRequest { width, height, fps }` struct) and feed
     `CameraConfig.fps` / `ScreenCaptureConfig.fps` through, which also fixes
     the hard-coded 30 in `screencap/macos.rs:372-378`.
* Capture callbacks (both files): swizzle row-wise with `chunks_exact(4)`
  into a **reused** buffer (keep two `Vec`s in the slot and `mem::swap`), no
  per-frame `vec![0; …]`. `read()` takes the ready buffer with `mem::swap`
  instead of `extend_from_slice`, so nothing is copied under the mutex.
* `read()`: replace the 8 ms poll with `Condvar::wait_timeout`, notified from
  the callback (pattern: `screencap/linux.rs:1126-1137`). For the camera,
  treat a timeout as "no new frame" (keep waiting / re-serve) instead of
  end-of-stream, so a stall does not kill the worker.
* Screen share: pass the demo's own window id to
  `initWithDisplay:excludingWindows:` (the `SCWindow` whose `windowID`
  matches the NSWindow number) to break the feedback loop (finding 13); on a
  timeout re-serve the *same* buffer without bumping the sequence so the
  widget does not repaint (finding 15).

### B. Widgets: size comes from layout, not from a constant (layout crate, ~1 day)

* `CameraWidget` / `ScreenCaptureWidget`: mirror `VideoWidget` — register
  `ComponentEventFilter::NodeResized`, store the `ThreadId` in the state, and
  in the handler send `ThreadSendMsg::Custom(RefAny::new((w, h)))` with the
  node's size **in device pixels** (`get_node_size` × the window's hidpi
  factor; `video_on_resize` currently sends logical pixels, which is a 2×
  undersize on Retina — fix it there too).
* `AfterMount` already has the hit node (`examples/azul-meet/src/lib.rs:254-258`
  relies on it), so the initial `open` can use the laid-out size straight
  away; keep `config.width/height` as an override and drop the 1280×720 /
  640×480 defaults to "tile size".
* Worker loop: generalise `capture_common::terminate_requested` into a
  control poll that also yields the latest target size; on change call a new
  optional `CaptureVTable::reconfigure(handle, w, h)` (macOS camera:
  `beginConfiguration` / update `videoSettings` / `commitConfiguration`;
  SCK: `updateConfiguration:completionHandler:`; v4l2/android: reopen) and,
  where the backend cannot resize (Windows, PipeWire), downscale in the
  worker with a shared scaler. Move `scale_frame_bilinear`
  (`video_codec/stream.rs:298`) into `capture_common` (or `core::video`) and
  add a box filter for ratios > 2, so all three widgets share one scaler.
* Back-pressure: do not queue a frame while the previous one is unconsumed
  (an `Arc<AtomicBool>` cleared by the writeback, or only send after a
  `ThreadSendMsg::Tick`). Caps the unbounded channel at one frame in flight.
* While there: honour `ScreenCaptureConfig.source` (display index / window)
  in `screencap.rs:155` instead of `0`.

### C. Zero-copy hand-off and no double swizzle (no API change, ~0.5 day)

* `camera.rs:198`, `screencap.rs:171`: `bytes: core::mem::take(&mut buf).into()`
  (`U8Vec::from_vec`) — `read` refills the buffer anyway.
* `present_frame` (`capture_common.rs:91-114`): call `invoke_on_frame`
  **first** (it needs the frame), then `downcast_mut` and
  `mem::take(&mut frame.bytes)` into the `RawImage` instead of cloning; the
  `RefAny` is dropped right after the writeback anyway. Set
  `premultiplied_alpha: true` — every backend emits alpha 255, and for
  alpha 255 straight == premultiplied, so `load_rgba8` skips the per-pixel
  multiply (`resources.rs:2443-2451` branch).
* Optional, API-visible (api.json via autofix, so a separate step): give
  `VideoFrame` a pixel-format tag so macOS backends can hand BGRA straight
  through `load_bgra8`'s zero-copy branch (`resources.rs:2627-2640`). Without
  it one swizzle per frame remains (at the *reduced* size after A/B, which is
  fine).

### D. CPU rasterizer: sample, don't convert (layout/cpurender, ~0.5 day)

* `render_image` (`raster.rs:3684`): delete the "convert the whole source to
  RGBA" prologue. Sample directly from `bytes` inside the blit loop with a
  per-format channel order (BGRA: r=2, g=1, b=0; RGBA: 0,1,2; RGB8/R8 via
  a small accessor), iterating only `rect ∩ clip` (the damage rect). Removes
  one W×H allocation + swizzle from *every* image paint, not just captures.
* Add a 2×2 box / bilinear tap when the downscale ratio ≥ 2 — a quality fix
  (finding 11), cheap because it only runs over destination pixels.

### E. macOS present: stop copying the whole framebuffer per drawRect (~0.5 day, separate ticket)

* `CPUView::drawRect` (`macos/mod.rs:1541-1625`): either copy only the rows of
  `_dirty_rect` into the cached `NSBitmapImageRep`, or create the rep over
  the framebuffer's own pointer (`initWithBitmapDataPlanes:` with a non-NULL
  plane) so there is no copy at all, and draw `_dirty_rect` instead of
  `bounds`. Saves ~12.7 MB/frame on a 2200×1440 window — every repaint, not
  just capture.

## How to verify

* **Before/after CPU:** run the downloaded AzMeet demo, toggle camera, read
  `%CPU` from Activity Monitor or `top -pid $(pgrep azmeet)` for 30 s; then
  camera + screen share. Record the `[camera] avfoundation: first frame WxH`
  and `[screencap] ScreenCaptureKit: first frame WxH` log lines — after A/B
  they must read ≈ the tile's device size (600×400 on Retina), not
  1920×1080 / 1280×720.
* **Where the time goes:** `sample <pid> 10` or Instruments Time Profiler;
  expected hot symbols today: `capture_output` (swizzle), `load_rgba8`,
  `render_image`, `drawRect`'s `copy_nonoverlapping`, `extend_from_slice`.
  After the fix none of them should be visible above the noise. The
  `dispatch.threads` probe span (`window.rs:10056`) gives the per-tick
  writeback time if the telemetry feature is on.
* **Unit tests to add:**
  - `capture_common`: a shared downscale helper returns exactly `(tw, th)`
    and averages a 2×2 checkerboard to mid-grey (box filter path).
  - `camera.rs` / `screencap.rs`: `dom()` registers `NodeResized` (mirror
    `video.rs:1304`); the worker, given a fake `CaptureVTable` (registration
    tests exist at `capture_common.rs:1219-1306`), opens with the laid-out
    size and re-issues `reconfigure` after a resize message; at most one
    frame is queued while the writeback has not run.
  - `capture_common::present_frame`: the installed `RawImage` is opaque /
    premultiplied and the frame's bytes were *moved*, not cloned (assert the
    `RefAny`'s `VideoFrame.bytes` is empty afterwards, and extend the
    existing "exactly one ChangeNodeImage per frame" tests at
    `capture_common.rs:1063-1123`).
  - `cpurender`: `render_image` of a 4096×4096 BGRA source into a 10×10
    rect produces the same pixels as before (golden) and — with the
    counting allocator from the leak test — allocates no buffer larger than
    the destination.
  - `avfoundation.rs`: hardware-only; at least assert the preset/size
    selection helper picks `640x480` for a 600×400 request and `1280x720`
    for 1000×600 (pure function, unit-testable without a device).
* **Feedback loop (finding 13):** share the display showing AzMeet with an
  otherwise static desktop; `[screencap]` frame counter (add a debug log
  every 100 frames) must stop advancing once the window is excluded.

## Effort estimate

A ≈ 1 day (macOS hardware needed), B ≈ 1 day, C ≈ 0.5 day, D ≈ 0.5 day,
E ≈ 0.5 day. The A + C subset (~1.5 days) is the high-value cut: capture at
≤640×480, two copies instead of four, no premultiply pass.

## Overlaps / already fixed

* `d386614cd` (live frame survives a DOM rebuild) and `5aaed0211`
  (image-churn lint) address placeholder flicker on `RefreshDom`, not
  per-frame cost; the lint only fires for image nodes *re-initialising*, so
  it is silent here by design. No overlap with this bug.
* The damage/present rework (2026-07-03) already makes the CPU path
  tile-local (finding 12); only `drawRect`'s framebuffer copy (E) is left.
* `VideoWidget` + `video_codec/stream.rs` already contain the
  resize-to-layout + worker-side scaling design (finding 3) — B is "finish
  the port", not new architecture. The `video_on_resize` logical-vs-device
  pixel slip should be fixed in the same change.
* The Linux screencap `Condvar` read (finding 14) is the template for the
  macOS `read()` rewrite.
* Same bug class elsewhere, out of scope for the macOS report but worth a
  follow-up: Windows camera ignores the size and requests the highest frame
  rate (`camera/windows.rs:34-36`); Linux PipeWire screen capture delivers
  full-display frames (`screencap/linux.rs:913`); `ScreenCaptureConfig`
  lacks width/height and its `source`/`fps` are ignored by the widget.
* The session notes (`bugfix_wave_2026_08_22.md`) list the AzMeet privacy
  indicator and mic-meter items as done; neither touches this path.

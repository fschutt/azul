# SUPER PLAN — azul 0.2.0 "AzVideo + AzMeet" release

Goal (by morning): the 0.2.0 release page has a **working AzVideo** (Big Buck Bunny
streaming player) and a **working AzMeet** (Google-Meet-style) demo, the Wayland
0.2.0 bugs are fixed, and the API exposes the full real-time-media surface
(camera/screen/mic/audio-out/WebTransport). Push to master, cut a fresh 0.2.0
release over the old one, babysit CI to green.

Mandate: commit + PUSH each VERIFIED step. ONE build at a time (`pgrep -x cargo`).
`timeout 900 cargo … -j4`. NEVER end on a broken build. Update the memory file
(`…/memory/azul-video-and-0.2.0-session.md`) every wake-up. Cron drives continuation.

Rebuild/codegen recipe: `azul-doc autofix → patch safe → patch → normalize →
codegen all`; FFI compile check = `cargo check -p azul-dll --features build-dll`.

---

## STATE (done, committed to master)
- Video widget rearchitected like the map: off-main streaming VK decode, typed
  `VideoSource` enum in `VideoConfig` (no RefAny), fills window + composites/clips,
  plays on CPU. Commits 96d153229 · 51d6d5624 · 819ef63b4.
- Heavy-stateful message trio: **resize** (NodeResized → ThreadSender → worker
  scales, 57d53217d), **scrub/seek** (config.timestamp → merge → worker reposition,
  52eeda29b + timeline 413b5d0ec), **source-change** (config.source → merge → worker
  re-init via `'session` loop, aa9052a5c).
- `VideoEncodeCheck` (encode-capability probe) + `ScreenRecorder` (software gst-x264
  frame→MP4, verified) + FFI: 09fc8ae23 · b4e2d291b · 11f29cdbc.
- `AudioDeviceList::enumerate()` (pactl sinks/sources) + FFI: d101d015b · 5fb5ff745.
- Pattern for ALL heavy-stateful widgets (video/screen/cam/mic/audio-out): bg thread
  owns the resource; UI events are `ThreadSendMsg::Custom(..)` messages to it;
  results return as a cheap image/sample swap (no relayout thrash).

---

## ORDER OF WORK (each = its own verified commit)

### 1. API completeness (finish the real-time-media surface)
1a. **Audio device selection** — add `device: OptionString` to `AudioConfig`
    (core/audio.rs). ALSA backend (dll audio/alsa.rs opens `b"default\0"`): on Linux
    set `PIPEWIRE_NODE` env from `device` before `snd_pcm_open` (the pactl names from
    enumerate() are PipeWire node names, NOT ALSA PCM names — that's the snag).
    Same for mic capture. Expose in api.json. Verify: open-by-name succeeds / bogus
    fails (full routing needs listening — note that).
1b. **Live screen recording** — wire the screencap widget's worker to feed a
    `ScreenRecorder` when a record flag/path is set on `ScreenCaptureConfig`. Portal-
    gated (Share-click) so live verify is flaky; verify the WIRING + a non-portal
    frame source (camera/video frames → ScreenRecorder → mp4, headless).
1c. **Round-trip verify** (carry-over) — our VK decoder reads gst-x264 4:2:0 output:
    regen `/tmp/rec420.mp4`, point a throwaway run at `VideoSource::File`, framelog.

### 2. WebTransport (remove AzUdp, add AzWebTransport)
2a. **Research** (deep-research / agent): WebRTC vs WebTransport for a browser+native
    "chat room" media app — datachannels, congestion control, the coordination
    server, how to carry video/audio/chat/system msgs at different qualities. Output
    `doc/webtransport-plan.md` (a real design, cited).
2b. **Remove `Udp`** from api.json (+ its impls) — search `grep -rn Udp`.
2c. **Add `WebTransport`** API: a handle to a coordination server with typed sends
    — `send_video(stream, frame)`, `send_audio`, `send_chat`, `send_system`, each
    with a quality/reliability hint (datagram vs reliable stream). Stub engine first
    (like VideoEncoder shipped a stub), real transport behind a feature. Expose +
    codegen + build-dll check.

### 3. azul-video release example (the AzVideo demo)
   Already a BBB streaming player with a scrubbing timeline (examples/azul-video).
   Polish for the release page: status line (HW-decode probe), keep the fill + rounded
   + shadow + timeline. Ensure it's in the release build set. Verify it builds in the
   static + dll configs.

### 4. azul-meet — Google-Meet-style app (the headline demo)
   A real meeting UI demonstrating the heavy-stateful relayout pattern:
   - **Auto-login to a fake session**: generate a hash as the "meeting link" (no
     Date/rand in scripts — use a process-time/uuid-ish hash in the app), show it.
   - **Toolbar toggles** (app state booleans): mic on/off, camera on/off, screen-
     share on/off. Each toggle → RefreshDom → the DOM gains/loses the corresponding
     widget → that widget's AfterMount fires (camera capture / screencap / mic) →
     "on enable" wiring. THIS is the demo of "enable → relayout → widget on-create".
   - **Self tile**: when camera on, a `CameraWidget` tile; else a grey rectangle with
     initials. When screen-share on, a `ScreenCaptureWidget` tile.
   - **Mic**: when on, a `MicrophoneWidget` (or an AudioFrame meter); off = muted icon.
   - **Other participants**: grey rectangles (placeholders) in the grid.
   - **Device pickers**: mic-input + speaker-output dropdowns from
     `AudioDeviceList::enumerate()`; camera/screen pickers if enumerable.
   - Later: wire the captured cam/screen/mic/audio-out frames to `WebTransport`
     send_* (the end goal — send to the chat room). Stub send for now.
   Verify: builds (static + dll); headless snapshot of the grid + toolbar; toggling
   booleans changes the DOM (by-inspection / headless).

### 5. Wayland 0.2.0 bugs (from /home/fs/Downloads/temp2/bugs.txt — yesterday's CI build)
   Cross-check each against CURRENT master (several already fixed per the task list).
   Categories + fix status to verify/repair:
   - **Repaint / damage-rect** (THE recurring one): button click doesn't reliably
     redraw; image updates only on mouse release; flicker; "redraws whole window on
     cursor blink". (task #5 marked done — VERIFY on current build.)
   - **Wayland frame-callback leak** → `wl_callback@N still attached` ×300 +
     `malloc(): mismatching next->prev_size` crash on close. (task #6 marked done —
     VERIFY no leak/crash on close now.)
   - **HiDPI / 2× low-res** — maps tiles, painted canvas/SVG render at half res
     (hello-world text is fine → it's the canvas/image/tile path). (task #7 PENDING.)
   - **Input misclassification** — click → pinch-zoom; click → titlebar maximize;
     marker placed instead of pan. (task #8 marked done — VERIFY.)
   - **Maps VirtualView resize** — grey background on snap-resize, tiles don't reload,
     arrow-button text → smudges, "Clear pins" greys the screen + blanks button text.
     (tasks #9 done / #10 #12 PENDING.)
   - **Menu/popup** — menu overlays the file dialog + captures all events (only ESC
     escapes); anchored to monitor not window; still has WM decorations. (task #11.)
   - **hello-world titlebar** — faint stroked serif (should be filled sans-serif),
     white bg (should be KWin-blurred), software+WM double decoration. (#1/#2 done,
     #3 PENDING.)
   - **Locate hang** — "Locating…" never resolves; stray blue dot + red rect + white
     dot. Should probe + msgbox "geolocation unavailable". (task #12.)
   - **azul-paint Wayland startup crash**; **azul-maps Wayland freeze** (CPU 12%).
   Approach: build the current examples (azul-maps/azul-paint/hello-world), reproduce
   what's reproducible headlessly + via the framelog/logs, fix the highest-impact
   (repaint + frame-leak + HiDPI first), commit each fix verified.

### 6. Release + CI
   When the above are in + green: `gh workflow run rust.yml -f run_mode=website`
   (NOT deploy), watch the run, fix any CI failure (autofix-lint, cross-builds), cut
   the 0.2.0 release over the old one. Babysit to green.

---

## RISKS / KNOWN TOOLING LIMITS (this box)
- Wayland window-mgmt: azshot/KWin won't reliably raise/resize the azul window;
  GUI pixel verification is flaky → prefer headless snapshots / framelogs / cargo
  tests. Visual GUI confirmation gets handed to the user.
- Audio device routing: PipeWire node ≠ ALSA PCM name (see 1a).
- Portal consent (Share-click) gates live screencap → verify the sink with a
  non-portal frame source.
- Connection has dropped agents mid-run before → prefer direct commits; use agents
  only for self-contained research/review (redo if they die).

## PROGRESS LOG (append per commit)
- (start 2026-06-18) plan created; cron set for tracking.
- (2026-06-18) **azul-meet rewritten** as a Google-Meet-style app (step 4 core): toolbar
  toggles mic/cam/screen → RefreshDom → DOM gains/loses CameraWidget/ScreenCaptureWidget/
  MicrophoneWidget (AfterMount starts capture); grey-rect participants; generated
  "meeting link" hash; **Udp removed** from the example. Builds static (REAL_BUILD_EXIT=0,
  4m56s) + renders headless (AZ_HEADLESS_SNAPSHOT_PATH snapshot verified: header+grid+tiles).
  Follow-ups: larger default window, device pickers (AudioDeviceList), WebTransport send.
- (2026-06-18) **azul-meet device panel + 1100×720 window**: settings strip lists the real
  enumerated audio devices (`AudioDeviceList::enumerate` — found the user's USB Blue mic +
  HDMI/iec958 sinks); wider window shows the full 4-tile grid + toolbar. Headless snapshot
  verified (REAL_BUILD_EXIT=0). Note: AudioDeviceList/LogicalSize live in `azul::css`.
  Remaining azul-meet follow-ups: clickable device selection (routing), WebTransport send.
- (2026-06-18) **Wayland bug assessment** → `doc/wayland-bugs-status.md`: #2 frame-leak, #4
  input-misclass, #5 maps-resize ALREADY FIXED on master (the CI binary predates them — a
  fresh 0.2.0 release clears them, plus #1/#7 fixed parts). OPEN: #3 HiDPI (top), #6
  menu→xdg_popup (blocker), #10 maps-freeze + #9 paint-crash (need live Wayland), #8 Locate
  (easy), #1 over-damage (partial). Fix order in the doc.
- (2026-06-18) **WebTransport research** → `doc/webtransport-plan.md` (step 2a DONE): decisive
  recommendation = build `AzWebTransport` on **WebTransport** (HTTP/3/QUIC via `web-transport-quinn`
  native + `web-transport-wasm`), NOT WebRTC (WebTransport hit Baseline Mar 2026; WebRTC's C++
  libwebrtc + SDP/ICE state machine fight azul's C-ABI + existing media pipeline). Full api.json
  shape (`WebTransport`/`WtEvent`/`WtReliability`/`WtStats`/`OptionWtEvent`), wire format, fan-out
  server sketch, stub-loopback-first plan, + v1 checklist all in the doc. Next: step 2b/2c —
  remove `Udp`, scaffold AzWebTransport (stub engine).
- (2026-06-18) **AzWebTransport scaffold** (step 2c, Rust half): `dll/src/desktop/extra/webtransport/mod.rs`
  (POD types WtReliability/WtEventKind/WtEvent/WtStats/OptionWtEvent + `WebTransport` handle +
  v1 loopback stub engine, mirrors the `Udp` handle convention) + `dll/src/unified/webtransport.rs`
  (native re-export + wasm no-op stub) + registered in desktop/extra + unified `mod.rs`.
  `cargo check -p azul-dll --features build-dll` green (CHECK_EXIT=0). Next: wire api.json (replace
  the `Udp` class block with `WebTransport`+Wt types, add `OptionWtEvent` to the `option` module),
  codegen all, build-dll + memtest, then remove the dll `udp` module + `azul-self-test` Udp usage.
- (2026-06-18) **AzWebTransport wired into api.json** (step 2b/2c): removed the `Udp` class, added
  `WebTransport` (→window), `WtEvent`/`WtEventKind` (→dom), `WtReliability`/`WtStats` (misc),
  `OptionWtEvent` (option) — modules per `azul-doc autofix` (which also added the Default impl
  declarations; fixed a U+2014 em-dash FFI-safety error in the doc). `autofix` clean (0 drift, 0
  critical), `normalize` no-op (canonical), `codegen all` OK (101 AzWebTransport/AzWt symbols, 0
  AzUdp), `cargo check -p azul-dll --features build-dll` green. Next: remove the dll `udp` module +
  `core::udp_framing` + `azul-self-test` Udp probe; CI memtest is the final FFI size/align gate.

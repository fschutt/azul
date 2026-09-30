# IDLE_CPU progress (branch wt/idle-cpu, base ab8d5a895)

## Order (coordinator, 2026-09-30)
1. Animation culling (user ruling): off-screen / hidden / obscured animations request no frames, keep their clock.
2. Display link on demand (macOS) via a pure `FramePacer`; survey X11 / Wayland / Windows pumps.
3. A: no idle internal timers (caret tween, blink, debug-server poll).
4. B: one per-window frame interval from the monitor refresh rate + a max-frame-rate cap; replace hard-coded 16 ms / 1/60.
5. Display-list reuse for transform / opacity-only animation ticks; one shared "animated values by key" source for CPU + GPU.
6. Debug-server screenshot cache + builder page polling only while visible.
7. `scripts/idle_cpu_probe.py` + report `scripts/IDLE_CPU_2026_09_30.md`.

## DONE
- 801be66ad test(idle-cpu): an animation nobody can see asks for no frames (RED)
- 3752810f0 fix(anim): cull animations nobody can see; they keep their clock
  (tests: dll headless `tests/idle_cpu.rs`, layout compositor
  `node_groups_on_screen_follows_clips_transforms_and_transparency`)

- b7ebc1627 test(pacer): a frame pump with nothing to do stops (RED)
- 1532d9494 fix(macos): the display link runs only while frames are wanted
  (tests: `cargo test --release -p azul-dll --lib --features build-dll frame_pacer`)
- Survey: Windows waits in WaitMessage, Wayland asks wl_surface_frame per
  present, X11/Wayland single-window loops block on fds + timerfds - no
  always-on vsync pump. BUT the Linux MULTI-window wait
  (`run.rs::wait_for_linux_window_activity`) caps poll() at 16 ms (60 Hz
  wake-ups while idle) because it does not poll the windows' timer fds; the
  Wayland loop polls at 16 ms while threads run (legit). macOS RunForever
  has a 33 ms repeating drain NSTimer (idle wake-ups) -> part A.

- f4f9ae3c4 fix(macos): a WebRender frame finished after the link stopped wakes the loop
- 1c74d977d test(timers): no internal timer keeps an idle window busy (RED)
- e0c7bdff8 fix(timers): no internal timer polls an idle app
  (debug poll 16 ms -> 250 ms when quiet; RunForever 33 ms drain timer ->
  CFRunLoop BeforeWaiting observer)

- b5dcc070b test(pacing): a window paces at its monitor's refresh rate (RED)
- 90340799f fix(pacing): every frame-paced driver runs at the window's monitor rate
  (API: RendererOptions.max_frame_rate: OptionU32 appended LAST)

- a4d939c4d test(anim): a rotate/fade animation tick repaints without a display-list rebuild (RED)
- f7deb7d0e fix(anim): a rotate/fade tick repaints its rect without rebuilding the list

- c5aef5305 test(debug-server): an unchanged window is not captured again (RED)
- f81bd3a22 fix(debug-server): serve an unchanged window's screenshot from a cache
- 5571a9c66 chore(idle-cpu): scripts/idle_cpu_probe.py
- 7e53d7945 fix(idle-cpu): review pass
- 49e3cee43 test(idle-cpu): the spinner harness consumes the dirty flag
- report: scripts/IDLE_CPU_2026_09_30.md

## IN PROGRESS
(none - all seven items done; parent compiles, runs the suites and the probe)

## NEXT
Parent: compile, run the test commands in the report, run
`python3 scripts/idle_cpu_probe.py --sample` before/after.

## Open questions

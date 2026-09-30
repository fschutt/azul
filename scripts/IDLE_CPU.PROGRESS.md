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
(nothing yet)

## IN PROGRESS
1. culling

## NEXT
see order

## Open questions

# Review: layout/src/headless.rs

## Summary
- Lines: 262
- Public functions: 3 (`CpuHitTester::new`, `CpuHitTester::rebuild_from_layout`, `CpuHitTester::hit_test`)
- Public structs/enums: 1 (`CpuHitTester`)
- Findings: 1 high, 0 medium, 0 low

## Findings

### [HIGH] Stub Code — `clip` and `pointer_events_none` are always hardcoded
- **Location**: `headless.rs:223-224`
- **Details**: `clip` is always `None` and `pointer_events_none` is always `false`. The TODO comments acknowledge this. Since `CpuHitTester` is actively used across all platform backends (wayland, x11, macos, windows, headless, ios), this means:
  1. Hit testing ignores `overflow: hidden` clip regions — clicks "through" clipped areas will incorrectly match hidden nodes.
  2. `pointer-events: none` CSS property is completely ignored — elements that should not receive pointer events will still be hit.
- **Evidence**: Lines 223-224 have `// TODO` comments. `CpuHitTester` is used in `dll/src/desktop/shell2/common/event.rs:613`, `dll/src/desktop/shell2/headless/mod.rs:912`, etc.
- **Recommendation**: Implement clip chain computation from ancestor overflow properties and read `pointer-events` from the styled DOM during `rebuild_from_layout`.



## System Documentation
- System identified: yes — Headless rendering / CPU rendering pipeline
- Existing doc: none (no `doc/guide/headless.md` or `doc/guide/rendering.md`)
- Doc needed: A guide covering the headless backend, CPU rendering pipeline, and how it relates to the GPU/WebRender path. The module doc in this file is actually a good starting point for such a guide.

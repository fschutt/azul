# Review: dll/src/desktop/shell2/common/layout.rs

## Summary
- Lines: 958
- Public functions: 3 (`regenerate_layout`, `incremental_relayout`, `generate_frame`)
- Public structs/enums: 1 (`LayoutRegenerateResult`)
- Findings: 0 high, 1 medium, 0 low

## Findings

### [MEDIUM] `incremental_relayout` only wired up on macOS
- **Location**: `layout.rs:559`
- **Details**: `incremental_relayout` is called only from `dll/src/desktop/shell2/macos/mod.rs` (lines 4109, 4181). There are no call sites in the Windows, X11, or Wayland backends.
- **Evidence**: `grep -r "incremental_relayout" dll/src/desktop/shell2/` shows hits only in `common/layout.rs` and `macos/mod.rs`.
- **Recommendation**: Either wire up `incremental_relayout` on all platforms or document why it's macOS-only. If it's intentionally macOS-only for now, add a comment noting the planned rollout.

## System Documentation
- System identified: yes — Layout regeneration pipeline (part of the rendering/event loop system)
- Existing doc: `doc/guide/lifecycle.md` covers the event loop lifecycle but there is no dedicated layout pipeline guide. `scripts/INCREMENTAL_LAYOUT_ARCHITECTURE.md` exists as an internal planning doc.
- Doc needed: A `doc/guide/layout-pipeline.md` explaining the full layout regeneration flow (DOM creation -> CSD injection -> reconciliation -> state migration -> flexbox -> display list -> scrollbar registration -> frame generation), the distinction between full and incremental relayout, and how `regenerate_layout` / `incremental_relayout` / `generate_frame` interact.

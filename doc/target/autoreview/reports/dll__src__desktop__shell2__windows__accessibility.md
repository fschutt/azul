# Review: dll/src/desktop/shell2/windows/accessibility.rs

## Summary
- Lines: 163
- Public functions: 8 (4 in `#[cfg(feature = "a11y")]`, 4 in stub)
- Public structs/enums: 2 (`WindowsAccessibilityAdapter` in both cfg branches)
- Findings: 1 high, 0 medium, 1 low

## Findings

### [HIGH] Duplicated Functionality — near-identical code in x11/accessibility.rs
- **Location**: Entire file vs `dll/src/desktop/shell2/linux/x11/accessibility.rs`
- **Details**: The Windows and X11 accessibility adapters share ~90% identical code: same struct layout (`adapter: Arc<Mutex<Option<...>>>`, `pending_actions: Arc<Mutex<Vec<ActionRequest>>>`), same `new()`, `update_tree()`, `set_focus()` (both no-ops), `take_pending_actions()`, same `AccessibilityActionHandler` with identical `ActivationHandler` and `ActionHandler` impls. The only differences are the concrete adapter type and `initialize()` parameters.
- **Evidence**: Compared `accessibility.rs:19-139` (Windows) with `x11/accessibility.rs:17-146` (Linux) — structure, method signatures, doc comments, and logic are nearly verbatim copies.
- **Recommendation**: Extract a generic `AccessibilityAdapter<A>` parameterised on the platform adapter type, or use a macro to generate the common parts. The macOS version (`macos/accessibility.rs`) has diverged more (uses channels, has `poll_action` with action decoding, calls `events.raise()`) — it represents the more complete implementation.

### [LOW] No logging — unlike macOS counterpart
- **Location**: `accessibility.rs:83-95` (`update_tree`)
- **Details**: The macOS adapter has detailed `log_trace!` / `log_warn!` calls throughout `update_tree` and `update_view_focus_state`. The Windows adapter has no logging at all, making it harder to debug accessibility issues on Windows.
- **Recommendation**: Add trace-level logging consistent with the macOS adapter.

## System Documentation
- System identified: yes — Accessibility / a11y system (Windows UIA integration via accesskit)
- Existing doc: none (no `doc/guide/accessibility.md` exists; only incidental mentions in `css-styling.md` and `styling-system.md`)
- Doc needed: An `accessibility.md` guide covering the three-platform a11y architecture (Windows UIA, macOS NSAccessibility, Linux AT-SPI), the accesskit bridge pattern, tree update lifecycle, and action processing flow.

# Review: dll/src/desktop/shell2/linux/wayland/mod.rs

## Summary
- Lines: 4401
- Public functions: 30
- Public structs/enums: 3 (MonitorState, WaylandWindow, WaylandPopup, WaylandEvent)
- Findings: 0 high, 2 medium, 2 low

## Findings

### [MEDIUM] Dead Code — `WaylandPopup` struct is not used outside the module
- **Location**: `mod.rs:239-284`
- **Details**: `WaylandPopup` is a full struct with `new()`, `close()`, and `Drop` implementation, but is never instantiated anywhere in the codebase. It's only mentioned in a comment in `wayland/menu.rs`.
- **Evidence**: Grep for `WaylandPopup` found only the definition and a doc comment in menu.rs.
- **Recommendation**: Either wire popup creation into the menu/event system, or remove the dead code.

### [MEDIUM] Unsafe — `std::mem::transmute` for wl_proxy_marshal function pointers
- **Location**: `mod.rs:2522-2523`, `mod.rs:2548`, `mod.rs:2558`, `mod.rs:3802-3808`, `mod.rs:3820-3822`, `mod.rs:3928-3933`, `mod.rs:3952-3954`, `mod.rs:3962-3965`, `mod.rs:3998-3999`
- **Details**: Multiple `std::mem::transmute` calls cast `wl_proxy_marshal` to various function pointer types with different signatures. The Wayland `wl_proxy_marshal` is a variadic C function; transmuting it to fixed-arity signatures may work in practice but is UB if the calling convention doesn't match. This pattern is fragile and repeated ~10 times.
- **Recommendation**: Consider using `wl_proxy_marshal_flags` or a typed wrapper that handles the variadics correctly, or at minimum centralize the casts into helper functions with documented safety invariants.

### [LOW] Unknown keysym defaults to Escape
- **Location**: `mod.rs:471`
- **Details**: `_ => VirtualKeyCode::Escape` for unknown keysyms is surprising — pressing an unknown key will behave as if Escape was pressed, potentially closing dialogs or canceling operations.
- **Recommendation**: Return a neutral value (e.g., `VirtualKeyCode::Unknown` if available) or handle the case by not generating a key event at all.

### [LOW] File Size
- **Location**: Entire file
- **Details**: At 4401 lines, this is a large file. However, the concerns (window lifecycle, input handling, rendering, IME, tooltips, popups, D-Bus) are all cohesive to the Wayland windowing implementation. The D-Bus screensaver inhibition (lines 4097-4345, ~250 lines) and popup implementation (lines 3494-3757, ~260 lines) could be extracted into submodules for readability, but splitting is not required.
- **Recommendation**: Consider extracting `set_prevent_system_sleep` / D-Bus code into a `dbus_inhibit.rs` submodule, and `WaylandPopup` into `popup.rs`.

## System Documentation
- System identified: yes — Wayland windowing/platform backend
- Existing doc: none (no Wayland-specific guide in `doc/guide/`)
- Doc needed: A `doc/guide/windowing.md` covering the windowing abstraction layer (Linux/macOS/Windows backends, PlatformWindow trait, event dispatch, render modes). Multiple files across `shell2/` would benefit from this shared documentation.

# Review: dll/src/desktop/shell2/linux/x11/tooltip.rs

## Summary
- Lines: 185
- Public functions: 4 (`new`, `show`, `hide`, `is_visible`)
- Public structs/enums: 1 (`TooltipWindow`)
- Findings: 0 high, 3 medium, 1 low

## Findings

### [MEDIUM] Missing docs — struct fields are documented but `is_visible()` lacks detail
- **Location**: `tooltip.rs:169`
- **Details**: `is_visible()` has a one-line doc but no mention of the semantics (tracks local state, not actual X11 map state). Minor issue.
- **Recommendation**: Acceptable as-is; low priority.

### [MEDIUM] Unsafe code — `std::mem::zeroed()` for `XSetWindowAttributes`
- **Location**: `tooltip.rs:50`
- **Details**: Uses `std::mem::zeroed()` to initialize `XSetWindowAttributes`, then sets only 3 of its ~15 fields. While this is standard practice for X11 programming and the `CWOverrideRedirect | CWBackPixel | CWBorderPixel` mask ensures only those fields are read, it's worth noting that any mistake in the mask could silently use zeroed values.
- **Evidence**: The mask correctly matches the three fields set (lines 51-53 correspond to the mask on line 67).
- **Recommendation**: No change needed — the pattern is correct and idiomatic for X11.

### [MEDIUM] Duplicated logic across platform tooltip modules
- **Location**: `tooltip.rs:110-112` vs `macos/tooltip.rs:110-112` vs `wayland/tooltip.rs:105-110`
- **Details**: Text width estimation formula (`len * 7 + 10`, clamped to 50-400) is copy-pasted across all three platform tooltip implementations. The Wayland version uses the same `char_width = 7` but slightly different padding math.
- **Recommendation**: Consider extracting the text size estimation into a shared helper in a common tooltip module, or at minimum named constants.

### [MEDIUM] Text rendering quality — XDrawString uses the default X11 font
- **Location**: `tooltip.rs:127-135`
- **Details**: `XDrawString` uses whatever the default font is in the GC, which on modern X11 systems is typically the ancient "fixed" bitmap font. The `7.0` pixels-per-character estimate will be wrong for most fonts. The Wayland tooltip has the same problem but is more explicit about it being a placeholder (drawing black rectangles instead of text).
- **Recommendation**: This is a known limitation. Consider using XFT/fontconfig for better text rendering, or accept this as a simple fallback tooltip.

### [LOW] Module doc is good
- **Location**: `tooltip.rs:1-10`
- **Details**: The file has a well-structured `//!` module doc explaining responsibility, key types, and architecture. No issues.

## System Documentation
- System identified: yes — X11 windowing / tooltip subsystem, part of the broader desktop shell2 platform layer
- Existing doc: none (no `doc/guide/windowing.md` or `doc/guide/platform-shell.md`)
- Doc needed: A guide document for the platform shell/windowing system (`doc/guide/windowing.md`) covering how the shell2 module is organized across platforms (X11, Wayland, macOS, Windows), the common abstractions, and how platform-specific features like tooltips, menus, and accessibility are wired in.

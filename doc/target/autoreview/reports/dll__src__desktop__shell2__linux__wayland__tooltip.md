# Review: dll/src/desktop/shell2/linux/wayland/tooltip.rs

## Summary
- Lines: 307
- Public functions: 3 (new, show, hide)
- Public structs/enums: 1 (TooltipWindow)
- Findings: 1 high, 2 medium, 0 low

## Findings

### [HIGH] Stub Code — Text rendering draws solid rectangles instead of actual glyphs
- **Location**: `tooltip.rs:221-244`
- **Details**: The `show()` method's "text rendering" draws a solid black rectangle for every character. The comment on line 222 explicitly says `"In a real implementation, you'd use a proper font rendering library"` and line 229 says `"Draw a simple rectangle as placeholder for each character"`. This means tooltips display solid black bars, not readable text.
- **Evidence**: Lines 226-243 iterate over characters but only draw filled rectangles of `char_width x char_height` pixels. The `_ch` variable (character value) is never used.
- **Recommendation**: Integrate with the existing text shaping/rendering pipeline used elsewhere in Azul, or at minimum use a bitmap font for basic ASCII rendering.

### [MEDIUM] Missing DPI awareness — Tooltip ignores display scale factor
- **Location**: `tooltip.rs:102`
- **Details**: The X11 tooltip (`x11/tooltip.rs:100-104`) accepts `dpi_factor: DpiScaleFactor` and converts logical to physical coordinates. The Wayland tooltip takes raw `i32` coordinates with no DPI scaling, and its pixel dimensions are hardcoded assuming 1x scale. On HiDPI displays, the tooltip will appear too small.
- **Evidence**: X11 tooltip signature: `pub fn show(&mut self, text: &str, position: LogicalPosition, dpi_factor: DpiScaleFactor)`. Wayland tooltip signature: `pub fn show(&mut self, text: &str, x: i32, y: i32)`.
- **Recommendation**: Accept logical coordinates and a scale factor, convert dimensions accordingly.

### [MEDIUM] Missing return values — `show()` and `hide()` return `()` unlike other platforms
- **Location**: `tooltip.rs:102, 262`
- **Details**: The X11, macOS, and Windows tooltip implementations return `Result<(), String>` from `show()` and `hide()`. The Wayland implementation returns `()` and silently logs errors instead. This means callers cannot detect or handle tooltip failures.
- **Evidence**: X11 `tooltip.rs:100`: `pub fn show(...) -> Result<(), String>`, X11 `tooltip.rs:154`: `pub fn hide(&mut self) -> Result<(), String>`.
- **Recommendation**: Return `Result<(), String>` for consistency with other platforms.

## System Documentation
- System identified: yes — Wayland windowing / shell integration subsystem
- Existing doc: none (no windowing guide in `doc/guide/`)
- Doc needed: A `doc/guide/windowing.md` covering the cross-platform windowing abstraction (`shell2`), per-platform tooltip/popup/dialog implementations, and how subsurfaces/overlays work on each platform.

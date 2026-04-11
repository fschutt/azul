# Review: core/src/resources.rs

## Summary
- Lines: 3022 (non-blank: ~2679)
- Public functions: ~84
- Public structs/enums: ~69
- Findings: 0 high, 4 medium, 2 low

## Findings

### [MEDIUM] Stub Field — `enable_tab_navigation` is marked as stub
- **Location**: `resources.rs:399`
- **Details**: The comment says `(STUB) Whether keyboard navigation should be enabled (default: true). Currently not implemented.` but the field is public API. Only referenced in `core/src/resources.rs` and `api.json`, never read for behavior.
- **Evidence**: `grep "enable_tab_navigation"` returns only `core/src/resources.rs` and `api.json`.
- **Recommendation**: Either implement tab navigation or document this as a planned feature. The `(STUB)` tag should be tracked.

### [MEDIUM] Excessive TODOs in `into_loaded_image_source`
- **Location**: `resources.rs:1667-2083`
- **Details**: This 416-line function contains 11 TODO comments about SIMD optimization and premultiply alpha. These are copy-paste repetitive across every pixel format branch.
- **Recommendation**: These are aspirational performance TODOs, not bugs. Consider consolidating into a single top-level TODO or tracking as an issue.

### [MEDIUM] Function Length — `into_loaded_image_source` is ~416 lines
- **Location**: `resources.rs:1668-2083`
- **Details**: This function handles 12 pixel format conversions in a single match block. While the logic per-arm is straightforward, the repetitive per-pixel-format structure makes it hard to audit.
- **Recommendation**: Extract helper functions for common patterns (e.g., `convert_rgb_to_bgra`, `convert_rgba16_to_bgra8`). The premultiply + swizzle logic is repeated ~6 times.

### [MEDIUM] Function Length — `build_add_font_resource_updates` is ~160 lines
- **Location**: `resources.rs:2706-2858`
- **Details**: Contains a macro (`insert_font_instances!`) defined inside the function body, nested label-break loops, and platform-conditional code. Readable but dense.
- **Recommendation**: Consider extracting the `insert_font_instances!` macro into a helper function.

### [MEDIUM] `font_ref_get_hash` is a trivial wrapper with no external callers
- **Location**: `resources.rs:1111-1113`
- **Details**: `pub fn font_ref_get_hash(fr: &FontRef) -> u64 { fr.get_hash() }` exists only to be called from `api.json` FFI bindings. It is a one-line pass-through.
- **Evidence**: Only called from `api.json` FFI binding. The function just delegates to `FontRef::get_hash()`.
- **Recommendation**: LOW priority — exists for FFI, acceptable.

### [LOW] `..Default::default()` in `FontInstanceOptions` construction
- **Location**: `resources.rs:2751-2755`
- **Details**: `FontInstanceOptions { render_mode, flags, ..Default::default() }` defaults `bg_color` to `ColorU::TRANSPARENT` and `synthetic_italics` to angle 0. Both are safe defaults for the font rendering context.
- **Recommendation**: No action needed — defaults are correct.

### [LOW] `match_route_for_path` only called internally
- **Location**: `resources.rs:599-606`
- **Details**: Only referenced in `core/src/resources.rs`. However, this is part of the public `AppConfig` API and is likely intended for use by the web server (`dll/src/web/server.rs` uses `match_route`).
- **Recommendation**: Acceptable — part of the public API surface.

## System Documentation
- System identified: **Resource Management** (font/image key lifecycle, resource caching, GPU texture management)
- Existing doc: none (no `doc/guide/resource-management.md` or similar)
- Doc needed: A guide explaining the resource lifecycle — how fonts and images are loaded, keyed, registered with the renderer, and garbage collected frame-to-frame. This file is the core of that system.

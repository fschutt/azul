# Review: dll/src/web/html_render.rs

## Summary
- Lines: 597
- Public functions: 1 (`render_initial_page`)
- Public structs/enums: 3 (`CollectedImage`, `CollectedFont`, `RenderOutput`)
- Findings: 1 high, 2 medium, 2 low

## Findings

### [HIGH] Stub Code — `"stub"` literal passed to `generate_loader_js`
- **Location**: `html_render.rs:89`
- **Details**: The call `super::loader_js::generate_loader_js("stub", cb_wasms)` passes the literal string `"stub"` as the `mini_wasm_hash` argument. The `generate_loader_js` function ignores both parameters (`_mini_wasm_hash`, `_callbacks`) and always calls `generate_phase0_loader()`. This means:
  1. The `mini_wasm` bytes passed to `render_initial_page` are preloaded (line 449) but never actually used by the loader JS.
  2. The `cb_wasms` are preloaded but never referenced in the generated JS.
  3. The literal `"stub"` is a clear indicator of unfinished integration.
- **Evidence**: `loader_js.rs:10-15` shows both params are prefixed with `_`. Also `server.rs:65` passes the same `"stub"` literal.
- **Recommendation**: Either wire up the real WASM hash/callback references or document this as intentionally Phase-0-only.

### [MEDIUM] Duplicated `html_escape` function
- **Location**: `html_render.rs:577-583`
- **Details**: A nearly identical `html_escape` function exists in `doc/src/reftest/regression.rs:2435`. The implementations differ slightly (regression.rs also escapes `"` and `'`), but the purpose is the same.
- **Evidence**: Grep for `fn html_escape` found both files.
- **Recommendation**: Low priority since these are in different crates with different audiences, but note the inconsistency — the `html_render.rs` version does NOT escape `"` in text content (only in `html_escape_attr`). This is technically correct for HTML text nodes but could cause issues if text is ever placed in attributes.

### [MEDIUM] `debug_print_dom` / `build_stylesheet` / `call_layout` / `generate_preload_hints` / `pseudo_state_to_css` — zero external call sites
- **Location**: Various internal functions
- **Details**: These are all private (`fn`, not `pub fn`) and only called within this file. This is correct — they are internal helpers. No issue with dead code since they are used within the file. **However**, `debug_print_dom` is only used for debug logging (line 77) and adds ~37 lines of code solely for stderr output.
- **Recommendation**: Consider removing `debug_print_dom` or gating it behind a feature flag.

### [LOW] Hard-coded `"font/ttf"` content type
- **Location**: `html_render.rs:106`
- **Details**: All bundled fonts are served with `content_type: "font/ttf"` regardless of actual format. If a WOFF2 or OTF font is bundled, it will be served with the wrong MIME type.
- **Recommendation**: Detect font format from the byte header or allow `NamedFont` to carry its content type.

### [LOW] `event_filter_to_js_name` catch-all returns `"click"`
- **Location**: `html_render.rs:563`, `572`, `574`
- **Details**: Unrecognized event filter variants fall through to `"click"`. This is a reasonable default for Phase 0 server-side execution, but could silently misroute events if new event types are added.
- **Recommendation**: Acceptable for now, but consider logging unhandled variants in debug mode.

## System Documentation
- System identified: **Web rendering / SSR pipeline** (part of the `dll/src/web/` module)
- Existing doc: `doc/guide/web.md`
- Doc needed: n/a — guide already exists for this system.

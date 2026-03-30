# Session 8C: Debugging, DropDown Fix, HTTP Errors, AZ_RECORD

**Date**: 2026-03-30
**Branch**: `layout-debug-clean`

---

## 1. DropDown Widget Bugs (from screenshot)

### 1.1 Text must use `<p>`, not bare `createText`

**Problem**: Bare text nodes (`NodeType::Text`) are inline and have no block formatting.
All text in the dropdown.c example runs together on one line because there's no `<p>`
wrapper providing block display + margins.

**Fix in dropdown.c**: Wrap labels in `AzDom_createP()` or use `AzDom_createDiv()` containers
with `display: block`.

**Fix in drop_down.rs**: The selected text label inside the dropdown trigger should be
wrapped in a `<p>` element: `Dom::create_p().with_children(vec![Dom::create_text(selected_text)])`.

### 1.2 Dropdown must use NATIVE menu spawning, not DOM popup

**Problem**: The dropdown opens a window-based DOM popup menu. On macOS, it should use
native `NSMenu` via `popUpMenuPositioningItem:` for proper system integration.

**Root cause**: The `use_native_context_menus` flag defaults to... need to check. The
dropdown calls `info.open_menu_for_hit_node(menu)` which goes through the platform's
`show_menu_from_callback()`. On macOS, this checks `flags.use_native_context_menus`:
- If true → `show_native_context_menu_at_position()` → NSMenu
- If false → `show_fallback_menu()` → window-based DOM popup

**Fix**: Either default `use_native_context_menus` to `true`, or have the dropdown
explicitly request native menus. The native path already exists in
`dll/src/desktop/shell2/macos/events.rs:983-1188`.

### 1.3 Arrow chevron should use `<icon>` not border hack

**Problem**: Current arrow uses a 6×6 div with 2px left+bottom borders rotated 315°.
This is fragile and doesn't scale with font size.

**Fix**: Use `Dom::create_icon("arrow_drop_down")` which resolves via the icon system.
Material Icons provides `expand_more` and `arrow_drop_down` glyphs. Need to ensure
material icons are registered at app startup (via `register_embedded_material_icons()`).

**Fallback**: If icons aren't registered, the icon resolver returns an empty div — so
the border hack can remain as CSS-only fallback on the icon node itself.

### 1.4 Dropdown should be `display: inline-block`

**Problem**: The dropdown trigger is currently `display: inline-flex` with `flex-grow: 0`,
but it doesn't auto-size properly — it either takes full width or collapses.

**Fix**: Change to `display: inline-block` so it shrinks to fit content but stays on the
same line as adjacent text. `LayoutDisplay::InlineBlock` is fully supported in the CSS
engine (`css/src/props/layout/display.rs:20`).

### Files to change:
- `layout/src/widgets/drop_down.rs` — fix DOM structure, icon, display
- `examples/c/dropdown.c` — wrap text in `<p>` elements

---

## 2. HTTP Error Diagnostics (browser.c still fails)

### 2.1 Root cause: error mapping loses all type info

**File**: `layout/src/http.rs:398-400`

All ureq errors are collapsed to `HttpError::Other(String)`:
```rust
let response = request.call().map_err(|e| {
    HttpError::other(e.to_string().into())  // Loses TLS/DNS/timeout specifics
})?;
```

The `HttpError` enum has specific variants (`TlsError`, `ConnectionFailed`, `Timeout`,
`IoError`, `HttpStatus`) but they're **never populated**. Everything goes to `Other`.

**Fix**: Restore proper error mapping from ureq 3.3's error types:

```rust
let response = request.call().map_err(|e| {
    match e.kind() {
        ureq::ErrorKind::Tls => HttpError::tls_error(e.to_string().into()),
        ureq::ErrorKind::Dns | ureq::ErrorKind::ConnectionFailed
            => HttpError::connection_failed(e.to_string().into()),
        ureq::ErrorKind::Io => HttpError::io_error(e.to_string().into()),
        ureq::ErrorKind::Timeout => HttpError::Timeout,
        ureq::ErrorKind::Http => {
            // Extract status code if available
            HttpError::other(e.to_string().into())
        }
        _ => HttpError::other(e.to_string().into()),
    }
})?;
```

### 2.2 Possible TLS handshake issue

**Symptom**: `curl https://example.com` succeeds in 46ms, but the ureq request hangs
for 30s and then fails.

**Likely cause**: `rustls-rustcrypto 0.0.2-alpha` may be missing a cipher suite or
key exchange algorithm that the server requires. Unlike `ring` which supports all
common algorithms, `rustls-rustcrypto` is alpha and may lack ECDH-X25519 or similar.

**Diagnostic approach**: After fixing the error mapping, the TLS error message will
tell us exactly which algorithm is missing. If it's a cipher suite issue, we may need
to check what `rustls_rustcrypto::provider()` actually provides vs what example.com
negotiates.

### 2.3 AzHttpError_toDbgString improvement

The Debug impl should include the variant name:
```
TlsError("handshake failed: no shared cipher suites")
```
Not just:
```
Other("ureq::Error: ...")
```

### Files to change:
- `layout/src/http.rs` — restore error mapping
- `core/src/http.rs` or wherever HttpError Debug is derived — verify output
- `examples/c/browser.c` — improve error display

---

## 3. AZ_RECORD Environment Variable (comprehensive logging)

### Design

`AZ_RECORD=<filepath>` enables full verbose logging to a file. Unlike `AZUL_DEBUG=<port>`
which requires an HTTP client to read logs, `AZ_RECORD` writes directly to a file.

### What it logs

When enabled, ALL `log_trace!` / `log_debug!` / etc. macros fire and write to the file:

1. **Window events**: Every mouse move, key press, focus change, resize
2. **Layout**: Cascade timings, node counts, dirty flags, reflow triggers
3. **Rendering**: Frame generation, display list diffs, damage rects, blit timings
4. **IME**: Preedit text, composition events, cursor position syncs
5. **Callbacks**: Which callbacks fire, their return values (Update::RefreshDom etc.)
6. **Timers**: Timer fire events, animation frame timings

### Implementation

In `dll/src/desktop/shell2/common/debug_server.rs`:

```rust
static RECORD_FILE: OnceLock<Option<Mutex<std::fs::File>>> = OnceLock::new();

pub fn init_recording() {
    if let Ok(path) = std::env::var("AZ_RECORD") {
        let file = std::fs::File::create(&path).ok();
        RECORD_FILE.set(file.map(Mutex::new)).ok();
        // Also enable debug logging so all log macros fire
        DEBUG_ENABLED.store(true, Ordering::SeqCst);
    }
}

// In the log() function, also write to file:
pub fn log(level: LogLevel, category: LogCategory, message: String, window_id: Option<usize>) {
    if let Some(Some(ref file)) = RECORD_FILE.get() {
        if let Ok(mut f) = file.lock() {
            writeln!(f, "[{:?}] [{:?}] {}", level, category, message).ok();
        }
    }
    // ... existing debug server log queue
}
```

### Call site

Add `init_recording()` call at the very start of `AzApp_run()` or `AzApp_create()`.

### Files to change:
- `dll/src/desktop/shell2/common/debug_server.rs` — add file recording
- `dll/src/desktop/shell2/macos/mod.rs` — add more log_trace! calls for window events
- `dll/src/desktop/shell2/common/event.rs` — add more log_trace! for event processing

---

## 4. CPU Rendering Damage Rects

### Current state: BROKEN

The CPU rendering path always does full-framebuffer redraws:
1. `render_with_font_manager_and_scroll()` renders the entire display list
2. `cpu_view.update_framebuffer()` always calls `setNeedsDisplay(true)` (full invalidation)
3. `previous_display_list` field exists but is **never populated or compared**
4. `gpu_damage_rects` is populated by WebRender but **unused in CPU path**

### Why it matters

Without partial redraws:
- Every cursor blink redraws the entire window
- Every preedit update redraws everything
- Scroll events redraw everything
- This is very slow for large DOMs

### Fix plan

1. After CPU rendering, store the display list in `previous_display_list`
2. On next frame, compare with `compute_text_damage_rect()` or full diff
3. Pass damage rects to CPU renderer — only re-render changed regions
4. In `update_framebuffer()`, use `setNeedsDisplayInRect:` per damage rect

This is a significant optimization that should be a separate PR.

### Files to change:
- `dll/src/desktop/shell2/macos/mod.rs` — CPU render path + damage tracking
- `layout/src/cpurender.rs` — add damage-rect-aware rendering
- Same for Windows/X11/Wayland CPU paths

---

## 5. Implementation Priority

| # | Task | Effort | Impact |
|---|------|--------|--------|
| 1 | Fix HTTP error mapping (restore ureq error types) | 30 min | Unblocks browser.c |
| 2 | Fix dropdown DOM (`<p>`, inline-block, icon) | 1 hr | Fixes visible bugs |
| 3 | Add AZ_RECORD env var | 1 hr | Enables debugging |
| 4 | Fix native menu spawning for dropdown | 30 min | Native UX |
| 5 | CPU damage rect optimization | 3-4 hrs | Performance |

---

## 6. Key Files

| Component | File |
|-----------|------|
| HTTP client | `layout/src/http.rs` |
| HttpError enum | `core/src/http.rs` |
| DropDown widget | `layout/src/widgets/drop_down.rs` |
| Icon system | `core/src/icon.rs`, `layout/src/icon.rs` |
| Debug logging | `dll/src/desktop/shell2/common/debug_server.rs` |
| CPU rendering (macOS) | `dll/src/desktop/shell2/macos/mod.rs` |
| Native menus (macOS) | `dll/src/desktop/shell2/macos/events.rs:948-1188` |
| Display list damage | `layout/src/solver3/display_list.rs:308-343` |

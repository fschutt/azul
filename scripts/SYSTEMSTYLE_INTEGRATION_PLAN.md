# SystemStyle Integration Plan

**Date:** 2025-01-XX  
**Status:** Draft — post-exploration, pre-implementation  
**Scope:** Connect OS-queried `SystemStyle` values to actual rendering/input behaviour;
add color-emoji support; implement CSD negotiation on Wayland.

---

## Table of Contents

1. [Current State — Gaps Summary](#1-current-state--gaps-summary)
2. [Task A: Wire InputMetrics into Gesture Manager](#2-task-a-wire-inputmetrics-into-gesture-manager)
3. [Task B: Wire Caret Blink / Width into Cursor Manager](#3-task-b-wire-caret-blink--width-into-cursor-manager)
4. [Task C: Wire Wheel Scroll Lines into Scroll Handling](#4-task-c-wire-wheel-scroll-lines-into-scroll-handling)
5. [Task D: Pass TextRenderingHints to Webrender](#5-task-d-pass-textrenderinghints-to-webrender)
6. [Task E: Color Emoji — COLR/SVG via allsorts + resvg](#6-task-e-color-emoji--colrsvg-via-allsorts--resvg)
7. [Task F: CSD Titlebar Decision Tree (Wayland xdg-decoration)](#7-task-f-csd-titlebar-decision-tree-wayland-xdg-decoration)
8. [Task G: Tests for KDE / GNOME CSS Detection](#8-task-g-tests-for-kde--gnome-css-detection)
9. [Priority & Dependency Graph](#9-priority--dependency-graph)

---

## 1. Current State — Gaps Summary

All `SystemStyle` fields are **queried from the OS** (Windows via Win32 API, macOS via
Objective-C runtime, Linux via D-Bus / gsettings) but most are **never consumed** by the
actual rendering or input code. The values sit in the `SystemStyle` struct and are only
used for CSD stylesheet generation.

| Metric | OS Query Exists? | Actually Used? | Problem |
|--------|:---:|:---:|---------|
| `drag_threshold_px` | ✅ Win | ❌ | `GestureDetectionConfig` hardcodes `5.0` |
| `double_click_time_ms` | ✅ Win, Mac | ❌ | `GestureDetectionConfig` hardcodes `500` |
| `double_click_distance_px` | ✅ Win | ❌ | Defaults disagree: `4.0` vs `5.0` |
| `caret_blink_rate_ms` | ✅ Win, Linux | ❌ | `CURSOR_BLINK_INTERVAL_MS` hardcodes `530` |
| `caret_width_px` | ✅ Win | ❌ | CSS property fallback is `2.0px`, `InputMetrics` default is `1.0` |
| `wheel_scroll_lines` | ✅ Win | ❌ | Scroll code hardcodes `* 20.0` factor |
| `hover_time_ms` | ✅ Win | ❓ | Win32 tooltips use native timing |
| `font_smoothing_enabled` | ✅ Win | ❌ | Glyph rasterizer ignores it |
| `font_smoothing_gamma` | ✅ Win | ❌ | `gamma_lut` module is dead code |
| `subpixel_type` | ✅ Win | ❌ | `prepare_font()` forces `Alpha` mode |
| Color emojis (COLR/SVG) | — | ❌ | Rasterizer only produces alpha masks |

### Key Files

| File | Role |
|------|------|
| `css/src/system.rs` | `SystemStyle`, `InputMetrics`, `TextRenderingHints`, all defaults |
| `css/src/system_native_{macos,windows,linux}.rs` | OS query code |
| `layout/src/managers/gesture.rs` | `GestureDetectionConfig` — drag, double-click, long-press |
| `layout/src/managers/cursor.rs` | `CursorManager`, `CURSOR_BLINK_INTERVAL_MS = 530` |
| `layout/src/window.rs:1565` | Timer setup using `CURSOR_BLINK_INTERVAL_MS` |
| `layout/src/solver3/getters.rs:1320–1360` | CSS caret property getter (color, width, animation) |
| `dll/src/desktop/shell2/windows/mod.rs:2261` | `scroll_amount * 20.0` hardcode |
| `dll/src/desktop/shell2/linux/x11/events.rs:448` | `-delta_x * 20.0` hardcode |
| `core/src/resources.rs:1938–1960` | `FontInstanceOptions`, `FontInstancePlatformOptions` |
| `dll/src/desktop/wr_translate2.rs:1030–1060` | Where `FontInstanceOptions` are created (hardcoded gamma/contrast) |
| `core/src/resources.rs:2505` | Second place `FontInstanceOptions` are created |
| `webrender/glyph/src/font.rs:96–100` | `prepare_font()` — forces `FontRenderMode::Alpha` |
| `webrender/glyph/src/rasterizer.rs` | `GlyphRasterizer` — pure-Rust, tiny-skia based |
| `dll/src/desktop/csd.rs:162` | `should_inject_csd()` — current decision logic |
| `dll/src/desktop/shell2/linux/wayland/mod.rs` | Wayland shell — no xdg-decoration yet |

---

## 2. Task A: Wire InputMetrics into Gesture Manager

### What

Replace the hardcoded `GestureDetectionConfig::default()` values with values from
`SystemStyle.input`.

### Where

- **Source:** `SystemStyle.input: InputMetrics` (in `css/src/system.rs`)
- **Sink:** `GestureDetectionConfig` (in `layout/src/managers/gesture.rs:95–135`)
- **Wiring point:** `GestureManager::new()` in `gesture.rs:393` currently calls
  `GestureDetectionConfig::default()`. Needs a `from_system_style(&InputMetrics)` constructor.

### How

1. Add `GestureDetectionConfig::from_input_metrics(im: &InputMetrics) -> Self`:
   ```
   drag_distance_threshold  ← im.drag_threshold_px          (default 5.0)
   double_click_time_ms     ← im.double_click_time_ms as u64 (default 500)
   double_click_distance    ← im.double_click_distance_px    (default 4.0)
   // long_press, swipe, pinch, rotation stay at defaults (no OS query for those)
   ```

2. All call-sites that create `GestureManager::new()` must pass an `&InputMetrics`.
   Search for `GestureManager::new()` in `dll/src/desktop/shell2/` — should be in
   per-window initialization code. The `SystemStyle` is already available there via
   `resources.system_style`.

3. Fix the default discrepancy: `InputMetrics.double_click_distance_px` defaults to `4.0`
   but `GestureDetectionConfig.double_click_distance_threshold` defaults to `5.0`.
   Use the OS value when available, otherwise `4.0` (Windows default).

### Effort: Small (< 50 LoC)

---

## 3. Task B: Wire Caret Blink / Width into Cursor Manager

### What

Replace the hardcoded `CURSOR_BLINK_INTERVAL_MS = 530` constant with the OS-queried value
from `SystemStyle.input.caret_blink_rate_ms`. Also make `InputMetrics.caret_width_px` serve
as the **fallback** when no CSS `caret-width` property is set.

### Current Flow

```
CSS property `caret-width` → getters.rs:1341 → unwrap_or(2.0px)
CSS property `caret-animation-duration` → getters.rs:1349 → unwrap_or(500ms)
```

The hardcoded `530ms` constant in `cursor.rs` is used for the actual blink timer, while
the CSS fallback is `500ms`. These are inconsistent.

### How

1. **Remove `CURSOR_BLINK_INTERVAL_MS` constant.** Replace all usages with a value read
   from `SystemStyle`:
   - `cursor.rs:228` — blink interval timer
   - `window.rs:1565` — timer setup

2. **Cascade:** CSS `caret-animation-duration` → SystemStyle → 500ms fallback.
   The CSS property always wins if explicitly set. If not set, use
   `system_style.input.caret_blink_rate_ms`. If that's zero (caret never blinks), honor it.

3. **Caret width cascade:** CSS `caret-width` → `system_style.input.caret_width_px` → `1.0px`.
   Update `getters.rs:1341` to accept an optional `&InputMetrics` parameter so it can use
   the OS default instead of hardcoded `2.0`.

4. **Fix caret color default:** Currently white `(255,255,255,255)` in `getters.rs:1328`.
   This should be `inherit` from the text color, or fallback to the `SystemStyle` text color
   if none is set. CSS spec says `caret-color: auto` means "currentcolor".

### Threading Note

The `SystemStyle` is stored in `Arc<SystemStyle>` and is immutable after window creation.
The `CursorManager` and timer code already have access to shared window state — need to
thread the `Arc<SystemStyle>` (or just the blink rate) through.

### Effort: Medium (~100 LoC, touches 4 files)

---

## 4. Task C: Wire Wheel Scroll Lines into Scroll Handling

### What

Replace the hardcoded `* 20.0` mouse-wheel scaling factor with a computed value based on
`SystemStyle.input.wheel_scroll_lines`.

### Where (hardcoded sites)

1. `dll/src/desktop/shell2/windows/mod.rs:2261` — `scroll_amount * 20.0`
2. `dll/src/desktop/shell2/linux/x11/events.rs:448–449` — `-delta_x * 20.0`, `-delta_y * 20.0`
3. macOS and Wayland use raw pixel deltas (correct behavior, no change needed)

### How

The `20.0` comes from the implicit assumption of `3 lines × ~6–7px per line ≈ 20px`.
The correct formula is:

```
pixels_per_notch = wheel_scroll_lines × line_height_px
```

Where `line_height_px` should ideally come from the actual font size of the content being
scrolled. As a simplification, use `system_style.fonts.ui_font_size * 1.2` (typical
line-height ratio) or a fixed `20.0` if the system font size is unknown.

**Simple approach:** Replace `20.0` with `system_style.input.wheel_scroll_lines as f32 * 6.67`
(where `6.67px ≈ one line at default size), preserving the ~20px default when lines=3.

The `SystemStyle` should already be accessible in the shell modules since they hold
`Arc<SystemStyle>` in their resources struct.

### Effort: Small (~20 LoC)

---

## 5. Task D: Pass TextRenderingHints to Webrender

### Current State

The glyph rasterizer is **pure-Rust** (allsorts + tiny-skia). It:

- Always rasterizes in `FontRenderMode::Alpha` (grayscale AA)
- Ignores `FontRenderMode::Subpixel` even though the display-list requests it
- Has no gamma correction (the `gamma_lut` module is dead code)
- Uses hardcoded `gamma: 300`, `contrast: 100`, `cleartype_level: 100` on Windows

### What We Can Do Now (Low-Hanging Fruit)

1. **Pass `font_smoothing_gamma`** from `TextRenderingHints` to `FontInstancePlatformOptions`:
   - `wr_translate2.rs:1032` — `gamma: 300` → `gamma: system_style.text_rendering.font_smoothing_gamma`
   - `resources.rs:2505` — same
   - The gamma value from `TextRenderingHints` is already in the right scale (percentage, 100–300 range)

2. **Honor `font_smoothing_enabled`**: When `false`, set `FontRenderMode::Mono` instead of `Alpha`.
   Modify `webrender/glyph/src/font.rs:prepare_font()`:
   ```rust
   pub fn prepare_font(font: &mut FontInstance) {
       // If font smoothing is disabled, use Mono mode
       if font.flags & FONT_INSTANCE_FLAG_NO_SMOOTH != 0 {
           font.render_mode = FontRenderMode::Mono;
       } else {
           font.render_mode = FontRenderMode::Alpha;
       }
       font.color = api::ColorU::new(255, 255, 255, 255);
   }
   ```
   And set the flag when creating font instances based on `text_rendering.font_smoothing_enabled`.

3. **Pass `increased_contrast`**: On Windows, map to the `contrast` field in
   `FontInstancePlatformOptions`. When accessibility `increased_contrast` is true,
   bump contrast from 100 to e.g. 200.

### What Needs More Work (Subpixel AA)

True subpixel (LCD) anti-aliasing requires:
- Rendering glyphs at 3× horizontal resolution (for RGB subpixel layout)
- Color-aware compositing in the shader
- Knowledge of the physical subpixel layout (RGB, BGR, V-RGB, V-BGR)

This is a **large** project and not in scope for this iteration. The current pure-Rust
rasterizer would need significant changes. However, we should still:
- Correctly propagate `subpixel_type` from `TextRenderingHints` into the font instance flags
- This way, when/if subpixel rendering is implemented later, the plumbing is already in place

### Effort: Medium (~80 LoC for low-hanging fruit)

---

## 6. Task E: Color Emoji — COLR/SVG via allsorts + resvg

### Current State

- **allsorts** (v0.16.1) is a dependency via the `text_layout` feature
- **resvg** (v0.45.0) is an optional dependency — used for rendering SVG *files*, not glyphs
- The glyph rasterizer (`webrender/glyph/src/font.rs`) only handles outline glyphs → alpha masks
- There is **zero** color font support (no COLR, CBDT, sbix, SVG-in-OpenType)

### allsorts COLR Support

allsorts has partial COLR v0 support. Relevant allsorts types:

```
allsorts::tables::colr::Colr — COLR table parser
allsorts::tables::cpal::Cpal — Color palette
allsorts::colour::ColourGlyph — Represents a layered color glyph
```

For **COLR v0**: Each glyph is composed of layers, each referencing a regular glyph ID + a
palette color index. We can render each layer as a separate alpha mask and tint it.

For **COLR v1** (gradients, compositing): allsorts does NOT have full v1 support yet.
This would require rendering gradient fills, which is more complex.

### SVG-in-OpenType

Some fonts (e.g., Twemoji Mozilla) embed SVG documents per glyph in the `SVG ` table.
allsorts can parse the SVG table. We already have resvg. The pipeline would be:

```
SVG table → extract SVG document for glyph → resvg::render() → RGBA pixmap → texture
```

### Proposed Implementation (COLR v0 + SVG)

1. **In `webrender/glyph/src/font.rs::rasterize_glyph()`:**
   - Before falling back to outline rendering, check for color tables in order:
     1. `SVG ` table → use resvg
     2. `COLR` + `CPAL` tables → layered rendering
     3. `CBDT`/`CBLC` (bitmap) → extract pre-rendered bitmap (stretch goal)
     4. Regular outline → current path

2. **SVG glyph path:**
   ```
   fn rasterize_svg_glyph(font_data, glyph_id, size) -> Option<RasterizedGlyph> {
       let svg_doc = allsorts::tables::svg::SvgTable::read(font_data)?;
       let svg_for_glyph = svg_doc.lookup(glyph_id)?;
       let tree = resvg::Tree::from_str(svg_for_glyph, &usvg::Options::default())?;
       let mut pixmap = tiny_skia::Pixmap::new(width, height)?;
       resvg::render(&tree, transform, &mut pixmap.as_mut());
       Ok(RasterizedGlyph {
           format: GlyphFormat::ColorBitmap,  // BGRA8
           bytes: pixmap.data().to_vec(),
           ...
       })
   }
   ```

3. **COLR v0 path:**
   ```
   fn rasterize_colr_glyph(font_data, cpal, colr, glyph_id, size) -> Option<RasterizedGlyph> {
       let layers = colr.get_glyph_layers(glyph_id)?;
       let mut pixmap = tiny_skia::Pixmap::new(width, height)?;
       for layer in layers {
           let alpha_mask = rasterize_outline(font_data, layer.glyph_id, size)?;
           let color = cpal.get_color(layer.palette_index)?;
           composite_tinted_mask(&mut pixmap, &alpha_mask, color);
       }
       Ok(RasterizedGlyph { format: GlyphFormat::ColorBitmap, bytes: pixmap.data().to_vec(), ... })
   }
   ```

4. **GlyphFormat change:** Currently only `Alpha` and `ColorBitmap` exist. The webrender
   scene builder already handles `ColorBitmap` format for texture uploads — we just need the
   rasterizer to actually produce it.

### Dependencies

- `resvg` feature must be enabled when color emoji support is wanted
- allsorts SVG table parsing: need to verify it's included in the 0.16.1 API
- May need `usvg` explicitly for SVG tree parsing (resvg 0.45 bundles it)

### Effort: Large (~300–500 LoC, new feature)

---

## 7. Task F: CSD Titlebar Decision Tree (Wayland xdg-decoration)

### Research Summary

The complete decision tree for "should Azul draw its own titlebar?" is:

```
START: Window creation
│
├── macOS → NEVER inject CSD
│   Native NSWindow always provides titlebar.
│   (titlebarAppearsTransparent for custom look)
│
├── Windows → NEVER inject CSD
│   DWM always provides titlebar.
│   (DwmExtendFrameIntoClientArea for custom look)
│
├── Linux / Wayland:
│   │
│   ├── Does compositor advertise zxdg_decoration_manager_v1?
│   │   │
│   │   ├── YES (KWin, Sway, Hyprland, wlroots-based, COSMIC, labwc):
│   │   │   ├── Create zxdg_toplevel_decoration_v1 object
│   │   │   ├── Call set_mode(server_side)  ← prefer SSD
│   │   │   ├── Wait for configure(mode) event
│   │   │   │   ├── mode = server_side → NO CSD (compositor draws titlebar)
│   │   │   │   └── mode = client_side → INJECT CSD (SoftwareTitlebar)
│   │   │   └── Respect compositor's decision, do not override
│   │   │
│   │   └── NO (GNOME/Mutter, Weston):
│   │       └── ALWAYS inject CSD (SoftwareTitlebar)
│   │           Must also handle:
│   │           - Window drag by titlebar (wl_surface.move)
│   │           - Resize edges (wl_surface.resize)
│   │           - Drop shadow (optional, via subsurface or CSS shadow)
│   │
│   └── ENV override: QT_WAYLAND_DISABLE_WINDOWDECORATION=1
│       or custom azul env var → no decoration at all
│
├── Linux / X11:
│   │
│   ├── Window type = Normal?
│   │   └── WM provides SSD via _NET_FRAME_EXTENTS → NO CSD
│   │       (unless user explicitly requests WindowDecorations::None)
│   │
│   ├── Window has _MOTIF_WM_HINTS.decorations = 0?
│   │   └── No WM decoration → INJECT CSD if has_decorations=true
│   │
│   └── If CSD injected, set _GTK_FRAME_EXTENTS for WM shadow cooperation
│
└── Frameless (any platform): WindowDecorations::None + has_decorations=true
    └── INJECT CSD (current behavior via should_inject_csd())
```

### Current Code vs. Needed Changes

**Current (`csd.rs:162`):**
```rust
pub fn should_inject_csd(has_decorations: bool, decorations: WindowDecorations) -> bool {
    has_decorations && decorations == WindowDecorations::None
}
```

This is a **static** check — it doesn't know about Wayland compositor capabilities.

**Needed:** A new enum/field on the window state:

```rust
pub enum DecorationMode {
    /// Compositor provides decorations (SSD)
    ServerSide,
    /// Application must draw decorations (CSD)  
    ClientSide,
    /// No decorations at all
    None,
}
```

This should be determined at window creation time (on Wayland, after the `configure` event)
and stored in the window state. The `should_inject_csd()` function should check this.

### Wayland Protocol Implementation

The `xdg-decoration-unstable-v1` protocol needs to be bound in the Wayland shell code:

1. **During `wl_registry.global` enumeration** (in `wayland/mod.rs`):
   - Check for `"zxdg_decoration_manager_v1"` interface
   - Bind it if available

2. **After `xdg_toplevel` creation:**
   - Call `decoration_manager.get_toplevel_decoration(toplevel)` → decoration object
   - Call `decoration.set_mode(server_side)`

3. **Handle `decoration.configure(mode)` event:**
   - Store the mode in window state
   - If `client_side` → set `DecorationMode::ClientSide`
   - If `server_side` → set `DecorationMode::ServerSide`

4. **On next layout pass:** `should_inject_csd()` checks `DecorationMode` instead of the
   static `WindowDecorations` enum.

### Protocol XML

The protocol is `xdg-decoration-unstable-v1.xml`. Since we use raw Wayland protocol
(no wayland-rs), we'll need to add the `zxdg_decoration_manager_v1` and
`zxdg_toplevel_decoration_v1` interfaces to our manual bindings.

Request opcodes:
- `zxdg_decoration_manager_v1.get_toplevel_decoration` = opcode 1
- `zxdg_toplevel_decoration_v1.set_mode` = opcode 1
- `zxdg_toplevel_decoration_v1.unset_mode` = opcode 2

Event opcodes:
- `zxdg_toplevel_decoration_v1.configure` = opcode 0 (carries u32 mode)

Mode values: `1 = client_side`, `2 = server_side`

### X11 Considerations

On X11, `_GTK_FRAME_EXTENTS` should be set when CSD is active, so that tiling window
managers (i3, bspwm, etc.) can properly account for the shadow area. This is currently
not done.

### Effort: Large (~200–400 LoC for Wayland protocol, medium for X11 property)

---

## 8. Task G: Tests for KDE / GNOME CSS Detection

### What

Unit tests for `system_native_linux.rs` that verify:
- GNOME gsettings CSS parsing (GTK theme name, icon theme, button layout)
- KDE detection (colors, font, widget style)
- D-Bus color-scheme / accent-color response parsing

### Challenges

The native discovery code uses `dlopen` and D-Bus / subprocess calls, which can't run in
a normal unit test. We need **parse-only tests** that verify the *parsing logic* given
known input, not the actual OS query.

### Approach

1. **Extract parsing functions** from the discovery code:
   - `parse_gsettings_output(key: &str, stdout: &str) -> Option<String>` — parses the
     gsettings CLI output format (`'value'` with GVariant quoting)
   - `parse_dbus_color_scheme(response_bytes: &[u8]) -> Option<u32>` — parses the raw
     D-Bus wire-protocol response
   - `parse_dbus_accent_color(response_bytes: &[u8]) -> Option<(f64, f64, f64)>`

2. **Test with known payloads:**

   ```rust
   #[test]
   fn test_gnome_gtk_theme_parsing() {
       // gsettings get org.gnome.desktop.interface gtk-theme
       // outputs: 'Adwaita-dark'
       let output = "'Adwaita-dark'\n";
       assert_eq!(parse_gsettings_output("gtk-theme", output), Some("Adwaita-dark".into()));
   }

   #[test]
   fn test_gnome_button_layout_parsing() {
       // Standard GNOME layout
       let output = "'close,minimize,maximize:'\n";
       let (left, right) = parse_button_layout(output);
       assert_eq!(left, vec!["close", "minimize", "maximize"]);
       assert!(right.is_empty());
   }

   #[test]
   fn test_kde_button_layout() {
       // KDE format: XBIAMSH where X=close, B=keep-below, I=minimize, A=maximize, S=shade, H=help
       // kreadconfig5 --group WM --key ButtonsOnLeft → "MS"
       // kreadconfig5 --group WM --key ButtonsOnRight → "HIAX"
       // Should produce: close on right, minimize on right, maximize on right
   }

   #[test]
   fn test_dbus_color_scheme_dark() {
       // Raw D-Bus response for color-scheme=1 (dark)
       let response = build_test_dbus_response_variant_u32(1);
       assert_eq!(parse_dbus_color_scheme(&response), Some(1));
   }

   #[test]
   fn test_dbus_accent_color_gnome46() {
       // GNOME 46+ accent-color as (ddd) triple: (0.21, 0.52, 0.89) = blue
       let response = build_test_dbus_response_accent((0.21, 0.52, 0.89));
       let color = parse_dbus_accent_color(&response).unwrap();
       assert!((color.0 - 0.21).abs() < 0.01);
   }
   ```

3. **Integration tests** (behind `#[cfg(target_os = "linux")]` + feature flag):
   - Actually call `discover_system_style_linux()` and verify it returns *something* 
     reasonable (non-zero colors, non-empty font name, etc.)
   - These would only run on Linux CI or manually

### File Location

- Parse tests: `css/src/system_native_linux.rs` (in `#[cfg(test)] mod tests`)
- Integration tests: `css/tests/system_style_linux_integration.rs`

### Effort: Medium (~150 LoC for parse tests, ~50 LoC for integration tests)

---

## 9. Priority & Dependency Graph

```
    [A] InputMetrics → Gesture       (Small, standalone)
     ↓
    [B] Caret blink/width            (Medium, standalone)
     ↓
    [C] Wheel scroll lines           (Small, standalone)
     ↓
    [D] TextRenderingHints → WR      (Medium, needs [A–C] pattern)
     ↓
    [E] Color Emoji (COLR/SVG)       (Large, independent)
     ↓
    [F] CSD Wayland xdg-decoration   (Large, independent)
     ↓
    [G] KDE/GNOME parse tests        (Medium, standalone)
```

### Recommended Order

| Phase | Tasks | Rationale |
|-------|-------|-----------|
| **Phase 1** | A, B, C | Wire up all InputMetrics. Small, low-risk, immediate user-visible benefit |
| **Phase 2** | D | Text rendering quality. Depends on understanding from Phase 1 |
| **Phase 3** | G | Tests for existing code. Good to validate before adding more features |
| **Phase 4** | F | CSD negotiation. Architectural change, higher risk |
| **Phase 5** | E | Color emoji. Largest feature, most new code, highest risk |

### Total Effort Estimate

| Task | LoC Estimate | Risk |
|------|:---:|:---:|
| A – Gesture InputMetrics | ~50 | Low |
| B – Caret blink/width | ~100 | Low |
| C – Wheel scroll | ~20 | Low |
| D – TextRenderingHints | ~80 | Medium |
| E – Color Emoji | ~300–500 | High |
| F – CSD xdg-decoration | ~200–400 | High |
| G – Parse tests | ~200 | Low |
| **Total** | **~950–1350** | |

---

## Appendix A: Wayland xdg-decoration Protocol Reference

### Globals

| Interface | Version | Compositors |
|-----------|:---:|-------------|
| `zxdg_decoration_manager_v1` | 1 | KWin, Sway, Hyprland, labwc, COSMIC, niri, wlroots-based |

**NOT supported by:** GNOME (Mutter), Weston

### Requests (client → compositor)

```
zxdg_decoration_manager_v1::get_toplevel_decoration(new_id, xdg_toplevel) → decoration
zxdg_toplevel_decoration_v1::set_mode(mode: u32)     // 1=client, 2=server
zxdg_toplevel_decoration_v1::unset_mode()             // let compositor choose
zxdg_toplevel_decoration_v1::destroy()
```

### Events (compositor → client)

```
zxdg_toplevel_decoration_v1::configure(mode: u32)     // 1=client, 2=server
```

### Negotiation Protocol

```
Client                              Compositor
  |                                     |
  |── get_toplevel_decoration() ──────→ |
  |── set_mode(2=server_side) ────────→ |
  |                                     |
  |←── configure(mode=2) ──────────────|  ← compositor agrees (SSD)
  |    OR                               |
  |←── configure(mode=1) ──────────────|  ← compositor overrides (CSD)
  |                                     |
```

## Appendix B: Relevant Environment Variables

| Variable | Affects | Values |
|----------|---------|--------|
| `GDK_BACKEND` | GTK backend selection | `wayland`, `x11`, `broadway` |
| `QT_QPA_PLATFORM` | Qt platform plugin | `wayland`, `xcb`, `wayland;xcb` |
| `QT_WAYLAND_DECORATION` | Qt CSD plugin | `bradient`, `material`, `adwaita` |
| `QT_WAYLAND_DISABLE_WINDOWDECORATION` | Qt decoration kill-switch | `1` |
| `WAYLAND_DISPLAY` | Wayland session detection | socket name or unset |
| `XDG_SESSION_TYPE` | Session type detection | `wayland`, `x11`, `tty` |

## Appendix C: X11 Properties for CSD

| Property | Set By | Purpose |
|----------|--------|---------|
| `_GTK_FRAME_EXTENTS` | Client (CSD) | Shadow area, `CARDINAL[4]` left/right/top/bottom |
| `_NET_FRAME_EXTENTS` | Window Manager (SSD) | Decoration size, EWMH standard |
| `_MOTIF_WM_HINTS` | Client | Request no decoration: `.decorations = 0` |
| `_NET_WM_WINDOW_TYPE` | Client | Influences WM decoration style |

## Appendix D: allsorts Color Font Tables

| Table | Format | allsorts Support | Notes |
|-------|--------|:---:|-------|
| `COLR` v0 | Layered glyphs + CPAL palette | ✅ Basic | Each layer = glyph + color |
| `COLR` v1 | Gradients, compositing, transforms | ❌ | Very complex (Paint tables) |
| `SVG ` | SVG documents per glyph range | ✅ Parse | Feed to resvg for rendering |
| `CBDT`/`CBLC` | Embedded bitmaps (Google emoji) | ❌ | Need allsorts bitmap extraction |
| `sbix` | Apple bitmap emoji | ❌ | PNG images per glyph per size |

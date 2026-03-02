# Scrolling Visual Bug Report — `scrolling.c`

## Overview

The `examples/c/scrolling.c` test renders a 600×500px window with:
- A **blue title bar** ("Regular Scroll Test (no VirtualView)")
- A **scroll container** (`overflow: auto`, `flex-grow: 1`, yellow background `#ffff00`, 3px green border `#00ff00`, 8px margin, `height: 400px`) containing **500 rows** each 30px tall (total content height: 15000px)
- A **grey footer** bar

Rows alternate between `#e8e8e8` (even) and `#ffffff` (odd), with 8px left padding, and display "Row 0", "Row 1", ..., "Row 499".

## Reproduction

```bash
cd tests/e2e && bash test_scrolling_repro.sh
```

This builds the DLL, compiles scrolling.c, starts the app with `AZUL_DEBUG=8766`, captures screenshots (before/after scroll, CPU vs native), display lists, scroll states, and layout data into `tests/e2e/repro_output/`.

## Screenshots

Three screenshots were captured:
1. **`screenshot_before_scroll.png`** — Native (WebRender) rendering, initial state (scroll_y=0)
2. **`screenshot_after_scroll.png`** — Native (WebRender) rendering, scrolled to bottom (scroll_y=14593)
3. **`screenshot_cpu_after_scroll.png`** — CPU software renderer, scrolled to bottom

## Visual Bugs (7 total)

### Bug 1: Row 0 and Row 1 are missing (rows offset by ~60px)

**Expected:** The first visible row should be "Row 0" at the very top of the scroll container.
**Actual (native):** The first visible row is "Row 2". "Row 0" and "Row 1" are never shown — they appear to be clipped/hidden above the visible area.
**CPU renderer:** Shows "Row 0" correctly at the top.

**Likely cause:** The `PushClip` / `PushScrollFrame` origin is at `(11, 53)` (which is `margin(8) + border(3) = 11` for X, and `title_height(~44) + margin(8) + border(3) ≈ 55` for Y). Either the content starts at a wrong offset relative to the clip origin, or the clip origin is miscalculated by approximately 2 row heights (60px).

### Bug 2: Yellow background bleeds through on the right side

**Expected:** Rows should fill the entire width of the scroll container (border to border), or at minimum, no yellow gap should be visible.
**Actual (native):** There's a ~16px wide **yellow vertical stripe** between the right edge of the rows and the green border. This is the container's `background: #ffff00` showing through.
**CPU renderer:** No yellow bleed — rows fill the width correctly.

**Likely cause:** The content width inside the scroll frame is narrower than the clip width, possibly due to scrollbar width being subtracted from the content area even though no scrollbar is rendered. The `scrollbar_info.scrollbar_width` may be reducing the clip rect width, but the scrollbar itself is not drawn.

### Bug 3: Yellow gap at the bottom when scrolled to end

**Expected:** When scrolled to the very bottom, "Row 499" should be at or near the bottom of the container.
**Actual (native):** There's ~90px of **yellow background** below "Row 499" at the bottom of the container.
**CPU renderer:** Shows rows filling to the bottom with no yellow gap.

**Likely cause:** The `max_scroll_y` calculation (14593.289) doesn't account for some offset, or the content height in the scroll frame doesn't match the actual combined row heights.

### Bug 4: No scrollbar visible despite `overflow: auto`

**Expected:** A vertical scrollbar should appear since content (15000px) vastly exceeds container height (~407px).
**Actual (native):** No scrollbar thumb or track is visible anywhere.
**CPU renderer:** Also no scrollbar, suggesting this is a display list generation issue, not renderer-specific.

**Key data:** The repro script queries `get_scrollbar_info` for node_id 1 and gets `found: false`. The actual scroll container is node_id 3. The scrollbar geometry computation in `display_list.rs` uses `compute_scrollbar_geometry()` (line ~3092) — need to check if scrollbar draw items are actually emitted into the display list.

**Files to check:**
- `layout/src/solver3/display_list.rs` around line 3092 (`push_scrollbar_styled`) — where scrollbar display list items are generated
- `layout/src/solver3/taffy_bridge.rs` — `compute_taffy_scrollbar_info()` function which determines if a scrollbar is needed

### Bug 5: CPU render differs from native (WebRender) render

**Expected:** CPU and native renders should produce identical output.
**Actual:** CPU renderer (screenshot_cpu_after_scroll.png) shows correct rendering:
- Row 0 visible at top (before scroll)
- No yellow bleed
- Rows fill the container width
- Correct text positioning

Native WebRender renderer has all the bugs listed above. This suggests the display list is correct, but the WebRender translation (display list → WebRender commands) has coordinate/sizing issues.

**Files to check:**
- The WebRender display list translation code that converts our `DisplayList` into WebRender's `BuiltDisplayList`

### Bug 6: Text clipped on the left edge

**Expected:** Each row should show "Row N" with full text visible (8px left padding).
**Actual (native):** The first character is partially or fully clipped — e.g., "ow 2" instead of "Row 2". The "R" is cut off.
**CPU renderer:** Shows full "Row N" text including the "R".

**Likely cause:** The text origin in the display list is at `x=19` (which is `margin(8) + border(3) + padding(8) = 19`), but the clip rect starts at `x=11` (margin + border). This means ~8px of padding should be available. The native renderer may be applying an incorrect transform or clip offset that shifts the text leftward.

### Bug 7: Inner "padding" effect around rows (border-box issue)

**Expected:** Rows should start right at the inner edge of the green border, with no gap between the border and the first row.
**Actual (native):** There appears to be a thick yellow gap between the green border and the rows on all sides (top, left, right), creating a "padding" effect that doesn't exist in the CSS.
**CPU renderer:** No such gap.

**Likely cause:** The clip rect or scroll frame content area has incorrect origin/size relative to the container's border-box. The content should start at `border-left + border-top` inside the container, but it may be double-counting some offset.

## Key Data from Repro Output

### Display List Structure (3021 items)
```
PushClip(bounds=(11.0, 53.0, 578.0, 407.0))   ← Container clip
  PushScrollFrame(clip=(11.0, 53.0, 578.0, 407.0), content=(578.0, 15000.0), id=3)
    // 500 rows: alternating PushRect + PushTextRun
    PushRect(bounds=(11.0, 53.0, 578.0, 30.0))       ← Row 0 background
    PushTextRun(origin=(19.0, 53.0), ...)             ← Row 0 text "Row 0"
    PushRect(bounds=(11.0, 83.0, 578.0, 30.0))       ← Row 1
    PushTextRun(origin=(19.0, 83.0), ...)
    ...
    PushRect(bounds=(11.0, 15023.0, 578.0, 30.0))    ← Row 499
    PushTextRun(origin=(19.0, 15023.0), ...)
  PopScrollFrame
PopClip
```

### Scroll State
```json
{
  "node_id": 3,
  "scroll_x": 0.0,
  "scroll_y": 14593.0,
  "content_width": 578.0,
  "content_height": 15000.0,
  "container_width": 578.0,
  "container_height": 406.7,
  "max_scroll_x": 0.0,
  "max_scroll_y": 14593.289
}
```

### Scrollable Node Info
```json
{
  "node_id": 3,
  "bounds": { "x": 8.0, "y": 50.0, "width": 584.0, "height": 412.7 },
  "can_scroll_x": false,
  "can_scroll_y": true
}
```

## Key Source Files

| File | Lines | Description |
|------|-------|-------------|
| `examples/c/scrolling.c` | 115 | Test app: 500-row overflow:auto container |
| `layout/src/solver3/display_list.rs` | 5170 | Display list generation, scrollbar rendering, clip/scroll frame emission |
| `layout/src/solver3/taffy_bridge.rs` | 2057 | CSS→taffy layout bridge, overflow handling, scrollbar computation |
| `dll/src/desktop/shell2/common/debug_server.rs` | 10006 | Debug HTTP server for automated inspection |
| `tests/e2e/test_scrolling_repro.sh` | 347 | Reproduction script that captures all debug data |

### Key functions in `display_list.rs`:
- `generate_display_list()` (line 1293) — Entry point for display list generation
- `push_node_clips()` (line 2258) — Decides whether to push PushClip / PushScrollFrame for overflow nodes
- `pop_node_clips()` (line 2349) — Pops matching clip/scroll commands
- `push_scrollbar_styled()` (line 872) — Emits scrollbar drawing commands
- Lines ~3092 and ~3170 — Where scrollbar_styled is actually called

### Key functions in `taffy_bridge.rs`:
- `azul_overflow_to_taffy()` — Converts CSS overflow to taffy overflow enum
- `compute_taffy_scrollbar_info()` — Computes whether scrollbar is needed and its dimensions
- `translate_style_to_taffy()` — Translates full CSS style to taffy::Style including overflow

## Debugging Commands

With the app running (`AZUL_DEBUG=8766`):

```bash
# Get scroll states
curl -s -X POST http://localhost:8766/ -d '{"op": "get_scroll_states"}'

# Get display list
curl -s -X POST http://localhost:8766/ -d '{"op": "get_display_list"}'

# Get scrollbar info for the scroll container (node_id 3, not 1!)
curl -s -X POST http://localhost:8766/ -d '{"op": "get_scrollbar_info", "node_id": 3}'

# Get layout for specific node
curl -s -X POST http://localhost:8766/ -d '{"op": "get_node_layout", "node_id": 3}'

# Take screenshots
curl -s -X POST http://localhost:8766/ -d '{"op": "take_native_screenshot"}'
curl -s -X POST http://localhost:8766/ -d '{"op": "take_screenshot"}'

# Scroll to a position
curl -s -X POST http://localhost:8766/ -d '{"op": "scroll_node_to", "node_id": 3, "x": 0, "y": 0}'
```

## Root Cause Hypothesis

The display list coordinates look correct (Row 0 at y=53, content 578×15000), and the CPU renderer produces correct output. This suggests the issue is in how the display list is **translated to WebRender commands**, specifically:

1. The PushScrollFrame content origin might be offset, causing rows to render 60px too high (hiding Row 0 and Row 1)
2. Scrollbar width is being reserved in the clip rect but no scrollbar is drawn
3. The coordinates in the display list (absolute window coordinates) may need adjustment when being pushed into WebRender's scroll frame (which uses content-relative coordinates)

The main area to investigate is the WebRender display list translation — where our `DisplayList` items are converted to `webrender::api::DisplayListBuilder` calls.

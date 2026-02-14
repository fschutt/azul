# Azul Scroll Architecture

## Overview

This document describes the scroll architecture in Azul: how scroll frames work,
why there is no viewport-level scrolling, how "scroll clip size" differs from
"content size", and how virtualized scrolling (IFrameCallback) fits in.

---

## 1. Key Concepts

### 1.1 Scroll Clip Size vs Content Size

```
┌───────────────────────────────────┐
│         Virtual Scroll Frame      │  ← iframe_virtual_scroll_size
│                                   │    (infinite scroll: total logical height,
│  ┌─────────────────────────────┐  │     may be larger than actual content)
│  │       Scroll Frame          │  │  ← content_size (overflow_content_size)
│  │    (actual content area)    │  │    (total height of rendered children)
│  │                             │  │
│  │  ┌───────────────────────┐  │  │
│  │  │    Screen Bounds      │  │  │  ← scroll clip size (= container inner size)
│  │  │   (visible window)    │  │  │    This is what the user sees.
│  │  │                       │  │  │
│  │  │   ← scroll offset →  │  │  │    Scroll offset shifts which part
│  │  │                       │  │  │    of content_size is visible in
│  │  └───────────────────────┘  │  │    the clip rect.
│  │                             │  │
│  └─────────────────────────────┘  │
│                                   │
└───────────────────────────────────┘
```

**Three sizes:**

| Term | Definition | Where in code |
|------|-----------|---------------|
| **Scroll clip size** | The visible area — the container's inner box (border-box minus borders minus scrollbar track). This is the "viewport" of the scroll. | `clip_rect` in `push_node_clips()` ([display_list.rs](layout/src/solver3/display_list.rs#L2150)) |
| **Content size** | The total size of all children — the area that can be scrolled through. | `overflow_content_size` on `LayoutNode`, returned by `get_scroll_content_size()` ([display_list.rs](layout/src/solver3/display_list.rs#L3509)) |
| **Virtual scroll size** | For lazy/infinite scroll: the *logical* total size, which may be larger than the actually-rendered content. Tracked by `IFrameManager`. | `iframe_virtual_scroll_size` in [iframe.rs](layout/src/managers/iframe.rs#L54) |

**The scrollbar ratio** is:

$$\text{thumb\_size\_ratio} = \frac{\text{scroll clip size}}{\text{content size}}$$

$$\text{thumb\_position\_ratio} = \frac{\text{scroll offset}}{\text{content size} - \text{scroll clip size}}$$

If `scroll clip size >= content size`, the scrollbar thumb fills the entire track
(ratio = 1.0) → no scrolling needed.

### 1.2 PushScrollFrame

The display list emits `PushScrollFrame` items ([display_list.rs](layout/src/solver3/display_list.rs#L549)):

```rust
PushScrollFrame {
    clip_bounds: LogicalRect,   // = scroll clip size (visible area)
    content_size: LogicalSize,  // = total scrollable content size
    scroll_id: LocalScrollId,   // renderer-agnostic tracking ID
}
```

WebRender translates this into `define_scroll_frame()` ([compositor2.rs](dll/src/desktop/compositor2.rs#L795)):

```rust
// frame_rect  = clip_bounds  → the visible viewport (clip)
// content_rect = content_size → the total scrollable area
let scroll_spatial_id = builder.define_scroll_frame(
    parent_space,
    external_scroll_id,
    content_rect,           // total scrollable content
    adjusted_frame_rect,    // visible clip area
    LayoutVector2D::zero(), // external_scroll_offset
    ...
);
```

**Important**: In WebRender, a scroll frame is purely a *spatial transform* — it
shifts child coordinates by the scroll offset. A **separate clip** is needed to
actually hide content outside the viewport. Azul does both in `push_node_clips()`:

```rust
// 1. Push clip (hides overflow)
builder.push_clip(clip_rect, border_radius);
// 2. Push scroll frame (enables scrolling transform)
builder.push_scroll_frame(clip_rect, content_size, scroll_id);
```

---

## 2. Why There Is No Viewport-Level Scrolling

### 2.1 Azul Is Not a Browser

In a browser, the **viewport** itself is scrollable: `<html>` or `<body>` can overflow
the window and the browser provides a built-in viewport scrollbar. This is a special
case baked into the browser's rendering pipeline.

In Azul:
- The **window** is a fixed-size rectangle. There is no concept of "the window itself scrolls".
- The **root `<html>` node** gets `width: 100%; height: 100%` of the window
  (set via UA CSS in [ua_css.rs](core/src/ua_css.rs)).
- A **CSD titlebar** is injected as a direct child of `<html>`, before `<body>`.
- The **`<body>`** gets `display: block; margin: 8px` and `height: auto`.

The DOM tree looks like:

```
<html>  ← 850×900 (window size via height:100%)
├── <div class="csd-titlebar">  ← 850×28
└── <body>  ← 834×??? (height:auto, 8px margin each side)
    └── <div class="grid">  ← user content
```

Since `<html>` is the window, it CANNOT scroll — if it did, the titlebar would
scroll away. Scrolling must happen on `<body>` or a descendant.

### 2.2 The Architecture

```
┌────────────────────────────────────┐
│ <html> (window size, no overflow)  │
│ ┌────────────────────────────────┐ │
│ │ <div.csd-titlebar> (28px)      │ │
│ └────────────────────────────────┘ │
│ ┌────────────────────────────────┐ │
│ │ <body> (overflow: scroll)      │ │  ← THIS is the scroll container
│ │ ┌────────────────────────────┐ │ │
│ │ │        clip rect           │ │ │  ← scroll clip size = body inner box
│ │ │  (visible content area)    │ │ │
│ │ │                            │ │ │
│ │ │   Scroll content extends   │ │ │
│ │ │   beyond this clip...      │ │ │
│ │ └────────────────────────────┘ │ │
│ │              ↕ scrollbar       │ │
│ └────────────────────────────────┘ │
└────────────────────────────────────┘
```

The scroll clip size for `<body>` is its **inner box** — i.e., whatever size `<body>`
resolves to (from its containing block = `<html>`) minus borders and scrollbar track.
The content size is the total height of `<body>`'s children.

---

## 3. The Current Bug: Body Expands to Content Height

### 3.1 What Happens Today

The layout pipeline in `calculate_layout_for_subtree()` ([cache.rs](layout/src/solver3/cache.rs#L1561)) runs these phases:

```
Phase 1: prepare_layout_context()
  → Resolves CSS properties to used_size
  → For height:auto → uses intrinsic height (initially 0 or min-height)
  → Sets available_size_for_children.height = containing_block.height

Phase 2: layout_formatting_context()
  → Runs the formatting context (block/flex/grid)
  → Returns content_size = total size of children

Phase 2.5: apply_content_based_height()           ← THE PROBLEM
  → For height:auto: final_size = max(old_size, content_size)
  → Body with height:auto → expands to content height (3632px!)

Phase 3: compute_scrollbar_info()
  → Compares content_size vs container_size (= inner box of final_used_size)
  → But final_used_size was ALREADY expanded to content_size
  → So container ≈ content → scrollbar thinks nothing overflows!
```

**Result**: `<body>` grows from ~860px (what it should be: html_height - titlebar - margins)
to 3632px (content height). The scrollbar sees `container_size ≈ content_size` and
reports `thumb_size_ratio ≈ 1.0`.

### 3.2 The Key Insight

**We don't need `<body>` to "fill remaining space" in `<html>`.** The body can have
whatever height it naturally resolves to. What matters is:

1. **Scroll clip size** = the body's resolved inner box height (from containing block, NOT from content)
2. **Content size** = the total height of body's children  
3. **Scrollbar** = scroll clip size / content size

If the body has `height: auto` and `overflow: scroll`, the body should NOT expand
to content height. The body's height should be determined by its containing block
(or min-height/max-height). The content overflows, and that's what the scrollbar
reports.

### 3.3 What CSS Specifies

From CSS 2.2 § 10.7 (heights of block-level elements):

> If the height is 'auto', the height depends on whether the element has any
> block-level children and whether it has padding or borders.

But this is for `overflow: visible`. For `overflow: scroll/auto/hidden`, CSS 2.1 § 11.1
says the element establishes a new block formatting context, and the overflow
content is clipped / scrollable — the element does NOT grow to fit.

In practice, browsers resolve `height: auto` + `overflow: scroll` like this:
- The element's height = determined by normal flow rules (containing block, min/max)
- Content that exceeds this height is scrollable, not a reason to grow

### 3.4 The Fix

In `calculate_layout_for_subtree()`, Phase 2.5 (`apply_content_based_height`) should
**not expand** the node if it has `overflow: scroll` or `overflow: auto`. The content
size is tracked separately in `overflow_content_size` for the scroll frame.

```rust
// Phase 2.5: Only apply content-based height for overflow:visible
if should_use_content_height(&css_height) && !has_scroll_overflow {
    final_used_size = apply_content_based_height(...);
}
```

With this fix:
- `<body>` stays at its containing-block-derived height (~860px)
- `overflow_content_size` = 3632px (unchanged, stored on the node)
- Display list emits `PushScrollFrame { clip: 860px, content: 3632px }`
- Scrollbar: `thumb_size_ratio = 860 / 3632 ≈ 0.24` → visible, functional scrollbar

---

## 4. Virtualized Scrolling (IFrameCallback)

### 4.1 Overview

For large datasets (think 100,000 rows), rendering all content upfront is too expensive.
Azul supports **virtualized scrolling** via `IFrameCallback` — a user-provided function
that generates DOM content on demand based on the visible window.

The `IFrameManager` ([iframe.rs](layout/src/managers/iframe.rs)) orchestrates this:

```
┌─────────────────────────────────────────────┐
│        Virtual Scroll Frame                 │
│   (iframe_virtual_scroll_size = 10,000px)   │
│                                             │
│   ┌─────────────────────────────────────┐   │
│   │    Rendered Content                 │   │
│   │  (iframe_scroll_size = 2,000px)     │   │
│   │  Only rows near the viewport are    │   │
│   │  actually in the DOM.               │   │
│   │                                     │   │
│   │   ┌─────────────────────────────┐   │   │
│   │   │   Viewport (clip rect)      │   │   │
│   │   │   = container inner box     │   │   │
│   │   │   ~500px visible            │   │   │
│   │   └─────────────────────────────┘   │   │
│   │                                     │   │
│   │   ↑ EDGE_THRESHOLD = 200px          │   │
│   │     When scroll offset gets within  │   │
│   │     200px of an edge, IFrameManager │   │
│   │     triggers EdgeScrolled callback  │   │
│   │     → user generates more rows      │   │
│   │                                     │   │
│   └─────────────────────────────────────┘   │
│                                             │
└─────────────────────────────────────────────┘
```

### 4.2 IFrame Lifecycle

1. **InitialRender**: First time the IFrame is laid out. The callback receives the
   container bounds and returns initial DOM content + `iframe_scroll_size` +
   `iframe_virtual_scroll_size`.

2. **BoundsExpanded**: The container grew larger than the content (e.g., window resize).
   The callback regenerates content for the new size.

3. **EdgeScrolled(edge)**: The user scrolled within `EDGE_THRESHOLD` (200px) of an edge.
   For infinite scroll, this means "load more rows". The callback returns updated DOM
   content with the new rows added.

### 4.3 scroll_size vs virtual_scroll_size  

- `iframe_scroll_size`: The size of the **actually rendered** DOM content. This is what
  the layout engine computes from the children that exist.
  
- `iframe_virtual_scroll_size`: The **total logical size** that the scrollbar should
  represent. For a table with 100,000 rows at 20px each, this would be 2,000,000px —
  even though only ~50 rows are rendered.

The scrollbar uses `virtual_scroll_size` (if set) instead of `scroll_size`:

$$\text{thumb\_size\_ratio} = \frac{\text{clip size}}{\text{virtual\_scroll\_size}}$$

This gives the user accurate scrollbar feedback about how much total content exists,
even though most of it isn't in the DOM.

### 4.4 Relation to Scroll Clip Size

For IFrame-based virtualized scrolling, the same principle applies:

- **Scroll clip size** = the IFrame container's inner box (fixed, determined by layout)
- **Content size** = `iframe_virtual_scroll_size` (total logical content)
- **Rendered content** = `iframe_scroll_size` (subset actually in DOM)

The IFrame callback is responsible for rendering the correct subset based on the
scroll offset. The layout engine provides the clip rect; the callback decides what
content to put in it.

---

## 5. Scrollbar Calculation Pipeline

```
                   ┌─────────────────────┐
                   │  Layout Engine       │
                   │  (cache.rs)          │
                   │                      │
                   │  Computes:           │
                   │  • used_size         │  ← container border-box
                   │  • overflow_content  │  ← total children extent
                   │    _size             │
                   └──────────┬──────────┘
                              │
                   ┌──────────▼──────────┐
                   │  compute_scrollbar  │
                   │  _info()            │
                   │                      │
                   │  container_inner =   │
                   │  used_size - borders │
                   │                      │
                   │  Compares vs         │
                   │  content_size        │
                   │                      │
                   │  → ScrollbarReqs     │
                   │    {needs_h, needs_v,│
                   │     width, height}   │
                   └──────────┬──────────┘
                              │
                   ┌──────────▼──────────┐
                   │  Display List        │
                   │  (display_list.rs)   │
                   │                      │
                   │  push_node_clips():  │
                   │  • push_clip(clip)   │  ← clips overflow
                   │  • push_scroll_frame │  ← enables scroll transform
                   │    (clip, content,   │
                   │     scroll_id)       │
                   └──────────┬──────────┘
                              │
              ┌───────────────┴───────────────┐
              │                               │
   ┌──────────▼──────────┐       ┌────────────▼───────────┐
   │  WebRender           │       │  CPU Renderer           │
   │  (compositor2.rs)    │       │  (cpurender.rs)         │
   │                      │       │                         │
   │  define_scroll_frame │       │  PushScrollFrame →      │
   │  define_clip_rect    │       │  simple clip (no scroll │
   │  define_clip_chain   │       │  offset applied; TODO)  │
   │                      │       │                         │
   │  WebRender handles   │       └─────────────────────────┘
   │  scroll offset via   │
   │  spatial transform   │
   └──────────────────────┘
```

### 5.1 ScrollManager

The `ScrollManager` ([scroll_state.rs](layout/src/managers/scroll_state.rs)) tracks per-node scroll state:

- **AnimatedScrollState**: Current offset, target offset, easing animation
- **ScrollbarState**: Geometry for rendering (thumb_position_ratio, thumb_size_ratio, track_rect)
- Handles smooth scrolling via easing functions
- Hit-testing on scrollbar components (track, thumb, buttons)

The scroll offset flows from user input → ScrollManager → WebRender's
`external_scroll_offset` / `set_scroll_offset()`.

---

## 6. Summary: What Needs to Change

| Component | Current Behavior | Correct Behavior |
|-----------|-----------------|------------------|
| `<html>` UA CSS | `height: auto` → grows to content | `height: 100%` → window size ✅ (already fixed) |
| `apply_content_based_height` | Always expands `height:auto` nodes to content | Skip expansion when `overflow: scroll/auto` |
| `compute_scrollbar_info` | Runs after expansion, sees container≈content | Runs after correct sizing, sees container < content |
| PushScrollFrame | `clip_bounds` = expanded body size | `clip_bounds` = body's containing-block-derived size |
| Scrollbar thumb | `thumb_size_ratio ≈ 1.0` (useless) | `thumb_size_ratio = clip/content` (functional) |

The fix is in **one place**: `calculate_layout_for_subtree()` Phase 2.5.
Don't call `apply_content_based_height()` for nodes with `overflow: scroll/auto`.

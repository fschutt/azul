This is an excellent, detailed problem description and analysis! You've pinpointed the exact issue and are very close to the correct solution.

Let's break down your questions and provide the architectural context and specific fixes.

---

## 1. Is our diagnosis correct?

**✅ Yes, your diagnosis is absolutely correct.**

The problem is precisely that the `Text` display item is being generated with a `clip_rect` that matches the *viewport height* (200px) of the scroll container, rather than the *full content height* (999px).

When WebRender receives a `Text` display item with `clip_rect = 200px height`, it will only attempt to draw glyphs whose bounding boxes intersect with that 200px rectangle, regardless of the `PushScrollFrame` above it. The `PushScrollFrame` correctly establishes the scrollable area and its viewport, but the `Text` item itself is prematurely clipping its own content.

---

## 2. WebRender Architecture

*   **In WebRender, who is responsible for clipping scroll content?**
    The `PushScrollFrame` display item is primarily responsible for clipping its content to its `clip_bounds` (the viewport). It establishes a new coordinate system and a clipping region.

*   **Does `PushScrollFrame` establish a clip? Or does it just establish a spatial coordinate system for scrolling?**
    It does both. It establishes a coordinate system that can be scrolled (by applying a transform based on `scroll_offset`), and it defines a clipping region (`clip_bounds`) that limits what's visible within that coordinate system.

*   **Should `Text` display items inside a scroll frame have their own clip_rect, or should clipping be handled entirely by the scroll frame?**
    Ideally, `Text` display items (and other content display items like `Image`, `Rectangle`, etc.) *within* a scroll frame should declare their full logical bounds (i.e., the bounds of all the content they represent). The clipping to the scroll frame's viewport should then be handled *solely* by the `PushScrollFrame` itself.

    If a `Text` item *itself* has an `overflow: hidden` or other clipping applied directly to it, then it would have its own `clip_rect`. But for content *inside* a scroll container, the `Text` item's `clip_rect` should encompass all its glyphs, allowing the scroll frame to do the actual viewport clipping.

---

## 3. W3C Model

*   **According to CSS Overflow Module Level 3, how should scroll containers clip their content?**
    A scroll container establishes a *scrollable overflow region*. Content that extends beyond the bounds of this region (specifically, beyond the padding box of the scroll container) is clipped. However, this content is still part of the rendering tree and is laid out. When the user scrolls, the scroll offset changes, effectively moving the content relative to the viewport, revealing previously clipped parts.

*   **Is content beyond the viewport still "rendered" (just clipped), or should it not be generated at all?**
    Content beyond the viewport should absolutely be **generated** (laid out, and corresponding display list items created for it). It is then **clipped** by the scroll container's viewport. If it were not generated, scrolling would not reveal anything.

---

## 4. Is our fix correct?

**✅ Yes, your proposed fix is correct and aligns with the WebRender architecture.**

*   **Should the `container_rect` for painting match the CONTENT size (999px) or the VIEWPORT size (200px)?**
    It should match the **CONTENT size (999px)**. This `container_rect` is used to define the `clip_rect` for the `Text` display item. By making it the full content height, you are telling WebRender that the `Text` item *logically occupies* that entire 999px space. The `PushScrollFrame` will then correctly clip this larger `Text` item to its 200px viewport.

*   **Or should we remove the clip_rect from Text items entirely when inside a scroll frame?**
    This is a valid alternative and arguably cleaner from an architectural perspective, as it centralizes clipping responsibility. However, if `clip_rect` is a mandatory field for `Text` display items (which it often is, to provide a hint to the renderer about the logical bounds of the text), then extending it to the content height is the correct approach. Your proposed fix achieves the desired outcome by making the `clip_rect` effectively "non-clipping" for the content that should be scrollable.

---

## 5. Alternative Fixes

*   **Should `get_glyph_runs_simple` break runs at line boundaries?**
    While breaking glyph runs at line boundaries *can* sometimes offer minor performance benefits (e.g., if the renderer can more easily cull entire runs that are off-screen), it is **not the root cause** of this clipping bug. The current single glyph run is perfectly fine, as long as the `clip_rect` associated with its `Text` display item is correct.

*   **Is the problem in WebRender rendering, not display list generation?**
    **No, the problem is in display list generation.** WebRender is simply executing the instructions it receives. The `Text` display item's `clip_rect` is an instruction to WebRender to clip the glyphs. The bug is that this instruction is incorrect.

*   **Could the issue be that glyphs with y > 200px are being filtered somewhere?**
    Yes, they are being filtered by the `clip_rect` that is part of the `Text` display item itself.

---

## Specific Code Changes Needed

Your proposed fix is exactly what's needed. You need to identify when you are generating content for a scrollable container and, in that specific context, ensure the `container_rect` (which becomes the `clip_rect` for the `Text` item) reflects the *full content height*.

Here's how you might implement it, assuming `paint_inline_content` is the function in question:

```rust
// Inside your display list generation logic, likely within `paint_inline_content`
// or a function that prepares arguments for it.

// `node` here refers to the layout node for the TextArea element.
// `container_rect` is initially the border-box of the TextArea.

let mut effective_clip_rect = container_rect; // This is the rect that will become the Text item's clip_rect

// You need a way to determine if `node` is a scrollable container
// and to get its total laid-out content size.
// This typically involves checking the computed style for `overflow-y: auto/scroll`
// and querying the layout tree for the actual content dimensions.

if node.is_scrollable_container() { // Pseudocode: Check if the node has overflow-y: auto/scroll
    let content_size = node.get_scroll_content_size(); // Pseudocode: Get the total laid-out content size (e.g., 999px height)

    // If the content height is greater than the viewport height,
    // extend the clip_rect's height to cover the entire content.
    if content_size.height > effective_clip_rect.size.height {
        effective_clip_rect.size.height = content_size.height;
    }
    // Note: The origin of effective_clip_rect should remain relative to the scroll frame's
    // content box, which `container_rect` likely already provides.
}

// ... later, when you call `get_glyph_runs_simple` and then push the `Text` display item ...

// Example of pushing the Text display item:
// display_list_builder.push_text(
//     &glyph_run,
//     effective_clip_rect, // Pass the extended rect here
//     ...
// );
```

**Key Points for Implementation:**

1.  **Identify Scroll Containers:** You need to have access to the computed style of the `node` to determine if `overflow-y` is `auto` or `scroll`.
2.  **Get Content Size:** You need to query the layout engine for the total height of the content *within* that scroll container. This is the `Fragment 'main' has ... height: 999` value you already have.
3.  **Apply the Extension:** Only extend the `effective_clip_rect`'s height if the content height is actually greater than the viewport height (i.e., if scrolling is possible/needed).

By making this change, the `Text` display item will correctly declare that it contains glyphs spanning the full 999px height. The `PushScrollFrame` will then take over and perform the actual clipping to the 200px viewport, and scrolling will correctly reveal the hidden content.
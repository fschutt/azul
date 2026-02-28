# IFrame Investigation Report

**Date:** 2026-02-26  
**Symptom:** Flickering between grey-white rows and yellow rectangle on resize  
**Related commit:** `44e900ad` (UA CSS: IFrame display:block)  
**Test case:** `examples/c/infinity.c` — 4M virtual rows via IFrameCallback  
**Status:** ALL BUGS FIXED (see §10 below)

---

## 1. Executive Summary

The IFrame rendering had **three bugs** that together produced the flickering:

| # | Bug | Impact | Status |
|---|-----|--------|--------|
| 1 | **`layout_results` cleared but IFrameManager flags not reset** — IFrame child DOM wiped on resize, `check_reinvoke()` returns `None` because `was_invoked=true`. | Child DOM alternates between present and absent. Yellow background flicker. | ✅ FIXED |
| 2 | **`CallbackChange::ScrollTo` does NOT check IFrame reinvocation** — Scrolling never re-invokes the IFrame callback, so virtual scroll (lazy loading at edges) does not work. | Virtual scrolling broken — only initial 100-row chunk visible. | ✅ FIXED |
| 3 | **`calculate_scrollbar_states()` and `is_node_scrollable()` should use `virtual_scroll_size`** — Listed in scroll6_report as broken. | Incorrect scrollbar thumb size and scrollability. | ✅ ALREADY FIXED (pre-existing) |

**Bug #1 was the root cause of the flickering.**

---

## 2. Full Lifecycle: How IFrameCallback Gets Invoked

### 2.1 Startup (First Frame)

```
main()
  → AzApp_run()
    → create_window()
      → regenerate_layout()                              [macos/mod.rs:2816]
        → layout_callback(app_data)                      [layout.rs:170]
          → user's layout() returns StyledDom (body + iframe + footer)
        → layout_window.layout_and_generate_display_list()  [layout.rs:370]
          → self.layout_results.clear()                  [window.rs:571]  ← CLEARS ALL
          → self.layout_dom_recursive(root_dom, ...)     [window.rs:585]
            → solver3::layout_document(styled_dom)       [window.rs:764]
            → scan_for_iframes()                         [window.rs:806]
              → finds node_id=3 is NodeType::IFrame
            → invoke_iframe_callback_with_dom(dom_id=0, node_id=3, bounds, ...)
              → invoke_iframe_callback_impl()            [window.rs:1064]
                → iframe_manager.check_reinvoke()        [iframe.rs:212]
                  → was_invoked=false → InitialRender
                → user's render_rows() callback          [window.rs:1143]
                  → returns StyledDom with 100 rows + scroll_size + virtual_scroll_size
                → iframe_manager.mark_invoked()          [window.rs:1146]
                → iframe_manager.update_iframe_info()    [window.rs:1180]
                → scroll_manager.update_virtual_scroll_bounds()  [window.rs:1185]
                → self.layout_dom_recursive(child_dom, ...)  ← RECURSIVE
                  → solver3::layout_document(child_styled_dom)
                  → self.layout_results.insert(dom_id=1, DomLayoutResult{...})  [window.rs:827]
                  → returns Ok(())
              → returns Some(child_dom_id=1)
            → display_list.push(DisplayListItem::IFrame { child_dom_id=1, bounds, clip })
            → self.layout_results.insert(dom_id=0, DomLayoutResult{...})  [window.rs:827]
          → returns Ok(())
        → register scrollable nodes                      [layout.rs:382]
        → calculate_scrollbar_states                     [layout.rs:470]
      → frame_needs_regeneration = false
      → display_list_initialized = false  (stays false until first drawRect:)
```

Then in `render_and_present_in_draw_rect()`:
```
drawRect:
  → frame_needs_regeneration = false (it was cleared above)
  → display_list_needs_rebuild = false (DOM unchanged, is_layout_equivalent=true)
  → BUT: !display_list_initialized → force display_list_needs_rebuild = true  [mod.rs:4581]
  → build_webrender_transaction()                         [wr_translate2.rs:2256]
    → iterates layout_results (dom_id 0, dom_id 1)
    → for dom_id 0: translate_displaylist_to_wr()
      → encounters DisplayListItem::IFrame { child_dom_id=1 }
      → recursively translates dom_id 1 → nested_pipelines
    → txn.set_display_list(pipeline(0, ...), root_display_list)
    → txn.set_display_list(pipeline(1, ...), child_display_list)  ← from nested
    → for dom_id 1: translate_displaylist_to_wr()
    → txn.set_display_list(pipeline(1, ...), child_display_list)  ← DUPLICATE
    → scroll_all_nodes()  → sets scroll offsets for all DOMs
    → generate_frame()
  → display_list_initialized = true
  → WebRender renders: shows grey-white rows ✓
```

**Result: First frame works correctly.** layout_results has {0, 1}. Both display lists submitted.

---

### 2.2 Resize (Subsequent Frames) — THE BUG

```
windowDidResize:
  → new_logical_width/height updated                     [mod.rs:1406]
  → frame_needs_regeneration = true                      [mod.rs:1416]
  → request_redraw()

drawRect:
  → frame_needs_regeneration = true
  → regenerate_layout()                                  [mod.rs:4559]
    → layout_callback(app_data)
      → user's layout() returns StyledDom (same structure)
    → window_size_changed = true  (dimensions differ)       [layout.rs:282]
    → skips is_layout_equivalent optimization              [layout.rs:288]
    → layout_window.layout_and_generate_display_list()
      → ★ self.layout_results.clear() ★                   [window.rs:571]
        → dom_id 0 and dom_id 1 are BOTH removed
      → self.layout_dom_recursive(root_dom, ...)
        → solver3::layout_document() — re-layouts root DOM with new window size
        → scan_for_iframes() — finds node_id=3 is IFrame
        → invoke_iframe_callback_with_dom(dom_id=0, node_id=3, new_bounds, ...)
          → invoke_iframe_callback_impl()
            → iframe_manager.check_reinvoke(dom_id=0, node_id=3, ...)
              → ★ was_invoked = true ★ (set by mark_invoked in 2.1)
              → check bounds expansion: new_bounds vs last_bounds
              → IF new_bounds expanded → BoundsExpanded → callback runs ✓
              → IF new_bounds same or smaller → check_reinvoke_condition()
                → checks EdgeScrolled conditions
                → IF not near edge → returns None
                → ★ CALLBACK NOT INVOKED ★
            → returns self.iframe_manager.get_nested_dom_id() → Some(dom_id=1)
          → returns Some(dom_id=1)
        → display_list.push(IFrame { child_dom_id=1, bounds, clip })
        → self.layout_results.insert(dom_id=0, ...)       [window.rs:827]
      → returns Ok(())
    ★★★ CRITICAL: layout_results = {0: DomLayoutResult} — dom_id 1 IS MISSING ★★★
```

Then when building the WebRender transaction:
```
  → display_list_needs_rebuild = true (layout changed)
  → build_webrender_transaction()
    → iterates layout_results: only dom_id 0
    → for dom_id 0: translate_displaylist_to_wr()
      → encounters DisplayListItem::IFrame { child_dom_id=1, ... }
      → layout_results.get(&dom_id_1) → None !!
      → logs "WARNING: Child DOM 1 not found in layout_results"
      → iframe is pushed to display list but has NO content
    → WebRender renders: yellow background (IFrame box) with no rows
```

**The flicker pattern:**

| Frame | Window growing? | check_reinvoke result | IFrame callback runs? | layout_results has dom_id 1? | User sees |
|-------|----------------|----------------------|----------------------|------------------------------|-----------|
| 1 (startup) | N/A | InitialRender | ✅ Yes | ✅ Yes | Grey-white rows |
| 2 (resize growing) | Yes | BoundsExpanded | ✅ Yes | ✅ Yes | Grey-white rows |
| 3 (resize same/shrink) | No | None | ❌ No | ❌ No | Yellow rectangle |
| 4 (resize growing) | Yes | BoundsExpanded | ✅ Yes | ✅ Yes | Grey-white rows |
| 5 (resize shrink) | No | None | ❌ No | ❌ No | Yellow rectangle |

**This is exactly the described flicker: alternating between rows and yellow.**

---

### 2.3 Scroll (No IFrame Re-invocation)

```
scrollWheel:
  → handle_scroll_wheel()                               [events.rs:305]
    → record_scroll_from_hit_test()
      → hit-test finds scrollable node
      → ScrollInputQueue.push(ScrollInput { dom_id, node_id, delta, ... })
      → if queue was empty: start SCROLL_MOMENTUM_TIMER_ID

Timer tick (every 16ms):
  → scroll_physics_timer_callback()                    [scroll_timer.rs:130]
    → drains queue, applies physics (velocity, decay, clamping)
    → timer_info.scroll_to(dom_id, node_id, new_position)
      → pushes CallbackChange::ScrollTo { dom_id, node_id, position }
    → returns Update::DoNothing

Process callback changes:
  → CallbackChange::ScrollTo { dom_id, node_id, position }  [event.rs:1315]
    → scroll_manager.scroll_to(dom_id, internal_node_id, position, ...)
    → returns ShouldReRenderCurrentWindow
  ★ NO call to iframe_manager.check_reinvoke() ★
  ★ NO call to invoke_iframe_callback() ★

drawRect:
  → frame_needs_regeneration = false
  → display_list_needs_rebuild = false
  → build_image_only_transaction()                      [wr_translate2.rs:2550]
    → process_image_callback_updates()  — re-invokes GL texture callbacks
    → txn.skip_scene_builder()          — no display list changes
    → scroll_all_nodes()                — sends scroll offsets to WebRender
    → synchronize_gpu_values()          — opacity/transform animations
    → txn.generate_frame()
```

**Result: Scroll offset updates in WebRender, but IFrame callback NEVER re-invoked.**
The user is scrolling the existing content. For regular (non-virtual) content this works fine —
WebRender clips the display list to the visible area. But for virtual scrolling, the
IFrame callback must be re-invoked to produce new rows as the user scrolls past the
initial 100-row chunk.

---

### 2.4 Unchanged DOM (Non-Resize Repaint)

```
drawRect: (triggered by e.g. timer, hover, click)
  → frame_needs_regeneration = true (DOM callback may have modified state)
  → regenerate_layout()
    → layout_callback() → user's layout() returns same DOM structure
    → window_size_changed = false
    → is_layout_equivalent(old_styled_dom, new_styled_dom) → true
    → returns LayoutUnchanged  (skips layout entirely)
  → display_list_needs_rebuild = false
  → build_image_only_transaction()  (lightweight)
```

**Result: No flicker — layout_results preserved from previous frame.**

---

## 3. Root Cause Analysis

### Bug #1: `layout_results.clear()` + `check_reinvoke` skipping = lost child DOM

**The fundamental design conflict:**

1. `layout_and_generate_display_list()` calls `self.layout_results.clear()` at the start  
   — This makes sense: a full re-layout should produce fresh results.

2. `invoke_iframe_callback_impl()` checks `iframe_manager.check_reinvoke()` and skips  
   the callback if the IFrameManager says "no re-invocation needed".  
   — This also makes sense: don't waste time re-invoking callbacks unnecessarily.

3. **But combining (1) and (2) is broken:** After `clear()`, layout_results has no  
   dom_id 1. If `check_reinvoke()` says "no need", the child DOM never gets put back.

**The IFrameManager's `was_invoked` flag persists across layout passes** because the
IFrameManager is owned by LayoutWindow, not by layout_results. When
`layout_results.clear()` destroys the child DOM, the IFrameManager doesn't know.

### Fix Options for Bug #1

**Option A: Reset IFrameManager on full relayout**
```rust
// In layout_and_generate_display_list():
self.layout_results.clear();
self.iframe_manager.reset_all_invocation_flags();  // Already exists!
```
This forces `check_reinvoke()` to return `InitialRender` after every full relayout,
ensuring the IFrame callback always runs when `layout_results` was cleared.

**Pros:** Simple, correct, minimal code change.  
**Cons:** IFrame callback re-invoked on every resize frame (but it's fast — only 100 rows).

**Option B: Don't clear IFrame child DOMs**
```rust
// In layout_and_generate_display_list():
// Only clear the root DOM, preserve child DOMs
self.layout_results.remove(&DomId::ROOT_ID);
```

**Pros:** Preserves child DOMs if IFrame callback doesn't need re-invocation.  
**Cons:** Layout_tree/positions would be stale for child DOMs. IFrame bounds may have
changed with the new window size, so keeping old layout is incorrect anyway.

**Option C: Always re-invoke IFrame when bounds changed** (not just expanded)
Modify `check_reinvoke()` to re-invoke on ANY bounds change, not just expansion.

**Recommended: Option A** — it's the simplest fix and semantically correct.

### Bug #2: No IFrame reinvocation during scroll

The `CallbackChange::ScrollTo` handler at [event.rs:1315](dll/src/desktop/shell2/common/event.rs#L1315)
only calls `scroll_manager.scroll_to()`. It does not check whether the scrolled node is an
IFrame that needs re-invocation.

**Fix:**
```rust
CallbackChange::ScrollTo { dom_id, node_id, position } => {
    // ... existing scroll_to code ...

    // Check if this is an IFrame that needs re-invocation
    if let Some(internal_node_id) = node_id.into_crate_internal() {
        if let Some(lw) = self.get_layout_window_mut() {
            let bounds = lw.get_iframe_bounds(*dom_id, internal_node_id);
            if let Some(reason) = lw.iframe_manager.check_reinvoke(
                *dom_id, internal_node_id, &lw.scroll_manager, bounds,
            ) {
                lw.invoke_iframe_callback(*dom_id, internal_node_id, bounds, ...);
                return ProcessEventResult::ShouldUpdateDisplayListCurrentWindow;
            }
        }
    }
    ProcessEventResult::ShouldReRenderCurrentWindow
}
```

This also requires adding a `ShouldUpdateDisplayListCurrentWindow` result that triggers
a display list rebuild without full DOM regeneration.

---

## 4. The Flicker Explained Step-by-Step

Given a window at 600×500, user drags the resize handle:

| Time | Event | `layout_results` after | Visible |
|------|-------|----------------------|---------|
| t0 | Initial render | {0: root, 1: 100 rows} | ✅ Rows visible |
| t1 | Resize 601×500 (growing) | **clear** → {0: root} → IFrame callback: BoundsExpanded → {0: root, 1: 100 rows} | ✅ Rows visible |
| t2 | Resize 601×499 (shrink h) | **clear** → {0: root} → IFrame callback: **skipped** (bounds not expanded, not at edge) → {0: root} | ❌ Yellow |
| t3 | Resize 602×499 (grow w) | **clear** → {0: root} → IFrame callback: BoundsExpanded → {0: root, 1: 100 rows} | ✅ Rows visible |
| t4 | Resize 602×498 (shrink h) | **clear** → {0: root} → IFrame callback: **skipped** → {0: root} | ❌ Yellow |

The alternation between growing and shrinking produces the flicker.
During a smooth diagonal resize, macOS fires many events alternating between
width-increase and height-increase, producing rapid on/off toggling.

---

## 5. Architecture Diagram

```
┌──────────────────────────────────────────────────────────────┐
│                   render_and_present_in_draw_rect()          │
│                                                              │
│  frame_needs_regeneration?                                   │
│   ├─ YES ──→ regenerate_layout()                             │
│   │            ├─ layout_callback() → new StyledDom          │
│   │            ├─ window_size_changed?                        │
│   │            │   YES → full layout                         │
│   │            │   NO  → is_layout_equivalent?               │
│   │            │           YES → LayoutUnchanged (skip)      │
│   │            │           NO  → full layout                 │
│   │            └─ full layout:                               │
│   │                 layout_and_generate_display_list()        │
│   │                 ├─ layout_results.clear()  ← BUG #1     │
│   │                 ├─ layout_dom_recursive(root)            │
│   │                 │   ├─ solver3::layout_document()        │
│   │                 │   ├─ scan_for_iframes()                │
│   │                 │   ├─ invoke_iframe_callback_impl()     │
│   │                 │   │   ├─ check_reinvoke()              │
│   │                 │   │   │   was_invoked=true             │
│   │                 │   │   │   bounds not expanded           │
│   │                 │   │   │   → None (SKIP) ← BUG #1      │
│   │                 │   │   └─ returns existing dom_id       │
│   │                 │   └─ layout_results.insert(dom_id=0)   │
│   │                 └─ ★ dom_id=1 MISSING ★                 │
│   │                                                          │
│   └─ display_list_needs_rebuild = true                       │
│       → build_webrender_transaction()                        │
│         ├─ translate dom_id=0 → finds IFrame(1) → lookup     │
│         │   → layout_results.get(1) = None  ← EMPTY         │
│         │   → "WARNING: Child DOM 1 not found"               │
│         └─ WebRender renders IFrame box without content      │
│                                                              │
│   NO ──→ build_image_only_transaction()                      │
│            (scroll-only: skip_scene_builder, no IFrame)      │
│                                                              │
│  ────── Scroll Path ──────                                   │
│  scrollWheel → record_scroll → timer →                       │
│  CallbackChange::ScrollTo → scroll_manager.scroll_to()       │
│  → ShouldReRenderCurrentWindow                               │
│  ★ NO iframe_manager.check_reinvoke() ← BUG #2 ★           │
│  → request_redraw() → drawRect with frame_needs_regen=false  │
│  → build_image_only_transaction (scroll offsets only)         │
└──────────────────────────────────────────────────────────────┘
```

---

## 6. Relevant Code Locations

| File | Line(s) | Function | Role |
|------|---------|----------|------|
| [layout/src/window.rs](layout/src/window.rs#L562) | 562-610 | `layout_and_generate_display_list()` | Entry point; calls `clear()` at L571 |
| [layout/src/window.rs](layout/src/window.rs#L638) | 638-850 | `layout_dom_recursive()` | Lays out one DOM, scans iframes, recurses |
| [layout/src/window.rs](layout/src/window.rs#L827) | 827 | `layout_results.insert()` | Only insert point for layout results |
| [layout/src/window.rs](layout/src/window.rs#L1064) | 1064-1220 | `invoke_iframe_callback_impl()` | Core IFrame logic; check_reinvoke + callback + recursive layout |
| [layout/src/managers/iframe.rs](layout/src/managers/iframe.rs#L212) | 212-244 | `check_reinvoke()` | Decides if callback should run |
| [layout/src/managers/iframe.rs](layout/src/managers/iframe.rs#L192) | 192-200 | `reset_all_invocation_flags()` | Resets was_invoked — **exists but never called on relayout** |
| [dll/src/desktop/shell2/common/layout.rs](dll/src/desktop/shell2/common/layout.rs#L72) | 72-490 | `regenerate_layout()` | Cross-platform layout entry; calls layout_and_generate_display_list |
| [dll/src/desktop/shell2/common/event.rs](dll/src/desktop/shell2/common/event.rs#L1315) | 1315-1331 | `ScrollTo` handler | Only scroll_to(); no IFrame check |
| [dll/src/desktop/shell2/macos/mod.rs](dll/src/desktop/shell2/macos/mod.rs#L1385) | 1385-1435 | `windowDidResize:` | Sets frame_needs_regeneration=true |
| [dll/src/desktop/shell2/macos/mod.rs](dll/src/desktop/shell2/macos/mod.rs#L4460) | 4460-4660 | `render_and_present_in_draw_rect()` | Decides full vs lightweight transaction |
| [dll/src/desktop/wr_translate2.rs](dll/src/desktop/wr_translate2.rs#L2256) | 2256-2548 | `build_webrender_transaction()` | Full rebuild; iterates all layout_results |
| [dll/src/desktop/wr_translate2.rs](dll/src/desktop/wr_translate2.rs#L2550) | 2550-2582 | `build_image_only_transaction()` | Lightweight; scroll offsets + GPU values only |
| [dll/src/desktop/compositor2.rs](dll/src/desktop/compositor2.rs#L1309) | 1309-1413 | `IFrame` branch in `translate_displaylist_to_wr()` | Looks up child DOM in layout_results |

---

## 7. Minimal Fix

The smallest change to fix the resize flicker is **one line** in `layout_and_generate_display_list()`:

```rust
// layout/src/window.rs, line 571
pub fn layout_and_generate_display_list(...) {
    self.layout_results.clear();
+   self.iframe_manager.reset_all_invocation_flags();  // Force re-invoke after clear
    ...
}
```

This ensures that after `layout_results.clear()` wipes the child DOM, the IFrameManager
will return `InitialRender` on the next `check_reinvoke()` call, causing the IFrame
callback to re-run and re-populate layout_results with the child DOM.

---

## 8. scroll6_report.md Corrections

The scroll6_report §6 "IFrame Re-Invocation" claims:

> **Current Implementation: ✅ WIRED UP**
> In `process_callback_result_v2()` (event_v2.rs line 2993):
> ```rust
> if let Some(_reason) = layout_window.iframe_manager.check_reinvoke(...)
> ```

**This is INCORRECT.** There is no `event_v2.rs` file. The actual scroll handler is in
[event.rs:1315](dll/src/desktop/shell2/common/event.rs#L1315) and it does NOT call
`check_reinvoke()`. The claim was based on a planned change that was never implemented.

Additionally, the two bugs listed in scroll6_report §7 ("Remaining Work") are real:
1. `calculate_scrollbar_states()` ignores `virtual_scroll_size` — confirmed
2. `is_node_scrollable()` ignores `virtual_scroll_size` — confirmed

---

## 9. Priority Order for Fixes

1. **Bug #1 (CRITICAL):** ✅ FIXED — Added `iframe_manager.reset_all_invocation_flags()` after
   `layout_results.clear()` in `layout_and_generate_display_list()`. Forces `check_reinvoke()`
   to return `InitialRender` after every full relayout, ensuring the IFrame callback always runs.

2. **Bug #2 (HIGH):** ✅ FIXED — Added `check_and_queue_iframe_reinvoke()` method to LayoutWindow.
   `CallbackChange::ScrollTo` handler now checks IFrame reinvocation after `scroll_to()`,
   queues pending updates, and returns `ShouldUpdateDisplayListCurrentWindow`. All platform
   render paths (macOS, Windows, X11, Wayland) process pending iframe updates before building
   the WebRender transaction.

3. **Bug #3 (MEDIUM):** ✅ ALREADY FIXED — `calculate_scrollbar_states()`, `is_node_scrollable()`,
   and the static scrollbar helpers all already use `virtual_scroll_size.unwrap_or(content_rect.size)`.
   The scroll6_report §7 claim was outdated.

4. **Enhancement (LOW):** `build_webrender_transaction` should skip dom_ids that are
   IFrame children (they get submitted via the parent's `translate_displaylist_to_wr`
   recursive IFrame handling) — avoids double-submission.

---

## 10. Fix Details

### Bug #1 Fix: Reset IFrame flags on layout_results.clear()

**Files changed:**
- `layout/src/managers/iframe.rs` — Added `reset_all_invocation_flags()` public method
- `layout/src/window.rs` — Call `self.iframe_manager.reset_all_invocation_flags()` after `self.layout_results.clear()`

### Bug #2 Fix: IFrame reinvocation during scroll

**Files changed:**
- `layout/src/window.rs` — Added `check_and_queue_iframe_reinvoke(dom_id, node_id) -> bool`
- `dll/src/desktop/shell2/common/event.rs` — `CallbackChange::ScrollTo` handler now calls
  `check_and_queue_iframe_reinvoke()` and returns `ShouldUpdateDisplayListCurrentWindow`
- `dll/src/desktop/shell2/macos/mod.rs` — `ShouldUpdateDisplayListCurrentWindow` → redraw only
  (no `frame_needs_regeneration`); render path processes pending iframe updates
- `dll/src/desktop/shell2/windows/mod.rs` — Same pattern
- `dll/src/desktop/shell2/linux/x11/mod.rs` — Same pattern
- `dll/src/desktop/shell2/linux/wayland/mod.rs` — Same pattern (4 event handler locations)

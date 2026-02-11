# Hello-World Layout Investigation Report

## Executive Summary

The hello-world.c example has **5 interconnected bugs** that cause: clipped titlebar title,
8px vertical offset, broken hit-testing (no clicks work), and wrong visual layout.
All issues trace back to two root causes in the layout engine + one in the debug API +
one in the DOM structure + one in the example code itself.

---

## Bug 1: `is_block_level()` Missing `Flex` and `Grid` Display Types

**File:** `layout/src/solver3/layout_tree.rs` line 1305  
**Severity:** Critical – cascading layout corruption

### The Problem

```rust
pub fn is_block_level(styled_dom: &StyledDom, node_id: NodeId) -> bool {
    matches!(
        get_display_type(styled_dom, node_id),
        LayoutDisplay::Block
            | LayoutDisplay::FlowRoot
            | LayoutDisplay::Table
            | LayoutDisplay::TableRow
            | LayoutDisplay::TableRowGroup
            | LayoutDisplay::ListItem
    )
}
```

`Flex` and `Grid` are **not** in this list. Per CSS spec (CSS Display Module Level 3 §2.1),
`display: flex` and `display: grid` are **block-level** elements — they generate block-level
boxes that establish new formatting contexts (FFC/GFC).

### The Cascade

When the outer body node (`display: block`) calls `process_block_children()`, it checks
each child with `is_block_level()`:

1. **Child 1: titlebar div** (`display: flex`) → `is_block_level()` returns **false**
2. **Child 2: user body** (`display: block`) → `is_block_level()` returns **true**

Since there's at least one block-level child (the user body), `process_block_children()`
enters the "mixed block+inline" branch. The titlebar div (incorrectly classified as
non-block) gets wrapped in an **`AnonymousBoxType::InlineWrapper`** anonymous box.

### Evidence from Debug Data

Layout tree (from `get_layout_tree` debug API):

```
Node 0: Body (display:block) - root
  Node 1: InlineWrapper (ANONYMOUS) ← should not exist!
    Node 2: Div.__azul-native-titlebar (display:flex)
      Node 3: Text "Hello World"
  Node 4: Body (display:block) - user DOM
    Node 5: Text "5"
    Node 6: Button
      Node 7: Text "Increase counter"
```

The InlineWrapper at node 1 is the smoking gun. The titlebar's `display:flex` div should
be treated as a block-level sibling of the user body, requiring no wrapping.

### Correct Layout Tree (after fix)

```
Node 0: Body (display:block) - root
  Node 1: Div.__azul-native-titlebar (display:flex) ← direct child, no wrapper
    Node 2: Text "Hello World"
  Node 3: Body (display:block) - user DOM
    Node 4: Text "5"
    Node 5: Button
      Node 6: Text "Increase counter"
```

### CSS Display Module Level 3 – Full Classification of All `LayoutDisplay` Variants

The `LayoutDisplay` enum (in `css/src/props/layout/display.rs`) has 21 variants.
Each variant maps to an **outer display type** per CSS Display Module Level 3 §2:

| Variant | Outer display type | `is_block_level()` should match? | Currently matched? |
|---------|-------------------|----------------------------------|-------------------|
| `None` | — (no box) | No | No ✓ |
| **`Block`** | **block** | **Yes** | **Yes ✓** |
| `Inline` | inline | No | No ✓ |
| `InlineBlock` | inline | No | No ✓ |
| **`Flex`** | **block** | **Yes** | **No ✗ — BUG** |
| `InlineFlex` | inline | No | No ✓ |
| **`Grid`** | **block** | **Yes** | **No ✗ — BUG** |
| `InlineGrid` | inline | No | No ✓ |
| **`Table`** | **block** | **Yes** | **Yes ✓** |
| `InlineTable` | inline | No | No ✓ |
| **`FlowRoot`** | **block** | **Yes** | **Yes ✓** |
| **`ListItem`** | **block** | **Yes** | **Yes ✓** |
| **`TableCaption`** | **block** | **Yes** | **No ✗ — minor** |
| `TableRow` | table-internal | Debatable¹ | Yes (harmless) |
| `TableRowGroup` | table-internal | Debatable¹ | Yes (harmless) |
| `TableHeaderGroup` | table-internal | No² | No ✓ |
| `TableFooterGroup` | table-internal | No² | No ✓ |
| `TableColumnGroup` | table-internal | No² | No ✓ |
| `TableColumn` | table-internal | No² | No ✓ |
| `TableCell` | table-internal | No² | No ✓ |
| `RunIn` | context-dependent | No | No ✓ |
| `Marker` | — (list marker) | No | No ✓ |

¹ `TableRow` and `TableRowGroup` are technically **table-internal** display types per CSS spec,
not block-level. They're in the current `is_block_level()` match — this is harmless because
`process_block_children()` is only called for block containers, and table rows inside a block
container would trigger CSS table fixup (anonymous table wrapping) regardless. Leaving them
is acceptable for now.

² Other table-internal types (`TableHeaderGroup`, `TableFooterGroup`, `TableColumnGroup`,
`TableColumn`, `TableCell`) are correctly excluded. A bare `table-cell` as a child of a
block container should ideally trigger anonymous table+row wrapping, but that's a separate
CSS table fixup feature, not related to the current bugs.

### How `Table` Behaves vs How It Should Behave

**Current behavior (correct for `Table`):**
- `is_block_level()` returns `true` for `Table` ✓
- `process_node()` dispatches `Table` to `process_table_children()` ✓
- A `display: table` element as a child of a block container is treated as block-level ✓

**How it should behave (per CSS spec):**
- `display: table` generates a **block-level** box (outer display = block)
- Its inner display establishes a **table formatting context**
- As a child of a block container, it participates in the BFC like any block-level box
- Current implementation is **correct** for `Table`

**The problem is that `Flex` and `Grid` are NOT treated the same as `Table`:**
- `display: flex` is analogous: block-level box (outer) + flex formatting context (inner)
- `display: grid` is analogous: block-level box (outer) + grid formatting context (inner)
- Both should be in `is_block_level()` just like `Table` is

### The Fix

Add `Flex`, `Grid`, and `TableCaption` to `is_block_level()`:

```rust
pub fn is_block_level(styled_dom: &StyledDom, node_id: NodeId) -> bool {
    matches!(
        get_display_type(styled_dom, node_id),
        LayoutDisplay::Block
            | LayoutDisplay::FlowRoot
            | LayoutDisplay::Flex           // NEW — block-level, establishes FFC
            | LayoutDisplay::Grid           // NEW — block-level, establishes GFC
            | LayoutDisplay::Table
            | LayoutDisplay::TableCaption   // NEW — block-level per CSS spec
            | LayoutDisplay::TableRow
            | LayoutDisplay::TableRowGroup
            | LayoutDisplay::ListItem
    )
}
```

**Note:** The existing `creates_block_context()` method on `LayoutDisplay` (at
`css/src/props/layout/display.rs` line 56) already correctly includes `Flex` and `Grid`.
This confirms the intent — the omission from `is_block_level()` was simply an oversight.

### Titlebar Cosmetic Change (Last)

After all bugs are fixed, the titlebar should be changed from `display: flex` to
`display: block` with `text-align: center` and `margin: 0 auto` for centering the
title text. This is a cosmetic improvement, not a bug fix. The flex-based layout works
correctly once Bug 1 is fixed, but a block-based titlebar is semantically simpler and
avoids potential edge cases with flex item sizing.

---

## Bug 2: `inject_software_titlebar()` Creates Body Root → Double Body Nesting

**File:** `dll/src/desktop/shell2/common/layout_v2.rs` line 554  
**Severity:** Critical – structural DOM corruption

### The Problem

```rust
fn inject_software_titlebar(user_dom, window_title, system_style) -> StyledDom {
    let mut container = StyledDom::default();  // ← creates BODY root!
    container.append_child(titlebar_styled);
    container.append_child(user_dom);          // user_dom is also BODY
    container
}
```

`StyledDom::default()` (defined in `core/src/styled_dom.rs` line 780) creates its root
node via `NodeData::create_body()`. The user's DOM also starts with a Body node (from
`AzDom_createBody()` in hello-world.c). This produces:

```html
<body style="margin:8px">                      <!-- container (StyledDom::default) -->
  <div class="__azul-native-titlebar" ...>     <!-- titlebar -->
    <text>Hello World</text>
  </div>
  <body style="margin:8px">                   <!-- user DOM -->
    <text style="font-size:32px">5</text>
    <button>Increase counter</button>
  </body>
</body>
```

### Consequences

1. **Double 8px margin**: The UA CSS (`core/src/ua_css.rs` lines 508-511) applies
   `margin: 8px` to every `<body>` element. Two nested bodies = 16px total offset.
   Debug data confirms: outer body at (8,8), inner body starts at y=52 
   (8 + 8 from UA margin + 28 titlebar + 8 inner body margin).

2. **Invalid HTML structure**: `<body>` inside `<body>` is invalid. The container
   should be `<html>` (which has `display:block` but no margin in UA CSS).

### The Fix

`inject_software_titlebar` should:
- Create an `Html` root node (not Body)  
- Keep the user's body as-is  
- Remove margin from the user's body and convert to padding when inside a titlebar-injected layout

Alternatively, `StyledDom::default()` could create an `Html` root instead of `Body`,
but that has wider implications. The most surgical fix is in `inject_software_titlebar`.

---

## Bug 3: Titlebar Title Text Gets Clipped Due to InlineWrapper Sizing

**File:** `layout/src/widgets/titlebar.rs` line 268 + Bug 1 interaction  
**Severity:** Visual – title text may be partially or fully invisible

### The Problem

The titlebar title text node has `overflow-x: Hidden` (line 268 of titlebar.rs):

```rust
fn build_title_style(&self) -> CssPropertyWithConditionsVec {
    // ...
    props.push(CssPropertyWithConditions::simple(
        CssProperty::const_overflow_x(LayoutOverflow::Hidden),
    ));
    // ...
}
```

This causes `push_node_clips()` in `display_list.rs` (line 1870) to push a `PushClip`
with the text node's content-box as the clip rect.

Because of Bug 1, the titlebar div is wrapped in an InlineWrapper. The InlineWrapper's
content sizing doesn't account for the flex container correctly (since it thinks it's 
laying out inline content). The resulting text node position is:

- **Clip rect**: `(86, 22.25, 71.93, 15.5)` — tiny rect
- **Stacking context**: the text "Hello World" is painted inside this clip

The `overflow-x: hidden` on the title text node is intentional (for text-overflow: ellipsis
behavior when the title is too long), but the InlineWrapper wrapping distorts the sizing.
Once Bug 1 is fixed, the flex layout will correctly size the titlebar and its text child.

### Evidence from Display List

```
Index 3: PushStackingContext { z_index: 0, bounds: (8, 8, 384, 95.5) }
Index 4: PushClip { bounds: (86, 22.25, 71.93, 15.5) }    ← clips title text
Index 5: Text "Hello World" at (86, 20.25)
...
Index 9: PopClip
```

The PushClip at index 4 constrains the title text to a 71.93×15.5 box. This is because
the text node is positioned relative to the InlineWrapper's distorted coordinate space.

### The Fix

This is a **secondary symptom** of Bug 1. Once `is_block_level()` correctly recognizes
`Flex`, the titlebar div won't be InlineWrapper-wrapped, and the flex layout algorithm
will properly size the title text within the titlebar's full width. The `overflow-x: hidden`
on the title text is correct and should remain (it's needed for ellipsis on long titles).

---

## Bug 4: Debug API `HitTest` Is a Stub (Always Returns null)

**File:** `dll/src/desktop/shell2/common/debug_server.rs` line 2652  
**Severity:** Debug tooling – not a layout bug, but prevents debugging

### The Problem

```rust
DebugEvent::HitTest { x, y } => {
    let hit_test = callback_info.get_hit_test_frame(0);
    let response = HitTestResponse {
        x: *x,
        y: *y,
        node_id: None, // TODO: extract from hit_test
        node_tag: None,
    };
    send_ok(request, None, Some(ResponseData::HitTest(response)));
}
```

The `node_id` and `node_tag` fields are hardcoded to `None`. The `hit_test` variable
from `get_hit_test_frame(0)` is computed but **never used**. This is why every hit test
via the debug API returns `{"node_id": null, "node_tag": null}`.

### Important Note

The **actual** hit-testing path (via WebRender) works differently. When the debug API
sends a `Click` event, it uses `queue_window_state_sequence()` to inject mouse states.
These are processed by the macOS event loop, which calls `update_hit_test_at()` →
`fullhittest_new_webrender()` → WebRender's native hit tester. This path **does** work
for click events.

However, the `HitTest` debug command only queries the *last cached* hit test result,
not performing a new one at the given coordinates. This is why debug hit tests show null
even though clicks might work.

### The Fix

The `HitTest` handler should:
1. Perform an actual hit test at the given (x, y) coordinates
2. Extract node_id and tag from the result
3. Return them in the response

---

## Bug 5: hello-world.c Uses `create_text()` Instead of `create_p()` for Counter

**File:** `examples/c/hello-world.c` line 62  
**Severity:** Minor – cosmetic/semantic

### The Problem

```c
AzDom label = AzDom_createText(label_text);  // Creates INLINE text node
```

`AzDom_createText()` creates a bare text node (`NodeType::Text`). Text nodes are always
inline-level (see `is_inline_level()` in layout_tree.rs line 1326). When placed directly
inside a body (block container), a bare text node becomes part of an inline formatting
context alongside the button (which is block-level), triggering the anonymous InlineWrapper
wrapping mechanism again.

### The Fix

Use `AzDom_createP()` or wrap in a div:

```c
AzDom label = AzDom_createText(label_text);
AzDom p = AzDom_createDiv();  // or AzDom_createP() if available
AzDom_addChild(&p, label);
```

This makes the counter a proper block-level element and avoids triggering anonymous box
creation inside the user's body.

---

## Bug 6 (Potential): Button Click May Not Fire Callback

**Severity:** Needs further investigation

### Observation

Debug API test sequence:
1. `get_app_state` → counter = 5
2. `click_button` → returns `{"success": true, "message": "Clicked at (93.8, 73.7)"}`
3. `get_app_state` → counter = **still 5**

The click was successfully resolved to coordinates, and `queue_window_state_sequence()`
was called with move/down/up states. But the counter didn't increment.

### Possible Causes

1. **Hit test at (93.8, 73.7) misses the button**: The button is at `(32, 52, 123.5, 43.5)`
   in logical coordinates. The point (93.8, 73.7) should be inside. But the WebRender
   hit tester works in **physical** (device) coordinates. On a Retina display (2x), the
   logical point (93.8, 73.7) maps to physical (187.6, 147.4). If the display list was
   built with the InlineWrapper distortions, the WebRender clip hierarchy might prevent
   the hit from reaching the button.

2. **The window_state_sequence isn't processed before the next debug query**: The
   `queue_window_state_sequence` is asynchronous — it queues states for the next event
   loop iterations. The `get_app_state` query might execute before the click sequence
   completes.

3. **The callback invocation path fails silently**: The `on_click` callback requires
   `MyDataModel_downcastMut()` to succeed. If the RefAny downcast fails (e.g., due to
   type mismatch), the callback returns `AzUpdate_DoNothing` without incrementing.

### Most Likely Cause

Cause 2 is most likely. The debug API collection script runs `click_button` followed
immediately by `get_app_state` without waiting for the event loop to process the queued
states. Adding a `wait_frame` or short delay between click and state query should show
the counter increment.

---

## Root Cause Dependency Graph

```
Bug 1: is_block_level() missing Flex/Grid
  └──→ titlebar div wrapped in InlineWrapper
        ├──→ Bug 3: InlineWrapper distorts sizing → PushClip clips title text
        ├──→ Flex layout not applied to titlebar (treated as inline)
        └──→ Visual: titlebar appears at wrong position/size

Bug 2: inject_software_titlebar creates Body root
  ├──→ Double body nesting (body > body)
  ├──→ Double 8px margin (total 16px offset)  
  └──→ No <html> root node in DOM

Bug 4: Debug HitTest is stub
  └──→ Debug API cannot verify hit-test results (independent of layout bugs)

Bug 5: hello-world.c uses inline text for counter
  └──→ Counter text + button trigger InlineWrapper in user body (cosmetic)

Bug 6: Click may not work
  └──→ Likely timing issue in debug API (async state queue)
```

## Recommended Fix Order

1. **Bug 1** (is_block_level) — highest impact, 2-line fix
2. **Bug 2** (inject_software_titlebar) — structural fix, moderate complexity
3. **Bug 5** (hello-world.c) — trivial, 1-2 line fix
4. **Bug 4** (debug HitTest stub) — debug tooling improvement
5. **Bug 6** (click timing) — verify after Bug 1+2 are fixed

## Key Files to Modify

| File | Change |
|------|--------|
| `layout/src/solver3/layout_tree.rs:1305` | Add `Flex` \| `Grid` to `is_block_level()` |
| `dll/src/desktop/shell2/common/layout_v2.rs:554` | Create Html root in `inject_software_titlebar` |
| `examples/c/hello-world.c:62` | Wrap counter text in a block element |
| `dll/src/desktop/shell2/common/debug_server.rs:2652` | Implement actual hit test in `HitTest` handler |

## Verification Plan

After applying fixes 1-3:
1. Rebuild: `cargo build --release -p azul-dll --features build-dll`
2. Recompile hello-world.c
3. Run with `AZUL_DEBUG=8765`
4. Run `scripts/collect_hello_world_debug.sh 8765`
5. Verify:
   - Layout tree has NO InlineWrapper for titlebar
   - HTML string shows `<html><div.titlebar>...<body>...` (single body)
   - Display list has no unexpected PushClip for title text
   - All node positions are correct (no 8px offset)
   - Button click increments counter (with wait_frame between click and read)

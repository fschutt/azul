# CSS List Implementation Report

**Date**: November 21, 2025  
**Status**: ⚠️ Partial Implementation with Critical Bugs  
**Priority**: HIGH - Affects core functionality

## Executive Summary

The current implementation of CSS lists has three critical bugs:

1. **❌ Counter Reset Missing**: `<ol>` and `<ul>` don't reset the `list-item` counter, causing numbered lists to continue from previous lists (e.g., "6. 7. 8. 9." instead of "1. 2. 3. 4.")
2. **❌ Padding Architecture Wrong**: Using `padding-left` + `text-indent` on `<li>` instead of respecting `padding-inline-start` on `<ol>`/`<ul>` per CSS spec
3. **❌ Font Style Bug**: H1/H2 rendering in italic instead of regular/bold weight

This report analyzes how browsers implement lists and provides architectural recommendations.

---

## Problem 1: Counter Reset Missing

### Current Behavior
```
<ul>
  <li>Item 1</li>  <!-- counter-value=1 ✓ -->
  <li>Item 2</li>  <!-- counter-value=2 ✓ -->
</ul>

<ol>
  <li>First</li>   <!-- counter-value=6 ❌ Should be 1 -->
  <li>Second</li>  <!-- counter-value=7 ❌ Should be 2 -->
</ol>
```

### Root Cause

**File**: `azul/layout/src/solver3/cache.rs` (lines 970-1090)

The `compute_counters_recursive()` function correctly implements:
- ✅ `counter-reset` property parsing
- ✅ `counter-increment` property parsing  
- ✅ Auto-increment for `display: list-item`

But it's **missing UA (User Agent) stylesheet rules** for `<ol>` and `<ul>`.

### CSS Specification Requirements

**CSS Lists Module Level 3** §4.1 states:

```css
/* User Agent Stylesheet */
ol, ul {
  counter-reset: list-item;  /* Reset to 0 */
}

li {
  display: list-item;
  counter-increment: list-item;  /* Auto-increment by 1 */
}
```

**Key Points**:
- `counter-reset: list-item` on `<ol>`/`<ul>` creates a **new counter scope**
- Each `<li>` within that scope increments from 0
- Nested lists create nested counter scopes

### Browser Implementation Analysis

#### Chromium (Blink)
**File**: `third_party/blink/renderer/core/css/html.css`

```css
ol, ul {
  counter-reset: list-item;
  padding-inline-start: 40px;
}

li {
  display: list-item;
  text-align: match-parent;
}
```

**Implementation**:
- `CounterNode` tree structure mirrors DOM tree
- `LayoutListMarker` class handles marker positioning
- Counters are **scoped** per list container
- `::marker` pseudo-element is **out-of-flow** (not in inline formatting context)

**File**: `third_party/blink/renderer/core/layout/list/layout_list_marker.cc`

```cpp
void LayoutListMarker::UpdateMarkerText() {
  // Get counter value from parent list-item
  const CounterNode* counter = GetCounterNode("list-item");
  int value = counter ? counter->Value() : 0;
  
  // Format based on list-style-type
  marker_text_ = GenerateMarkerText(value, list_style_type_);
}
```

#### Firefox (Gecko)
**File**: `layout/style/res/html.css`

```css
ol, ul {
  counter-reset: list-item 0;
  padding-inline-start: 40px;
}

li {
  display: list-item;
}
```

**Implementation**:
- `nsBulletFrame` manages marker rendering
- Counters stored in `nsCounterManager` with hierarchical scope
- Markers positioned **outside** content area using `::marker` box

**File**: `layout/generic/nsBulletFrame.cpp`

```cpp
int32_t nsBulletFrame::GetListItemOrdinal() {
  nsCounterManager* counterManager = PresShell()->CounterManager();
  nsCounterNode* counterNode = 
    counterManager->GetCounterFor("list-item", this);
  
  return counterNode ? counterNode->GetValue() : 1;
}
```

#### WebKit (Safari)
**File**: `Source/WebCore/css/html.css`

```css
ol, ul {
  counter-reset: list-item;
  padding-inline-start: 40px;
}

li {
  display: list-item;
}
```

**Implementation**:
- `RenderListMarker` class
- Counter scoping via `RenderCounter` tree
- Marker is **pseudo-element** with special positioning

### Solution Design

**Option 1: UA Stylesheet (Recommended)**

Add to User Agent stylesheet in `azul/layout`:

```rust
// In UA CSS initialization
pub fn get_ua_list_styles() -> &'static str {
    r#"
    ol, ul {
        counter-reset: list-item 0;
        padding-inline-start: 40px;
    }
    
    ol {
        list-style-type: decimal;
    }
    
    ul {
        list-style-type: disc;
    }
    
    li {
        display: list-item;
    }
    "#
}
```

**Option 2: Hardcode in Counter Computation**

In `compute_counters_recursive()`, add:

```rust
// Check if this is an ol/ul element that should reset counter
let node_type = &styled_dom.node_data.as_container()[dom_id].node_type;
if matches!(node_type, NodeType::Label(label) if label.as_str() == "ol" || label.as_str() == "ul") {
    // Auto-reset list-item counter for list containers
    let counter_name = "list-item".to_string();
    counter_stacks
        .entry(counter_name.clone())
        .or_default()
        .push(0);  // Reset to 0
    reset_counters_at_this_level.push(counter_name);
    eprintln!("[compute_counters] Auto-reset list-item counter for <{}> at node_idx={}", 
              label.as_str(), node_idx);
}
```

**Recommendation**: Use **Option 2 (hardcode)** for immediate fix, then implement Option 1 (UA stylesheet) properly.

---

## Problem 2: List Indentation Architecture

### Current Implementation (Workaround)

```html
<li style="padding-left: 25px; text-indent: -25px;">Item</li>
```

**Why it's wrong**:
- ❌ Hardcoded values in HTML
- ❌ `text-indent` is a **text property**, not list property
- ❌ Doesn't respect `padding-inline-start` on parent `<ul>`/`<ol>`
- ❌ Breaks RTL (right-to-left) languages
- ❌ Doesn't work with `list-style-position: inside`

### CSS Specification: How Lists Should Work

**CSS Lists Module Level 3** §4.2 - List Marker Positioning

```
┌─────────────────────────────────────────┐
│ <ul> (padding-inline-start: 40px)       │
│  ┌─────────────────────────────────┐    │
│  │ <li>                             │    │
│  │  ::marker  Text content here... │    │
│  │  (outside) wraps to this line   │    │
│  │            with proper indent    │    │
│  └─────────────────────────────────┘    │
└─────────────────────────────────────────┘

Outside positioning (list-style-position: outside):
├─ ::marker is positioned in the MARGIN area of <li>
├─ ::marker does NOT consume inline space
└─ Text wraps at the content-box edge, NOT at marker edge
```

**Key Architectural Points**:

1. **`padding-inline-start` on `<ul>`/`<ol>`**: Creates space for markers
2. **`::marker` is out-of-flow**: Not part of inline formatting context
3. **Marker positioning**: In the margin area, to the left of content
4. **Text wrapping**: Happens at content-box edge, all lines aligned

### Browser Implementation: Marker Positioning

#### Chromium Architecture

**File**: `third_party/blink/renderer/core/layout/list/layout_list_marker.cc`

```cpp
void LayoutListMarker::PositionMarker() {
  if (list_style_position_ == EListStylePosition::kOutside) {
    // Marker is positioned in the margin area of the list-item
    // It does NOT participate in the inline formatting context
    LayoutUnit marker_inline_offset = -MarkerWidth() - kMarkerPadding;
    SetLogicalLeft(marker_inline_offset);
  } else {  // kInside
    // Marker is an inline box at the start of content
    SetLogicalLeft(LayoutUnit());
  }
}

LayoutRect LayoutListMarker::ComputeMarkerRect() {
  // Marker bounding box is OUTSIDE the content area
  LayoutUnit inline_offset = IsOutside() ? 
    -MarkerLogicalWidth() - kMarkerPadding : LayoutUnit();
  
  return LayoutRect(inline_offset, block_offset, 
                    MarkerLogicalWidth(), MarkerLogicalHeight());
}
```

**Key insight**: Markers are **absolutely positioned** relative to the list-item's content edge, NOT part of text flow.

#### Firefox Architecture

**File**: `layout/generic/nsBulletFrame.cpp`

```cpp
void nsBulletFrame::Reflow(nsPresContext* aPresContext,
                           ReflowOutput& aMetrics,
                           const ReflowInput& aReflowInput) {
  if (StyleList()->mListStylePosition == StyleListStylePosition::Outside) {
    // Position bullet in margin area
    // The bullet frame has NEGATIVE margin-inline-start
    nsMargin margin = aReflowInput.ComputedPhysicalMargin();
    margin.left = -BulletWidth() - kBulletMargin;
    
    // Bullet does NOT affect line layout
    mRect.SetRect(-BulletWidth(), blockOffset, BulletWidth(), BulletHeight());
  }
}
```

**Key insight**: Markers use **negative margin** to position outside content flow.

#### WebKit Architecture

**File**: `Source/WebCore/rendering/RenderListMarker.cpp`

```cpp
LayoutUnit RenderListMarker::lineHeight() const {
    // Marker height doesn't affect line height of content
    return 0;
}

void RenderListMarker::layout() {
    if (isOutside()) {
        // Marker is positioned absolutely in marker box area
        // Located in the padding area of the list item
        LayoutUnit xPos = -width() - listMarkerPadding;
        setLocation(LayoutPoint(xPos, yPos));
    }
}
```

### Correct Architecture for Azul

The current implementation is fundamentally wrong. Here's what needs to happen:

**Current Flow (Wrong)**:
```
collect_and_measure_inline_content()
  └─> Add marker as InlineContent::Text
      └─> Text layout engine positions marker as first character
          └─> Use text-indent hack to shift first line left
              └─> ❌ Breaks multi-line wrapping
```

**Correct Flow (Spec-Compliant)**:
```
layout_ifc()
  └─> Check if list-item has ::marker pseudo-element
      └─> Measure marker dimensions (width, height, baseline)
          └─> Position marker OUTSIDE content area:
              - X = -marker_width - marker_padding
              - Y = first_line_baseline - marker_baseline
          └─> Collect inline content WITHOUT marker
              └─> Layout text normally in content box
                  └─> All lines aligned at content edge ✓
```

**Implementation Changes Required**:

#### File: `azul/layout/src/solver3/fc.rs`

**Current** (lines 2970-3020):
```rust
// Generate marker text segments with proper Unicode font fallback
let marker_segments = generate_list_marker_segments(...);
for segment in marker_segments {
    content.push(InlineContent::Text(segment));  // ❌ WRONG
}
```

**Should be**:
```rust
// Store marker info for later positioning (don't add to inline content)
let marker_info = MarkerInfo {
    width: measure_marker_width(&marker_segments),
    height: measure_marker_height(&marker_segments),
    baseline: measure_marker_baseline(&marker_segments),
    segments: marker_segments,
    position: ListStylePosition::Outside,  // from CSS
};

// DON'T add marker to inline content!
// It will be positioned separately after line layout
```

#### Add Marker Positioning Pass

**New** (after line layout in `layout_ifc`):
```rust
// After text3 layout completes
let text_layout_result = text_cache.layout_flow(...)?;

// Position markers for list items
if let Some(marker_info) = stored_marker_info {
    let first_line = text_layout_result.fragment_layouts["main"]
        .lines.first();
    
    if let Some(line) = first_line {
        let marker_x = -marker_info.width - MARKER_PADDING;
        let marker_y = line.baseline_y - marker_info.baseline;
        
        // Add marker as separate display list item
        // (not part of text layout)
        node.marker_position = Some(LogicalPoint::new(marker_x, marker_y));
        node.marker_content = Some(marker_info);
    }
}
```

### Padding Inheritance Chain

**Correct CSS cascade**:
```css
/* UA Stylesheet */
ol, ul {
    padding-inline-start: 40px;  /* Creates space for markers */
}

/* User can override */
ul {
    padding-inline-start: 20px;  /* Smaller indent */
}

/* Individual list items inherit, but DON'T need padding */
li {
    /* NO padding needed - markers are outside content box */
}
```

**Current workaround** (should be removed):
```html
<li style="padding-left: 25px; text-indent: -25px;">
```

**Where `padding-inline-start` is used**:
1. **On `<ul>`/`<ol>`**: Creates space in the **margin** area for markers
2. **Inherited by `<li>`**: But the padding is on the LIST CONTAINER, not individual items
3. **Marker positioning**: Uses parent's padding value to calculate position

---

## Problem 3: Font Style Issues (H1/H2 Italic)

### Reported Issue
H1 and H2 headings render in **italic** font style instead of **bold** or **regular**.

### Investigation Required

This is likely caused by:

1. **Font Selection Bug**: System font fallback choosing italic variant
2. **Font-Weight Mapping**: CSS `font-weight: bold` not mapping to correct font file
3. **Font Subsetting**: Bold glyphs not included in subset

**Check**:
```rust
// In azul/layout/src/text3/font.rs or similar
let font_style = styled_dom.get_font_style(node_id);
let font_weight = styled_dom.get_font_weight(node_id);

eprintln!("H1 font_style={:?}, font_weight={:?}", font_style, font_weight);
```

**Expected UA Stylesheet**:
```css
h1, h2, h3, h4, h5, h6 {
    font-weight: bold;
    font-style: normal;
}

h1 { font-size: 2em; }
h2 { font-size: 1.5em; }
```

**Common Bug**: Font fallback chain:
```
Request: Helvetica, bold, normal
  ├─> Font file lookup finds: Helvetica-Oblique.ttf
  └─> ❌ Wrong variant selected
  
Should be: Helvetica-Bold.ttf
```

---

## Implementation Priority

### Phase 1: Critical Fixes (Immediate)

1. **Fix Counter Reset** (1 hour)
   - Add auto-reset for `<ol>`/`<ul>` in `compute_counters_recursive()`
   - Test: Verify ordered list shows "1. 2. 3. 4."

2. **Fix Font Selection** (2 hours)
   - Debug font style/weight resolution for headings
   - Test: Verify H1/H2 render in bold, not italic

### Phase 2: Architecture Refactor (1 week)

3. **Proper Marker Positioning** (3 days)
   - Separate marker from inline content
   - Implement out-of-flow positioning
   - Add `MarkerInfo` structure
   - Update display list generation

4. **UA Stylesheet for Lists** (1 day)
   - Add proper `padding-inline-start` on `<ol>`/`<ul>`
   - Remove `padding-left`/`text-indent` workaround from HTML

5. **list-style-position Support** (1 day)
   - Implement `inside` vs `outside` positioning
   - Update marker positioning logic

### Phase 3: Advanced Features (2 weeks)

6. **::marker Styling** (1 week)
   - Allow CSS styling of `::marker` pseudo-element
   - Support `marker-content` property
   - Custom marker colors, fonts, sizes

7. **Nested List Support** (3 days)
   - Proper counter scoping for nested lists
   - Hierarchical counter display (e.g., "1.1", "1.2")

8. **RTL Support** (2 days)
   - Mirror marker positioning for RTL languages
   - Test with Arabic, Hebrew content

---

## Testing Plan

### Unit Tests

```rust
#[test]
fn test_counter_reset_on_ol() {
    let html = r#"
        <ul>
            <li>Item 1</li>
            <li>Item 2</li>
        </ul>
        <ol>
            <li>First</li>
            <li>Second</li>
        </ol>
    "#;
    
    let layout = compute_layout(html);
    
    // Check ul counters
    assert_eq!(get_counter_value(&layout, "ul > li:nth-child(1)"), 1);
    assert_eq!(get_counter_value(&layout, "ul > li:nth-child(2)"), 2);
    
    // Check ol counters (should reset)
    assert_eq!(get_counter_value(&layout, "ol > li:nth-child(1)"), 1);
    assert_eq!(get_counter_value(&layout, "ol > li:nth-child(2)"), 2);
}

#[test]
fn test_marker_positioning_outside() {
    let html = r#"<ul><li>Item with long text that wraps to multiple lines</li></ul>"#;
    let layout = compute_layout(html);
    
    let marker_pos = get_marker_position(&layout, "li");
    assert!(marker_pos.x < 0, "Marker should be positioned outside (negative X)");
    
    // Check that all text lines are aligned
    let text_lines = get_text_line_positions(&layout, "li");
    assert_eq!(text_lines[0].x, text_lines[1].x, "Text lines should align");
}

#[test]
fn test_heading_font_weight() {
    let html = r#"<h1>Heading</h1>"#;
    let layout = compute_layout(html);
    
    let font_weight = get_font_weight(&layout, "h1");
    assert_eq!(font_weight, 700, "H1 should be bold (weight 700)");
    
    let font_style = get_font_style(&layout, "h1");
    assert_eq!(font_style, FontStyle::Normal, "H1 should NOT be italic");
}
```

### Visual Regression Tests

Compare PDF output against reference images:

1. **test_list_counter_reset.pdf**
   - Expected: "1. 2. 3." for first list, "1. 2. 3." for second list
   - Current: "1. 2. 3." for first list, "6. 7. 8." for second list ❌

2. **test_list_indentation.pdf**
   - Expected: Multi-line items with all lines aligned
   - Current: First line outdented, subsequent lines indented ✓ (with workaround)

3. **test_heading_fonts.pdf**
   - Expected: Bold, upright text for H1/H2
   - Current: Italic text ❌

---

## Browser Behavior Reference

### Test Case: Counter Scoping

```html
<ol>
  <li>One</li>      <!-- 1. -->
  <li>Two</li>      <!-- 2. -->
</ol>

<ol>
  <li>First</li>    <!-- 1. (reset) -->
  <li>Second</li>   <!-- 2. -->
</ol>

<ol start="5">
  <li>Five</li>     <!-- 5. (start attribute) -->
  <li>Six</li>      <!-- 6. -->
</ol>
```

**All browsers** (Chrome, Firefox, Safari, Edge): ✅ Correct behavior

### Test Case: Nested Lists

```html
<ol>
  <li>Item 1</li>        <!-- 1. -->
  <li>Item 2             <!-- 2. -->
    <ol>
      <li>Sub A</li>     <!-- 1. (nested reset) -->
      <li>Sub B</li>     <!-- 2. -->
    </ol>
  </li>
  <li>Item 3</li>        <!-- 3. (back to parent scope) -->
</ol>
```

**All browsers**: ✅ Nested lists create independent counter scopes

### Test Case: Multi-line Wrapping

```html
<style>
  ul { padding-inline-start: 40px; }
  li { /* NO padding, NO text-indent */ }
</style>

<ul>
  <li>This is a very long item that will definitely wrap to multiple
  lines and all those lines should be properly aligned at the left
  edge of the content area, not indented or outdented</li>
</ul>
```

**Expected layout**:
```
• This is a very long item that will
  definitely wrap to multiple lines
  and all those lines should be
```

**All browsers**: ✅ Perfect alignment of wrapped lines

---

## References

### CSS Specifications

1. **CSS Lists Module Level 3**
   - https://www.w3.org/TR/css-lists-3/
   - §3: List Counters
   - §4: List Markers and `::marker`

2. **CSS Counter Styles Level 3**
   - https://www.w3.org/TR/css-counter-styles-3/
   - Counter algorithms and formatting

3. **CSS Display Module Level 3**
   - https://www.w3.org/TR/css-display-3/
   - §3.4: `display: list-item`

### Browser Source Code

1. **Chromium**
   - `third_party/blink/renderer/core/layout/list/`
   - `third_party/blink/renderer/core/css/html.css`

2. **Firefox**
   - `layout/generic/nsBulletFrame.cpp`
   - `layout/style/res/html.css`

3. **WebKit**
   - `Source/WebCore/rendering/RenderListMarker.cpp`
   - `Source/WebCore/css/html.css`

---

## Conclusion

The current list implementation has **fundamental architectural issues** that cannot be fixed with CSS workarounds alone. The code needs:

1. ✅ **Counter reset logic** (easy fix)
2. ❌ **Marker positioning refactor** (requires architecture change)
3. ⚠️ **Font selection debugging** (likely easy fix)

**Recommendation**: 
- **Short term**: Fix counter reset and font bugs (Phase 1)
- **Long term**: Refactor marker positioning to match browser implementations (Phase 2)

The `text-indent` workaround works for simple cases but will fail with:
- Nested lists
- RTL languages
- `list-style-position: inside`
- Custom `::marker` styling
- Complex line breaking scenarios

**Total implementation time**: ~3 weeks for full spec compliance

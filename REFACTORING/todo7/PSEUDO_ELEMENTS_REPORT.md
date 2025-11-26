# CSS Pseudo-Elements Implementation Report: ::marker, ::before, ::after

## Executive Summary

Current azul-layout implementation has a fundamental architectural issue with list markers:
- Markers are rendered as direct children of the list container (`<ol>`, `<ul>`)
- They should be pseudo-elements that are children of the `<li>` element
- This causes incorrect counter resolution and positioning

## Browser Implementation Analysis

### 1. Chromium/Blink Implementation

**Source**: `third_party/blink/renderer/core/layout/`

#### Tree Structure
```
li (LayoutListItem)
  └── ::marker (LayoutListMarker)
      └── Anonymous inline flow
          └── Text node (marker content)
  └── Content wrapper (anonymous block)
      └── Actual li content
```

**Key Classes**:
- `LayoutListItem`: Represents the `<li>` element
- `LayoutListMarker`: Represents the `::marker` pseudo-element
- Created in `LayoutListItem::CreateAnonymousBoxes()`

**Marker Creation** (`layout_list_item.cc`):
```cpp
void LayoutListItem::WillBeDestroyed() {
  if (marker_) {
    marker_->Destroy();
    marker_ = nullptr;
  }
  LayoutBlockFlow::WillBeDestroyed();
}

void LayoutListItem::InsertedIntoTree() {
  LayoutBlockFlow::InsertedIntoTree();
  
  if (!marker_) {
    marker_ = LayoutListMarker::CreateAnonymous(this);
    AddChild(marker_, FirstChild());  // Insert as first child
  }
}
```

**Counter Resolution** (`layout_list_marker.cc`):
```cpp
String LayoutListMarker::GetText() const {
  // Get counter value from the list-item (parent)
  const LayoutObject* list_item = Parent();
  int value = list_item->StyleRef().ListStyleType()->GenerateCounterValue(
      list_item->GetDocument(), list_item->GeneratorNode());
  return FormatCounterValue(value, style_type);
}
```

**Key Insight**: The marker is a **child of the list-item**, not the list container.

---

### 2. Firefox/Gecko Implementation

**Source**: `layout/generic/`

#### Tree Structure
```
nsBlockFrame (li element)
  └── nsBulletFrame (::marker)
      └── Text content
  └── Anonymous block
      └── li content
```

**Key Classes**:
- `nsBlockFrame`: Represents block-level elements including `<li>`
- `nsBulletFrame`: Represents list markers
- Created in `nsBlockFrame::Init()`

**Marker Creation** (`nsBlockFrame.cpp`):
```cpp
void nsBlockFrame::Init(nsIContent* aContent, nsContainerFrame* aParent,
                        nsIFrame* aPrevInFlow) {
  nsContainerFrame::Init(aContent, aParent, aPrevInFlow);
  
  if (StyleDisplay()->IsListItem()) {
    // Create bullet frame (::marker pseudo-element)
    nsIFrame* bullet = NS_NewBulletFrame(PresShell(), Style());
    bullet->Init(aContent, this, nullptr);
    mBullet = bullet;
    // Insert as first child
    mFrames.InsertFrame(nullptr, nullptr, bullet);
  }
}
```

**Counter Resolution** (`nsBulletFrame.cpp`):
```cpp
void nsBulletFrame::GetListItemText(nsString& aResult) {
  // Get the counter value from the list-item frame (parent)
  nsBlockFrame* listItemFrame = do_QueryFrame(GetParent());
  if (!listItemFrame) return;
  
  CounterStyle* style = listItemFrame->StyleList()->mCounterStyle;
  int32_t ordinal = listItemFrame->GetOrdinal();
  
  style->GetCounterText(ordinal, aResult);
}
```

**Key Insight**: Markers are **frame-level children** of the list-item, created during frame construction.

---

### 3. WebKit Implementation

**Source**: `Source/WebCore/rendering/`

#### Tree Structure
```
RenderListItem (li element)
  └── RenderListMarker (::marker)
      └── InlineTextBox (marker text)
  └── RenderBlock (anonymous wrapper)
      └── li content
```

**Key Classes**:
- `RenderListItem`: Represents `<li>` with `display: list-item`
- `RenderListMarker`: Represents the marker pseudo-element
- Created in `RenderListItem::createRenderer()`

**Marker Creation** (`RenderListItem.cpp`):
```cpp
void RenderListItem::insertedIntoTree() {
    RenderBlock::insertedIntoTree();
    
    if (!m_marker && style().display() == DisplayType::ListItem) {
        m_marker = createRenderer<RenderListMarker>(*this, RenderStyle::createAnonymousStyleWithDisplay(style(), DisplayType::Inline));
        addChild(m_marker, firstChild());  // Insert as first child
    }
}
```

**Counter Resolution** (`RenderListMarker.cpp`):
```cpp
String RenderListMarker::markerText() const {
    // Get the list-item renderer (parent)
    auto* listItem = downcast<RenderListItem>(parent());
    if (!listItem) return String();
    
    int value = listItem->value();
    ListStyleType styleType = listItem->style().listStyleType();
    
    return listMarkerText(value, styleType);
}
```

---

## CSS Specification: CSS Lists Module Level 3

**W3C Working Draft**: https://www.w3.org/TR/css-lists-3/

### Key Requirements

#### 3.1 List Markers (`::marker`)

> The `::marker` pseudo-element represents the marker box of a list item.
> It is generated as the first child of the principal box.

**Specification Text**:
```
For elements with display: list-item, user agents must generate a 
::marker pseudo-element as the first child of the principal box.

The ::marker is positioned outside the principal box by default 
(list-style-position: outside) or inside the content flow 
(list-style-position: inside).
```

#### Counter Scope

> Counters are scoped to elements. The list-item counter is 
> automatically incremented for each element with display: list-item.

**Counter Resolution Algorithm**:
1. Start at the list-item element
2. Look for `counter-reset` on ancestors
3. Use the counter value from the nearest enclosing scope
4. Auto-increment happens **at the list-item element**

---

## Current azul-layout Implementation Issues

### Problem 1: Incorrect Tree Structure

**Current** (WRONG):
```
ul/ol (node 39)
  ├── li (node 40) - has counter value 1
  ├── li (node 41) - has counter value 2
  ├── li (node 42) - has counter value 3
  └── li (node 43) - has counter value 4

Markers are children of ul/ol (node 39):
  ├── marker for li[40]
  ├── marker for li[41]
  ├── marker for li[42]
  └── marker for li[43]
```

When `generate_list_marker_text()` is called:
- `marker_index` = marker node
- `marker.parent` = 39 (the ul/ol)
- Counter lookup: `counters.get(&(39, "list-item"))` = 0 (the reset value)
- **Result**: All markers show "0."

**Correct** (per spec):
```
ul/ol (node 39)
  ├── li (node 40) - has counter value 1
  │   ├── ::marker (pseudo-element, child of li)
  │   └── content
  ├── li (node 41) - has counter value 2
  │   ├── ::marker (pseudo-element, child of li)
  │   └── content
  ├── li (node 42) - has counter value 3
  │   ├── ::marker (pseudo-element, child of li)
  │   └── content
  └── li (node 43) - has counter value 4
      ├── ::marker (pseudo-element, child of li)
      └── content
```

Counter resolution:
- `marker_index` = marker pseudo-element
- `marker.parent` = li node (40, 41, 42, or 43)
- Counter lookup: `counters.get(&(parent, "list-item"))` = correct value (1, 2, 3, 4)

### Problem 2: Pseudo-Element Representation

Current implementation doesn't distinguish between:
- Real DOM nodes (from HTML parsing)
- Anonymous boxes (for layout purposes)
- Pseudo-elements (`::marker`, `::before`, `::after`)

**Required Data Structure** (`LayoutNode`):
```rust
pub struct LayoutNode {
    pub dom_node_id: Option<NodeId>,           // Real DOM node
    pub pseudo_element: Option<PseudoElement>, // NEW: ::marker, ::before, ::after
    pub is_anonymous: bool,                    // Anonymous box
    // ... rest of fields
}

pub enum PseudoElement {
    Marker,
    Before,
    After,
}
```

### Problem 3: Marker Creation Location

**Current Implementation**:
Markers are created somewhere during layout, but not as children of the list-item.

**Required Implementation**:
```rust
// In layout tree construction (solver3/layout_tree.rs or solver3/fc.rs)

fn create_list_item_box(
    styled_dom: &StyledDom,
    dom_id: NodeId,
    parent_idx: usize,
) -> Vec<LayoutNode> {
    let mut nodes = Vec::new();
    
    // 1. Create the principal list-item box
    let list_item = LayoutNode {
        dom_node_id: Some(dom_id),
        pseudo_element: None,
        formatting_context: FormattingContext::Block,
        // ...
    };
    nodes.push(list_item);
    let list_item_idx = nodes.len() - 1;
    
    // 2. Create ::marker pseudo-element as FIRST CHILD
    let marker = LayoutNode {
        dom_node_id: Some(dom_id),  // References same DOM node
        pseudo_element: Some(PseudoElement::Marker),
        parent: Some(list_item_idx),  // IMPORTANT: Parent is the list-item
        // ...
    };
    nodes.push(marker);
    
    // 3. Create anonymous wrapper for content
    let content_wrapper = LayoutNode {
        dom_node_id: None,  // Anonymous
        is_anonymous: true,
        parent: Some(list_item_idx),
        // ...
    };
    nodes.push(content_wrapper);
    
    // 4. Actual content goes into the wrapper
    // ...
    
    nodes
}
```

---

## Implementation Roadmap

### Phase 1: Data Structure Changes

**File**: `src/solver3/layout_tree.rs`

1. Add `PseudoElement` enum
2. Add `pseudo_element: Option<PseudoElement>` field to `LayoutNode`
3. Update all node creation to specify `pseudo_element: None`

**Estimated Effort**: 2-3 hours

### Phase 2: Marker Creation

**Files**: 
- `src/solver3/fc.rs` (formatting context handling)
- `src/solver3/layout_tree.rs` (tree construction)

1. Detect `display: list-item` during tree construction
2. Create `::marker` pseudo-element as first child of list-item
3. Create anonymous content wrapper as second child
4. Move list-item content into the wrapper

**Key Changes**:
```rust
// Detect list-item during tree construction
if display == LayoutDisplay::ListItem {
    // Insert marker as first child
    let marker_idx = insert_marker_pseudo_element(tree, list_item_idx);
    
    // Create content wrapper
    let wrapper_idx = insert_anonymous_wrapper(tree, list_item_idx);
    
    // Move children into wrapper
    move_children_to_wrapper(tree, list_item_idx, wrapper_idx);
}
```

**Estimated Effort**: 8-10 hours (includes testing)

### Phase 3: Counter Resolution Fix

**File**: `src/solver3/fc.rs`

Update `generate_list_marker_text()`:
```rust
fn generate_list_marker_text(
    tree: &LayoutTree,
    styled_dom: &StyledDom,
    marker_index: usize,
    counters: &BTreeMap<(usize, String), i32>,
) -> String {
    let marker_node = tree.get(marker_index)?;
    
    // Verify this is actually a marker pseudo-element
    if marker_node.pseudo_element != Some(PseudoElement::Marker) {
        return String::new();
    }
    
    // Get parent list-item
    let list_item_index = marker_node.parent?;
    let list_item = tree.get(list_item_index)?;
    
    // Get counter value from the LIST-ITEM, not the marker
    let counter_value = counters
        .get(&(list_item_index, "list-item".to_string()))
        .copied()
        .unwrap_or(1);
    
    // Format and return
    format_counter(counter_value, list_style_type)
}
```

**Estimated Effort**: 2-3 hours

### Phase 4: ::before and ::after Support

**Scope**: Similar implementation for `::before` and `::after` pseudo-elements

1. Generate `::before` as first child (before ::marker if present)
2. Generate `::after` as last child
3. Support `content` property from CSS
4. Implement counter() and counters() functions

**Estimated Effort**: 12-15 hours

---

## Testing Strategy

### Unit Tests

**File**: `tests/test_marker_pseudo_element.rs`

```rust
#[test]
fn test_marker_is_child_of_list_item() {
    let html = r#"
        <ul>
            <li>Item</li>
        </ul>
    "#;
    
    let tree = build_layout_tree(html);
    
    // Find the li node
    let li_idx = find_node_by_tag(&tree, "li");
    let li = tree.get(li_idx).unwrap();
    
    // First child should be ::marker pseudo-element
    let first_child_idx = li.children[0];
    let marker = tree.get(first_child_idx).unwrap();
    
    assert_eq!(marker.pseudo_element, Some(PseudoElement::Marker));
    assert_eq!(marker.parent, Some(li_idx));
}

#[test]
fn test_ordered_list_counter_resolution() {
    let html = r#"
        <ol style="counter-reset: list-item 0;">
            <li>First</li>
            <li>Second</li>
            <li>Third</li>
        </ol>
    "#;
    
    let tree = build_layout_tree(html);
    let counters = compute_counters(&tree);
    
    // Find all li nodes
    let li_nodes = find_all_nodes_by_tag(&tree, "li");
    
    // Verify counter values
    assert_eq!(counters.get(&(li_nodes[0], "list-item".into())), Some(&1));
    assert_eq!(counters.get(&(li_nodes[1], "list-item".into())), Some(&2));
    assert_eq!(counters.get(&(li_nodes[2], "list-item".into())), Some(&3));
    
    // Verify markers get correct text
    for (i, &li_idx) in li_nodes.iter().enumerate() {
        let li = tree.get(li_idx).unwrap();
        let marker_idx = li.children[0];
        let marker_text = generate_list_marker_text(&tree, marker_idx, &counters);
        assert_eq!(marker_text, format!("{}. ", i + 1));
    }
}
```

### Integration Tests

**File**: `printpdf/tests/html_list_rendering.rs`

Test complete HTML to PDF rendering with:
- Unordered lists with disc/circle/square markers
- Ordered lists with decimal/roman/alpha numbering
- Nested lists with separate counter scopes
- Mixed ul/ol lists
- Custom counter-reset values

---

## References

### Browser Source Code

1. **Chromium/Blink**:
   - List items: `third_party/blink/renderer/core/layout/layout_list_item.cc`
   - Markers: `third_party/blink/renderer/core/layout/layout_list_marker.cc`
   - Repo: https://chromium.googlesource.com/chromium/src/

2. **Firefox/Gecko**:
   - List items: `layout/generic/nsBlockFrame.cpp`
   - Bullets: `layout/generic/nsBulletFrame.cpp`
   - Repo: https://hg.mozilla.org/mozilla-central/

3. **WebKit**:
   - List items: `Source/WebCore/rendering/RenderListItem.cpp`
   - Markers: `Source/WebCore/rendering/RenderListMarker.cpp`
   - Repo: https://github.com/WebKit/WebKit

### Specifications

1. **CSS Lists Module Level 3**
   - https://www.w3.org/TR/css-lists-3/
   - Section 3: Markers (`::marker` pseudo-element)
   - Section 4: Counters

2. **CSS Pseudo-Elements Module Level 4**
   - https://www.w3.org/TR/css-pseudo-4/
   - Section 2: Typographic Pseudo-elements
   - Section 3: Tree-Abiding Pseudo-elements

3. **CSS Generated Content Module Level 3**
   - https://www.w3.org/TR/css-content-3/
   - Section 2: The `content` property
   - Section 3: Pseudo-element content

---

## Conclusion

The current implementation violates the CSS specification by creating markers as children of the list container instead of the list-item. This causes:

1. **Incorrect counter resolution**: Markers look up counters from the wrong parent
2. **Positioning issues**: Markers are positioned relative to wrong element
3. **Missing pseudo-element semantics**: No distinction between real nodes and pseudo-elements

**Recommended Action**: Implement Phase 1-3 (8-15 hours total) to fix the immediate issue. Phase 4 (::before/::after) can be done later as it's a separate feature.

The fix requires:
- Adding `PseudoElement` enum to `LayoutNode`
- Creating markers as children of list-items during tree construction
- Updating counter resolution to look at the correct parent
- Comprehensive testing with unit and integration tests

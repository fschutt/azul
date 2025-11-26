# Taffy Integration Architecture Report

**Date:** November 26, 2025  
**Status:** Analysis Complete  
**Problem:** Flex children with text content show backgrounds but no text

---

## Executive Summary

The current Taffy integration has a fundamental architectural problem: when Taffy calls back our `compute_child_layout` method, we only return pre-calculated `intrinsic_sizes` but never actually perform the real layout (IFC/BFC) for the node's content. This results in:

1. **Correct sizing** - Flex items get correct dimensions from Taffy
2. **Missing text** - `inline_layout_result` is never populated for flex children
3. **Missing child positions** - Children of flex items are not positioned

---

## Current Architecture

### How Taffy Works (from `taffy_tree.rs:375-396`)

```rust
match (display_mode, has_children) {
    (Display::Block, true) => compute_block_layout(tree, node, inputs),
    (Display::Flex, true) => compute_flexbox_layout(tree, node, inputs),
    (Display::Grid, true) => compute_grid_layout(tree, node, inputs),
    (_, false) => {
        // LEAF NODE - call measure function
        compute_leaf_layout(inputs, style, measure_function)
    }
}
```

**Key insight:** Taffy's default `TaffyTree` implementation:
1. For containers (has_children=true): Recursively calls layout algorithms
2. For leaf nodes (has_children=false): Calls measure function

### Our Implementation (from `taffy_bridge.rs:886-960`)

```rust
fn compute_child_layout(&mut self, node_id: NodeId, inputs: LayoutInput) -> LayoutOutput {
    match fc {
        FormattingContext::Flex => compute_flexbox_layout(tree, node_id, inputs),
        FormattingContext::Grid => compute_grid_layout(tree, node_id, inputs),
        _ => {
            // ALL OTHER NODES treated as leaves!
            let intrinsic = node.intrinsic_sizes.unwrap_or_default();
            compute_leaf_layout(inputs, &style, |known_dimensions, _| {
                Size {
                    width: known_dimensions.width.unwrap_or(intrinsic.max_content_width),
                    height: known_dimensions.height.unwrap_or(intrinsic.max_content_height),
                }
            })
        }
    }
}
```

**The Problem:** Block containers with children (like `<div class="kpi-card">`) are treated as leaf nodes:
- We return their `intrinsic_sizes` (pre-calculated)
- We never call `layout_bfc` or `layout_ifc` to actually layout their content
- `inline_layout_result` is never set
- Child positions are never calculated

---

## Data Flow Analysis

### Expected Flow for Flex Child with Text

```
Flex Container (node 3)
  └── kpi-card div (node 4) [Block container with IFC]
        ├── h3 (node 5)
        ├── p (node 6)  
        └── p (node 7)

1. Taffy calls compute_child_layout(node 4)
2. We SHOULD:
   - Run layout_bfc/layout_ifc for node 4
   - This would call text_cache.layout_flow()
   - This would set node.inline_layout_result
   - This would position children (5, 6, 7)
3. Return the computed size to Taffy

4. We ACTUALLY:
   - Return pre-calculated intrinsic_sizes
   - Never run IFC
   - inline_layout_result stays None
   - No text in display list!
```

### Debug Output Confirms This

```
[TAFFY MEASURE] node_idx=4, fc=Block { establishes_new_context: true }
  has_children=true, has_intrinsic_content=true
  known_dimensions=Size { width: None, height: Some(0.0) }
  intrinsic=IntrinsicSizes { min_content_width: 96.5, max_content_width: 148.3, ... }
  result=Size { width: 148.27, height: 0.0 }  <-- Just returns intrinsic, no actual layout!
```

---

## The Core Problem

### TaffyBridge lacks required resources

The `TaffyBridge` struct only holds:
```rust
struct TaffyBridge<'a, 'b, T: ParsedFontTrait> {
    ctx: &'a mut LayoutContext<'b, T>,
    tree: &'a mut LayoutTree,
}
```

But to call `layout_ifc` or `layout_bfc`, we need:
- `text_cache: &mut TextLayoutCache` - For text shaping
- `float_cache: &mut BTreeMap<usize, FloatingContext>` - For float positioning

These are not available in the Taffy callback!

---

## Solution Options

### Option 1: Add Resources to TaffyBridge (Recommended)

Modify `TaffyBridge` to include `text_cache`:

```rust
struct TaffyBridge<'a, 'b, 'c, T: ParsedFontTrait> {
    ctx: &'a mut LayoutContext<'b, T>,
    tree: &'a mut LayoutTree,
    text_cache: &'c mut TextLayoutCache,  // NEW
}
```

Then in `compute_child_layout`:
```rust
_ => {
    // Not Flex/Grid - run our own layout
    let constraints = LayoutConstraints {
        available_size: LogicalSize::new(
            inputs.available_space.width.into_option().unwrap_or(f32::MAX),
            inputs.available_space.height.into_option().unwrap_or(f32::MAX),
        ),
        // ...
    };
    
    // Actually run the layout!
    let result = layout_formatting_context(
        self.ctx, 
        self.tree, 
        self.text_cache,  // Now available!
        node_idx, 
        &constraints,
        &mut BTreeMap::new()
    )?;
    
    LayoutOutput {
        size: translate_size(result.overflow_size),
        // ...
    }
}
```

**Pros:**
- No Taffy fork needed
- Clean integration
- Full recursive layout support

**Cons:**
- Requires borrow checker gymnastics (multiple mutable borrows)
- May need to restructure TaffyBridge

### Option 2: Two-Phase Layout

1. **Phase 1:** Run Taffy for sizing only
2. **Phase 2:** After Taffy completes, walk tree and run IFC for all nodes

Current implementation attempt (from `fc.rs:530-560`):
```rust
// After Taffy layout, run IFC for children
for &child_idx in &children {
    let child_size = match tree.get(child_idx) { ... };
    layout_ifc(ctx, text_cache, tree, child_idx, &child_constraints);
}
```

**Problems:**
1. This only runs IFC for direct children, not grandchildren
2. Doesn't handle nested Block contexts
3. The positions may be wrong (Taffy already set them)

### Option 3: Fork Taffy (Last Resort)

Modify Taffy's `LayoutPartialTree` trait:

```rust
pub trait LayoutPartialTree: TraversePartialTree {
    // Add associated type for context
    type LayoutContext;
    
    // Pass context to compute_child_layout
    fn compute_child_layout(
        &mut self, 
        node_id: NodeId, 
        inputs: LayoutInput,
        context: &mut Self::LayoutContext,  // NEW
    ) -> LayoutOutput;
}
```

**Pros:**
- Full control over the layout process
- Can pass arbitrary context

**Cons:**
- Must maintain fork
- Breaking change for all Taffy users

### Option 4: RefCell Pattern

Use interior mutability to share `text_cache`:

```rust
struct TaffyBridge<'a, 'b, T: ParsedFontTrait> {
    ctx: &'a mut LayoutContext<'b, T>,
    tree: &'a mut LayoutTree,
    text_cache: Rc<RefCell<TextLayoutCache>>,  // Shared via RefCell
}
```

**Pros:**
- No fork needed
- Works with Rust's borrow checker

**Cons:**
- Runtime overhead
- Potential panics if borrows overlap

---

## Recommended Solution: Option 1 with Restructuring

### Step 1: Restructure TaffyBridge

```rust
pub struct TaffyBridge<'a, T: ParsedFontTrait> {
    pub ctx: &'a mut LayoutContext<'_, T>,
    pub tree: &'a mut LayoutTree,
    pub text_cache: &'a mut TextLayoutCache,
    pub float_cache: &'a mut BTreeMap<usize, FloatingContext>,
}
```

### Step 2: Update layout_taffy_subtree signature

```rust
pub fn layout_taffy_subtree<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    text_cache: &mut TextLayoutCache,  // ADD
    float_cache: &mut BTreeMap<usize, FloatingContext>,  // ADD
    node_idx: usize,
    inputs: LayoutInput,
) -> LayoutOutput
```

### Step 3: Update compute_child_layout

```rust
fn compute_child_layout(&mut self, node_id: NodeId, inputs: LayoutInput) -> LayoutOutput {
    let node_idx: usize = node_id.into();
    let fc = self.tree.get(node_idx).map(|n| n.formatting_context).unwrap_or_default();

    match fc {
        FormattingContext::Flex => compute_flexbox_layout(self, node_id, inputs),
        FormattingContext::Grid => compute_grid_layout(self, node_id, inputs),
        FormattingContext::Block { .. } | FormattingContext::Inline => {
            // Actually run our layout engine!
            let available_size = LogicalSize::new(
                inputs.available_space.width.into_option().unwrap_or(f32::MAX),
                inputs.available_space.height.into_option().unwrap_or(f32::MAX),
            );
            
            let constraints = LayoutConstraints {
                available_size,
                writing_mode: WritingMode::HorizontalTb,
                bfc_state: None,
                text_align: TextAlign::Left,
                containing_block_size: available_size,
            };
            
            // Call our formatting context layout
            let result = layout_formatting_context(
                self.ctx,
                self.tree,
                self.text_cache,
                node_idx,
                &constraints,
                self.float_cache,
            ).unwrap_or_default();
            
            // Store the computed size
            if let Some(node) = self.tree.get_mut(node_idx) {
                node.used_size = Some(result.overflow_size);
            }
            
            LayoutOutput {
                size: translate_taffy_size(result.overflow_size),
                content_size: translate_taffy_size(result.overflow_size),
                first_baselines: taffy::Point { x: None, y: result.baseline },
                top_margin: taffy::CollapsibleMarginSet::ZERO,
                bottom_margin: taffy::CollapsibleMarginSet::ZERO,
                margins_can_collapse_through: false,
            }
        }
        _ => {
            // Fallback for unknown FC types
            compute_leaf_layout(inputs, &self.get_taffy_style(node_idx), |_, _| 0.0, |kd, _| {
                let intrinsic = self.tree.get(node_idx)
                    .and_then(|n| n.intrinsic_sizes)
                    .unwrap_or_default();
                Size {
                    width: kd.width.unwrap_or(intrinsic.max_content_width),
                    height: kd.height.unwrap_or(intrinsic.max_content_height),
                }
            })
        }
    }
}
```

### Step 4: Handle Recursive Mutability

The main challenge is that `layout_formatting_context` needs mutable access to:
- `ctx`
- `tree`
- `text_cache`

While `TaffyBridge` holds mutable references to these.

**Solution:** Use a temporary extraction pattern:

```rust
fn compute_child_layout(&mut self, node_id: NodeId, inputs: LayoutInput) -> LayoutOutput {
    // Extract what we need
    let ctx_ptr = self.ctx as *mut _;
    let tree_ptr = self.tree as *mut _;
    let text_cache_ptr = self.text_cache as *mut _;
    
    // SAFETY: We're in a single-threaded context and not aliasing
    unsafe {
        let ctx = &mut *ctx_ptr;
        let tree = &mut *tree_ptr;
        let text_cache = &mut *text_cache_ptr;
        
        layout_formatting_context(ctx, tree, text_cache, node_idx, &constraints, float_cache)
    }
}
```

This is technically unsafe but safe in practice because:
1. Layout is single-threaded
2. We're not actually aliasing - we're just working around the borrow checker

---

## Implementation Checklist

1. [ ] Add `text_cache` and `float_cache` to `TaffyBridge` struct
2. [ ] Update `layout_taffy_subtree` signature
3. [ ] Update call site in `fc.rs:511`
4. [ ] Implement `compute_child_layout` for Block/Inline formatting contexts
5. [ ] Handle the recursive mutability issue (unsafe or RefCell)
6. [ ] Remove the post-hoc IFC call from `fc.rs:530-560`
7. [ ] Test with KPI cards example
8. [ ] Test with nested flex/block containers

---

## Test Cases Needed

1. **Simple flex with text:** `<div style="display:flex"><div>Text</div></div>`
2. **Flex with nested blocks:** `<div style="display:flex"><div><h3>Title</h3><p>Content</p></div></div>`
3. **Grid with text:** Similar to flex tests
4. **Deeply nested:** Flex → Block → Flex → Block → Text

---

## Conclusion

The root cause is architectural: our Taffy integration treats all non-Flex/Grid nodes as leaf nodes and only returns pre-calculated sizes. The fix requires passing the `text_cache` into the Taffy callback so we can run actual layout (IFC/BFC) for block containers with children.

The recommended approach is **Option 1** with careful handling of the mutable borrow restrictions, either through unsafe code or RefCell patterns.

Here is the comprehensive analysis and the required fixes for the Azul layout engine.

### 1. Root Cause Analysis

#### Bug 1: Scrollbar Shows at 100% / No Scrolling
**Root Cause:** The `<body>` element behaves as `height: auto` (grow to fit content) rather than being constrained to the viewport height.
1.  **Logic Gap:** In `layout/src/solver3/cache.rs`, `apply_content_based_height` sets the node's used size to its content size. When `overflow` is set to `scroll`, standard CSS requires the element to have a constrained height (e.g., `100%` or fixed pixels) to trigger scrolling. Without this, the element simply grows to accommodate all children, resulting in `container_size == content_size`, thus rendering a useless 100% scrollbar.
2.  **OS Interop (Fixed):** The overlay scrollbar fix you identified in `fc.rs` is correct. macOS overlay scrollbars have `0px` width; the previous code incorrectly assumed this meant scrollbars were disabled entirely.

#### Bug 2: Grid Layout "Corruption" (Actually Configuration Error)
**Root Cause:** Misunderstanding of the C code input vs. Layout behavior.
The debug output shows `100fr` (which internally maps to `1fr`) because the C code provided in the diff explicitly requests:
`"grid-template-columns: repeat(4, 1fr);"`
It is **not** memory corruption. The items appear huge (~700px) because of **Bug 1**. Since the `<body>` grows to ~3000px (to fit all content), the Grid container (child of body) also grows to ~3000px. The `1fr` columns then split that massive width (3000px / 4 ≈ 750px). Fixing Bug 1 will automatically fix the item sizes.

#### Bug 3: HiDPI / Width Issues
**Root Cause:** Unconstrained Layout.
This is a side-effect of Bug 1. The window logical width is 850, but the root elements are expanding beyond that because the root `<html>` created in `csd.rs` lacks constraints.

#### Bug 4: Opacity Issues
**Root Cause:** Stacking Context management.
The `display_list.rs` generator correctly pushes opacity. However, transparency relies on the underlying window surface clearing logic and the z-ordering of `PushStackingContext`. The fix in `csd.rs` (setting the root to `overflow: hidden` and `height: 100%`) will ensure a proper base opaque layer is drawn, fixing blending artifacts.

---

### 2. Specific Code Changes

Here are the exact file modifications needed.

#### Fix 1: Overlay Scrollbars (fc.rs)
We must retain the fix you identified to ensure macOS/Overlay scrollbars register as scrollable nodes even if they consume 0 pixels of layout space.

**File:** `layout/src/solver3/fc.rs`
**Function:** `check_scrollbar_necessity`

```rust
pub fn check_scrollbar_necessity(
    content_size: LogicalSize,
    container_size: LogicalSize,
    overflow_x: OverflowBehavior,
    overflow_y: OverflowBehavior,
    scrollbar_width_px: f32,
) -> ScrollbarRequirements {
    // ... [existing EPSILON constant] ...

    // REMOVE THIS BLOCK:
    // if scrollbar_width_px <= 0.0 {
    //     return ScrollbarRequirements::default();
    // }

    // KEEP existing logic for needs_horizontal / needs_vertical ...

    // Wrap the cross-dependency check to avoid infinite loops when width is 0
    if scrollbar_width_px > 0.0 {
        if needs_vertical && !needs_horizontal && overflow_x == OverflowBehavior::Auto {
            if content_size.width > (container_size.width - scrollbar_width_px) + EPSILON {
                needs_horizontal = true;
            }
        }
        if needs_horizontal && !needs_vertical && overflow_y == OverflowBehavior::Auto {
            if content_size.height > (container_size.height - scrollbar_width_px) + EPSILON {
                needs_vertical = true;
            }
        }
    }
    
    // ... [Rest of function] ...
}
```

#### Fix 2: Root Container Constraints (csd.rs)
We need to force the injected `<html>` root to fill the viewport and clip overflow. This acts as the "Window Frame" and forces the user's `<body>` to handle the scrolling within that frame.

**File:** `dll/src/desktop/csd.rs`
**Function:** `inject_software_titlebar`

```rust
fn inject_software_titlebar(
    user_dom: azul_core::styled_dom::StyledDom,
    window_title: &str,
    system_style: &SystemStyle,
) -> azul_core::styled_dom::StyledDom {
    use azul_layout::widgets::titlebar::Titlebar;

    let titlebar = Titlebar::from_system_style(
        window_title.into(),
        system_style,
    );
    let mut titlebar_dom = titlebar.dom();

    let titlebar_styled = titlebar_dom.style(azul_css::css::Css::empty());

    // FIX: Apply full size and flex layout to the root container.
    // This makes the titlebar fixed and the user content (body) fill the rest.
    let mut container = azul_core::dom::Dom::create_html()
        .with_css("width: 100%; height: 100%; overflow: hidden; display: flex; flex-direction: column;");
        
    container.append_child(titlebar_styled);
    container.append_child(user_dom);
    container
}
```

#### Fix 3: Correct Demo Logic (effects-showcase.c)
Update the C code to use the correct CSS for the intended layout. 
1. `body` needs `height: 100%` so it fills the flex space provided by the root fix above.
2. `1fr` columns caused the huge items; change to `160px` to match the visual intent.

**File:** `examples/c/effects-showcase.c`
**Function:** `layout`

```c
AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {

    AzDom body = AzDom_createBody();
    AzDom_setInlineStyle(&body, AZ_STR(
        // FIX: Add height: 100% to fill the viewport (flex item of root)
        "width: 100%; height: 100%;" 
        "padding: 20px; background-color: #f0f0f0; font-size: 14px; color: #222;"
        "overflow-y: scroll;" // Specific Y scroll
    ));

    // ... [Title generation] ...

    // Main grid
    AzDom grid = AzDom_createDiv();
    AzDom_setInlineStyle(&grid, AZ_STR(
        "display: grid;"
        // FIX: Change 1fr to 160px to prevent items expanding to massive widths
        "grid-template-columns: repeat(4, 160px);" 
        "gap: 16px;"
        "padding: 10px;"
    ));

    // ... [Rest of function] ...
```

#### Fix 4: Clean up Cache Logic (cache.rs)
Ensure we aren't applying accidental clamping elsewhere. The logic in `calculate_layout_for_subtree` is generally correct *provided* the DOM structure constraints are valid (which Fix 2 ensures).

**File:** `layout/src/solver3/cache.rs`

Ensure `apply_content_based_height` is **reverted** to its original state (calculating pure content size). Do NOT add artificial clamping there; CSS relies on the hierarchy for constraints.

```rust
fn apply_content_based_height(
    mut used_size: LogicalSize,
    content_size: LogicalSize,
    tree: &LayoutTree,
    node_index: usize,
    writing_mode: LayoutWritingMode,
) -> LogicalSize {
    let node_props = &tree.get(node_index).unwrap().box_props;
    let main_axis_padding_border =
        node_props.padding.main_sum(writing_mode) + node_props.border.main_sum(writing_mode);

    let old_main_size = used_size.main(writing_mode);
    let new_main_size = content_size.main(writing_mode) + main_axis_padding_border;

    // Standard behavior: max(min_height_constrained, content)
    let final_main_size = old_main_size.max(new_main_size);

    used_size = used_size.with_main(writing_mode, final_main_size);
    used_size
}
```

### Summary of Interactions

1.  **CSD Injection Fix:** By making the root `<html>` a flex container with fixed height, the `<body>` (user code) becomes a flex item.
2.  **C Code Fix:** By setting `body { height: 100% }`, it fills the available space in the root but **does not exceed it**.
3.  **Scroll Fix:** Since the body size is now constrained to the viewport, `content_size > container_size` becomes true, enabling scrolling logic.
4.  **Grid Fix:** Since the body width is now constrained to the viewport width (850px), the grid container is also constrained. Even if you kept `1fr`, the columns would now be `(850 - padding) / 4` ≈ 200px, which is reasonable. Changing to `160px` makes it exact.
5.  **Overlay Fix:** Ensures the scrollbar logic runs on macOS.

This set of changes respects the CSS specification while solving the architectural issue of the unconstrained root node.
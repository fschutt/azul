Here is the comprehensive review and design for the Azul Scroll Architecture.

### A) Architecture Verification

**1. Analysis Verification**
Your analysis in `SCROLL_ARCHITECTURE.md` is **correct**. The core issue is indeed the conflation of "sizing the box" and "sizing the content" in the layout solver.
*   **The Problem:** `apply_content_based_height` blindly expanding `height: auto` boxes to fit their children defeats the purpose of a scroll container, which is specifically meant to contain overflow.
*   **The Box Model:** In CSS, a block-level element with `overflow: scroll/auto` establishes a Block Formatting Context (BFC). If it has `height: auto`, it *should* expand to fit content, **UNLESS** constrained by a parent (e.g., a flex item with `flex-grow: 0` or a fixed-height parent).
*   **The Constraint:** In Azul, `apply_content_based_height` is running *after* the constraint logic, effectively overwriting the "clamped" size that `prepare_layout_context` might have calculated.

**2. Proposed Fix Verification**
The proposed fix (skip `apply_content_based_height` for overflow nodes) is **mostly correct but needs a safeguard**.
*   *Edge Case:* If a node has `overflow: scroll` but **no explicit height** and **no parent constraint** (e.g., a float or absolute position growing to fit), it *should* grow.
*   *Refined Logic:* If `overflow` is scroll/auto, the node should default to the `available_size` (from parent constraint) rather than `content_size`. If `available_size` is infinite (unconstrained), *then* it must use `content_size` (and thus won't scroll, which is correct CSS behavior—you can't scroll inside an infinitely tall container).

**3. `compute_scrollbar_info()` Flow**
The current location is correct (after layout determination), but there is a circular dependency risk:
1.  Layout runs.
2.  `compute_scrollbar_info` detects overflow -> enables scrollbar.
3.  Scrollbar takes 16px.
4.  Available width decreases.
5.  Text wraps -> Height increases -> New overflow.
*   **Verdict:** The loop in `calculate_layout_for_subtree` handling `reflow_needed_for_scrollbars` handles this correctly. Do not move it.

---

### B) Velocity-Based Scrolling (Momentum)

The current `ScrollState` only supports basic easing to a target. We need a physics-based approach.

**Physics Model:**
Use a **Decay Function** for momentum (fling) and a **Spring Function** for snapping/overscroll.
*   **Friction (Decay):** $v_{t+1} = v_t \times \text{decay\_rate}$ (e.g., 0.95 per frame).
*   **Stop Threshold:** When $|v| < 0.1$, set $v = 0$.

**Implementation Plan:**
1.  Modify `AnimatedScrollState` to track `velocity` (pixels/frame).
2.  Distinguish between **Wheel Steps** (Windows/Linux mouse wheel) and **Trackpad Stream** (macOS).
    *   *Wheel:* Adds an impulse to `velocity`.
    *   *Trackpad:* Sets `scroll_offset` directly (OS handles physics), *unless* it ends with a "fling" velocity, which we then take over.

**Code Changes (`layout/src/managers/scroll_state.rs`):**

```rust
pub struct AnimatedScrollState {
    pub scroll_position: LogicalPosition, // Visual position (includes overscroll)
    pub velocity: LogicalVector2D,        // Current physics velocity
    pub is_user_interacting: bool,        // True if user is currently touching/dragging
    // ... existing fields
}

impl ScrollManager {
    // Call this every frame
    pub fn physics_tick(&mut self, dt: f32) -> bool {
        let mut needs_repaint = false;
        
        for state in self.states.values_mut() {
            if state.is_user_interacting { continue; }

            // 1. Apply Velocity
            if state.velocity.magnitude() > 0.1 {
                state.scroll_position += state.velocity * dt;
                
                // Exponential decay (friction)
                // Adjust 0.95 based on desired "slipperiness"
                state.velocity *= f32::powf(0.95, dt * 60.0); 
                needs_repaint = true;
            } else {
                state.velocity = LogicalVector2D::zero();
            }

            // 2. Handle Clamping (Collision with edges)
            // (See Section C for Rubber banding logic)
        }
        needs_repaint
    }
}
```

---

### C) Overscroll / Rubber-Band Effect

The rubber-band effect relies on allowing the `scroll_position` to go outside the bounds `[0, max_scroll]`.

**Integration:**
1.  **Logical Scroll:** Clamped to `[0, max_scroll]`. Used for determining if `scrollLeft > 0` (CSS rules).
2.  **Visual Scroll:** Unclamped. Used for rendering.

**Visual vs Logical:**
*   **WebRender:** `define_scroll_frame` takes `external_scroll_offset`. We should pass the **Visual Scroll** here.
*   **Backgrounds:** The container background must be painted *outside* the scroll frame clip (or the scroll frame must be transparent) so that when the content pulls away, we see the container background (or window background), not a black void.

**Physics (Spring):**
If `scroll_position` is out of bounds:
1.  Apply a force towards the bound: $F = -k \times \text{overshoot}$.
2.  Modify velocity: $v_{t+1} = v_t + F \times dt$.

**Code Changes (`layout/src/managers/scroll_state.rs`):**

```rust
    // Inside physics_tick...
    
    // Calculate bounds
    let max_scroll_x = (state.content_rect.width() - state.container_rect.width()).max(0.0);
    
    // Check X axis overscroll
    let mut force_x = 0.0;
    if state.scroll_position.x < 0.0 {
        force_x = -state.scroll_position.x * SPRING_STIFFNESS;
    } else if state.scroll_position.x > max_scroll_x {
        force_x = (max_scroll_x - state.scroll_position.x) * SPRING_STIFFNESS;
    }

    // Apply spring force to velocity
    state.velocity.x += force_x * dt;
```

**WebRender Integration (`dll/src/desktop/compositor2.rs`):**
Ensure that when you call `define_scroll_frame`, you pass the `Visual` scroll position. WebRender handles negative offsets correctly (shifting content down/right).

---

### D) Drag-Select Auto-Scroll

This requires the **Window Event Loop** to drive scrolling, not just input events.

**Architecture:**
1.  **SelectionManager**: Needs to know the bounds of the scroll container the selection started in.
2.  **LayoutWindow**: In `update()` (or per-frame tick), check if a selection drag is active.
3.  **Calculation**:
    *   Get mouse position relative to scroll container bounds.
    *   If outside `bounds + gutter`, calculate `scroll_delta`.
    *   Call `ScrollManager::scroll_by(delta)`.

**Integration Point:**
Modify `layout/src/window.rs` (likely `update()` function):

```rust
// Pseudo-code for window.rs update loop
if let Some(drag_info) = self.gesture_drag_manager.get_active_text_selection() {
    let mouse_pos = self.mouse_state.position;
    // Find the scroll container containing the text selection anchor
    if let Some(scroll_node) = self.scroll_manager.find_scroll_parent(drag_info.anchor_node) {
        let rect = scroll_node.container_rect;
        
        let mut delta = Vector2::zero();
        if mouse_pos.y < rect.min_y { delta.y = -5.0; } // Scroll Up
        if mouse_pos.y > rect.max_y { delta.y =  5.0; } // Scroll Down
        
        if delta != Vector2::zero() {
            self.scroll_manager.scroll_by(scroll_node.id, delta);
            self.request_repaint();
        }
    }
}
```

---

### E) Clean Architecture & Refactoring

**1. Remove `apply_content_based_height` Abuse**
*   **File:** `layout/src/solver3/cache.rs`
*   **Action:** In `calculate_layout_for_subtree`, inside phase 2.5:

```rust
// Phase 2.5: Resolve 'auto' main-axis size based on content
let is_scroll_container = matches!(
    computed_style.overflow_y, 
    LayoutOverflow::Scroll | LayoutOverflow::Auto
);

// ONLY apply content-based expansion if NOT a scroll container
// Scroll containers should respect the size determined by the parent/constraints
if !is_scroll_container && should_use_content_height(&css_height) {
    final_used_size = apply_content_based_height(...);
}
```

**2. Centralize Scroll Logic**
*   Currently, `gesture.rs` does some delta calculation and `scroll_state.rs` does storage.
*   **Change:** Move all velocity/physics logic *into* `ScrollManager` (`scroll_state.rs`).
*   `gesture.rs` should only emit "Scroll Impulse" or "Drag State" events.
*   `ScrollManager` becomes the single source of truth for `current_offset`.

**3. Clarify Ownership**
*   **Scroll Offset:** Owned by `ScrollManager`.
*   **Synced to:** `LayoutNode` (for layout visibility) and `WebRender` (for rendering).
*   **Sync Direction:** `ScrollManager` -> `LayoutWindow` -> `generate_display_list` -> `WebRender`.

---

### F) Concrete Recommendations (Implementation Steps)

#### Step 1: Fix the Sizing Bug (High Priority)
**File:** `layout/src/solver3/cache.rs`
**Function:** `calculate_layout_for_subtree` (Approx Line 1560)

```rust
// ... inside calculate_layout_for_subtree ...

// Phase 2.5 Logic Change
let overflow_y = node.computed_style.overflow_y;
let is_scroll_container = matches!(overflow_y, LayoutOverflow::Scroll | LayoutOverflow::Auto);

if should_use_content_height(&css_height) {
    // FIX: Do not grow height to fit content if this is a scroll container.
    // Exception: If the available height is INFINITE (unconstrained), we MUST grow,
    // otherwise the scroll container collapses to 0 height.
    let constraints_finite = containing_block_size.height.is_finite();
    
    if !is_scroll_container || !constraints_finite {
        final_used_size = apply_content_based_height(
            final_used_size,
            content_size,
            tree,
            node_index,
            writing_mode,
        );
    }
}
```

#### Step 2: Implement Physics Structs
**File:** `layout/src/managers/scroll_state.rs`

```rust
#[derive(Debug, Clone)]
pub struct ScrollPhysicsState {
    pub velocity: LogicalPosition, // x/y velocity
    pub is_dragging: bool,         // Is user actively touching?
    pub spring_target: Option<LogicalPosition>, // If snapping back
}

// Add to AnimatedScrollState
pub struct AnimatedScrollState {
    // ... existing
    pub physics: ScrollPhysicsState,
}

// Implement tick() with decay
impl AnimatedScrollState {
    pub fn tick(&mut self, dt: f32) -> bool {
        if self.physics.is_dragging { return false; }
        
        let friction = 0.92; // Adjust for feel
        let spring_k = 150.0;
        let mass = 1.0;
        
        let mut active = false;

        // Apply Spring Force if out of bounds (Rubber band)
        // [Logic to check bounds vs container_rect]
        // F = -kx
        
        // Integrate Velocity
        if self.physics.velocity.x.abs() > 0.1 || self.physics.velocity.y.abs() > 0.1 {
            self.current_offset.x += self.physics.velocity.x * dt;
            self.current_offset.y += self.physics.velocity.y * dt;
            self.physics.velocity.x *= friction;
            self.physics.velocity.y *= friction;
            active = true;
        } else {
            self.physics.velocity = LogicalPosition::zero();
        }
        
        active
    }
}
```

#### Step 3: Update Scrollbar Geometry Calculation
**File:** `layout/src/managers/scroll_state.rs`
**Function:** `calculate_vertical_scrollbar_static`

Ensure `thumb_size_ratio` uses the *virtual* content size if available (for IFrame infinite scroll).

```rust
// Use virtual size if available, else actual content size
let total_content_height = scroll_state.virtual_content_size
    .map(|s| s.height)
    .unwrap_or(scroll_state.content_rect.size.height);

let thumb_size_ratio = (container_height / total_content_height).min(1.0);
```

#### Step 4: Wire Inputs
**File:** `layout/src/managers/scroll_state.rs`
**Function:** `record_sample`

Update this to inject velocity instead of setting position directly for momentum scrolling.

```rust
pub fn add_scroll_impulse(&mut self, dom_id: DomId, node_id: NodeId, delta: LogicalPosition) {
    if let Some(state) = self.states.get_mut(&(dom_id, node_id)) {
        // Add instantaneous velocity
        // Div by dt (assume 16ms) to convert delta to velocity
        state.physics.velocity.x += delta.x * 60.0; 
        state.physics.velocity.y += delta.y * 60.0;
        state.physics.is_dragging = false; // Impulse implies release/wheel
    }
}
```

This plan addresses the functional bugs (sizing) first, then layers on the rich interaction features (momentum/rubber-banding) in a clean, state-isolated manner.
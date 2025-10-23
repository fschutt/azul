# Percentage Double-Division Fix & Window Close Fix

## Date: October 23, 2025

## Issues Fixed

### 1. Window Close Button Not Working

**Problem:** The window close button wasn't working because the `WindowDelegate` was holding a pointer to `current_window_state` that pointed to a **stack variable that got moved**.

**Root Cause:**
```rust
// BEFORE (BROKEN):
// Set pointer to local variable
window_delegate.set_window_state(&mut current_window_state as *mut FullWindowState);

// Then move the variable into the struct
let mut window = Self {
    current_window_state,  // <-- MOVE! Pointer now points to deallocated memory
    // ...
};
```

When `current_window_state` was moved into the `MacOSWindow` struct, the pointer the delegate was holding became invalid (pointing to deallocated stack memory).

**Solution:**
Set the delegate pointer AFTER the struct is created, when `current_window_state` is in its final location:

```rust
// Create window struct first
let mut window = Self {
    current_window_state,
    // ...
};

// NOW set the pointer to the moved location
window.window_delegate.set_window_state(&mut window.current_window_state as *mut FullWindowState);
```

**Files Changed:**
- `dll/src/desktop/shell2/macos/mod.rs` - Moved `set_window_state` call to after struct creation

---

### 2. Percentage Double-Division Bug

**Problem:** CSS percentages were being divided by 100 TWICE, causing `100%` of `640px` to become `6.4px` instead of `640px`.

**Root Cause:**
The `PixelValue::to_percent()` method returned a plain `f32` that was already normalized (0.0-1.0), but the sizing code treated it as if it still needed division:

```rust
// In pixel.rs
pub fn to_percent(&self) -> Option<f32> {
    match self.metric {
        SizeMetric::Percent => Some(self.number.get() / 100.0),  // First division
        _ => None,
    }
}

// In sizing.rs
Some(p) => (p / 100.0) * containing_block_size.width,  // Second division! BUG!
```

This caused:
- CSS "100%" → FloatValue{100000} → 100.0 (via FloatValue.get())
- First /100 in to_percent(): 100.0 / 100.0 = 1.0 ✓
- **Second /100 in sizing.rs**: 1.0 / 100.0 = 0.01 ❌
- Result: 0.01 * 640 = 6.4 ❌

**Solution:**
Created a type-safe `NormalizedPercentage` wrapper that makes it explicit the value is already in 0.0-1.0 range:

```rust
/// A normalized percentage value (0.0 = 0%, 1.0 = 100%)
#[repr(transparent)]
pub struct NormalizedPercentage(f32);

impl NormalizedPercentage {
    /// Create from unnormalized value (divides by 100 internally)
    pub fn from_unnormalized(value: f32) -> Self {
        Self(value / 100.0)
    }

    /// Resolve against containing block (multiply, don't divide!)
    pub fn resolve(self, containing_block_size: f32) -> f32 {
        self.0 * containing_block_size
    }
}
```

Now `to_percent()` returns `NormalizedPercentage` instead of `f32`:

```rust
pub fn to_percent(&self) -> Option<NormalizedPercentage> {
    match self.metric {
        SizeMetric::Percent => Some(NormalizedPercentage::from_unnormalized(self.number.get())),
        _ => None,
    }
}
```

And the sizing code uses the type-safe `.resolve()` method:

```rust
// BEFORE (WRONG):
Some(p) => (p / 100.0) * containing_block_size.width,

// AFTER (CORRECT):
Some(p) => p.resolve(containing_block_size.width),
```

**Files Changed:**
- `css/src/props/basic/pixel.rs` - Added `NormalizedPercentage` type, updated `to_percent()` and `to_pixels()`
- `layout/src/solver3/sizing.rs` - Updated to use `.resolve()` instead of manual division
- `layout/src/solver3/taffy_bridge.rs` - Updated Taffy conversions to use `.get()` on normalized percentage

**Why This Prevents Future Bugs:**
1. **Type safety**: Can't accidentally divide by 100 again when you have a `NormalizedPercentage`
2. **Self-documenting**: The type name makes it clear it's already normalized
3. **API clarity**: `.resolve()` method makes it obvious what you should do
4. **Compile-time checking**: Using `* 100.0` on a `NormalizedPercentage` would be suspicious

---

## Testing

Both fixes verified working:

1. **Window close**: Now properly sets `close_requested` flag in the correct memory location
2. **Percentage sizing**: 
   ```
   [calculate_used_size_for_node] containing_block_size=640x480
   [calculate_used_size_for_node] css_width=Px(100%), css_height=Px(100%)
   [calculate_used_size_for_node] RESULT: 640x480  ✓
   ```

---

## Remaining Issue

**Positioning Bug**: All display list items are still at position `@ (0, 0)`. This is a separate issue from sizing - the layout is calculating correct sizes but not propagating positions to the display list generation.

**Next Steps**: Debug why `paint_rect` always shows `@ (0, 0)` for all nodes instead of their actual layout positions.

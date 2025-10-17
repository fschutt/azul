# IFrame Layout Behavior

## Special Layout Treatment

IFrame nodes in Azul have **changed default values** for CSS properties that differ from standard HTML elements.

### Changed Default Values

**IFrame nodes have implicit default styles:**
- `display: block` (block-level element)
- `width: 100%` (fills parent width)
- `height: 100%` (fills parent height)

**Body nodes also have special defaults:**
- `width: 100%` (fills viewport width)
- `height: 100%` (fills viewport height)

These are **default values**, not hard-coded behavior. Users can override them with explicit CSS:

```rust
// Override display type
Dom::iframe(data, callback)
    .with_inline_css_props(css_property_vec![
        ("display", "inline-block"),  // Override: use inline-block layout
    ])

// Override size
Dom::iframe(data, callback)
    .with_inline_css_props(css_property_vec![
        ("width", "300px"),   // Override: fixed width
        ("height", "150px"),  // Override: fixed height
    ])
```

### Behavior with Defaults

With default values (`width: 100%; height: 100%`):
- IFrames will **fill their parent container**
- Works naturally in block, flex, and grid layouts
- Provides sensible default sizing without requiring explicit CSS
- Similar to how `<img>` elements work with percentage sizing

### User Control

Users can limit IFrame size using standard CSS properties:
- `max-width`: Limit horizontal expansion
- `max-height`: Limit vertical expansion
- `width`: Set explicit width (overrides flex-grow behavior)
- `height`: Set explicit height (overrides flex-grow behavior)

### Examples

```rust
// IFrame that fills available space
Dom::body()
    .with_child(
        Dom::iframe(data, callback)
        // No CSS needed - will fill parent
    )

// IFrame with maximum size
Dom::body()
    .with_child(
        Dom::iframe(data, callback)
            .with_inline_css_props(css_property_vec![
                ("max-width", "800px"),
                ("max-height", "600px"),
            ])
    )

// IFrame with fixed size
Dom::body()
    .with_child(
        Dom::iframe(data, callback)
            .with_inline_css_props(css_property_vec![
                ("width", "400px"),
                ("height", "300px"),
            ])
    )
```

### Implementation Details

This behavior is implemented in `layout/src/solver3/layout_tree.rs` and `layout/src/solver3/box_props.rs`:

1. **`determine_formatting_context()`** - When encountering `NodeType::IFrame(_)`:
   - Returns `FormattingContext::Flex` as the default
   - Can be overridden if user sets explicit `display` CSS property

2. **`resolve_box_props()`** - When processing IFrame nodes:
   - Sets `flex-grow: 1` as the default value
   - User can override with explicit `flex-grow` CSS property

3. **CSS Property Resolution** - Standard CSS cascade applies:
   - User-specified inline styles override defaults
   - External CSS overrides defaults
   - Default values are only used when no explicit value is provided

### Rationale

This design provides:
- **Sensible defaults**: IFrames fill available space without explicit sizing
- **Easy limiting**: Simple max-width/max-height constraints
- **Full control**: Users can override with any CSS display/flex values
- **Consistency**: Behaves like other replaced elements (images, videos)
- **Flexibility**: Works in any layout context (block, flex, grid parents)

### Browser Comparison

Standard HTML `<iframe>` elements:
- Default: `display: inline` with fixed size 300x150px
- Require explicit sizing for most use cases

Azul's IFrame elements:
- Default: `display: flex; flex-grow: 1` (fills space)
- More intuitive for application UI
- **Can be overridden** to behave like HTML iframes if needed

## Related Files

- `layout/src/solver3/layout_tree.rs` - Layout tree construction and formatting context
- `layout/src/solver3/box_props.rs` - Box model property resolution
- `layout/src/window.rs` - IFrame callback invocation and child DOM layout
- `REFACTORING/IFRAME_AND_SCROLL_ARCHITECTURE.md` - Overall IFrame architecture

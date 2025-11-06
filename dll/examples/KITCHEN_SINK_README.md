# Kitchen Sink Example

A comprehensive showcase of Azul's current layout and rendering capabilities.

## What This Example Demonstrates

This consolidated example shows the current state of Azul's API, specifically:

### ✅ Working Features

- **Grid Layout**: 4-quadrant grid using CSS `display: grid`
- **Flexbox Layout**: Tab bars and flexible containers
- **Text Rendering**: Multiple font sizes, styles (bold, italic), decorations (underline, strikethrough)
- **Colors**: Color boxes, gradients, borders
- **Scrolling**: Overflow containers with many items
- **Contenteditable**: Text input fields with accessibility attributes
- **Menu System**: Menu bar definition (rendering not yet complete)
- **CSS Inline Styles**: All styling via `with_inline_style()`
- **Accessibility**: ARIA labels and attributes

### 🚧 Features Requiring Additional Work

The following features exist in the API but require further integration:

- **Interactive Callbacks**: Mouse/keyboard event handling (API exists in `CallbackInfo`)
- **Text Input Changesets**: New system for handling text input (documented in `CALLBACKINFO_API.md`)
- **Timers**: Background tasks with `Timer` and `TimerCallbackInfo`
- **Threads**: Multi-threaded operations
- **Focus Management**: Keyboard focus control
- **Tooltips**: Hover tooltips
- **Dynamic Updates**: Modifying DOM nodes after initial render

## Building and Running

```bash
# From azul/dll directory
cargo run --bin kitchen_sink --features desktop

# Or from azul root
cd dll && cargo run --bin kitchen_sink --features desktop
```

## File Structure

```
dll/examples/
  └── kitchen_sink.rs  - Main example file (~350 lines)

REFACTORING/todo4/
  ├── CALLBACKINFO_API.md  - Complete API reference for callbacks
  └── kitchen_sink.md      - Original design document
```

## Layout Structure

The example uses a modern grid layout:

```
┌─────────────────────────────────────┐
│         Tab Bar (Flexbox)           │
├──────────────────┬──────────────────┤
│  Text & Fonts    │  Colors & Shapes │
│  - Font sizes    │  - Color grid    │
│  - Styles        │  - Gradients     │
│  - Decorations   │  - Borders       │
├──────────────────┼──────────────────┤
│ Contenteditable  │   Scrolling      │
│  - Single line   │  - 50 items      │
│  - Multi-line    │  - Overflow:auto │
│  - Number input  │                  │
└──────────────────┴──────────────────┘
```

## Next Steps

To enable full interactivity:

1. **Register Callbacks**: Use `with_callbacks()` method (exists but needs proper registration)
2. **Event Processing**: Connect `EventFilter` with DOM nodes
3. **State Management**: Wire up `RefAny` data updates
4. **Text Input**: Integrate the new changeset system
5. **Timers/Threads**: Add background task examples

See `CALLBACKINFO_API.md` for the complete modern API that's available.

## API Migration Status

This example is intentionally simplified to show what works TODAY. For future callback-based interactivity:

- ✅ `CallbackInfo` API is complete and documented
- ✅ Text changeset system exists
- ✅ Timer/Thread infrastructure in place
- 🚧 Event registration needs completion
- 🚧 Callback execution loop needs integration

The groundwork is done - it's now a matter of connecting the pieces!

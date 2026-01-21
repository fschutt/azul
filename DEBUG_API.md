# Azul Debug API

The Azul Debug API provides remote control of Azul 
applications via HTTP for automated end-to-end testing, 
debugging and inspection.

## Quick Start

```bash
# Start your Azul application with the `AZUL_DEBUG` env var
AZUL_DEBUG=8765 ./your_app
# Check that the server is running
curl http://localhost:8765/
# Send commands via POST
curl -X POST http://localhost:8765/ -d '{"op": "click", "selector": ".button"}'
```

GET `/` returns inspection messages while POST `/` executes a command and returns
once the result is finished. Messages are processed sequentially. Internally the trick
is that the HTTP server and the GUI application run in the same process, so they just
share the memory. All inputs and outputs are done via JSON, the Content-Type header 
is optional.

```json
{
  "op": "event_name",
  "param1": "value1",
  "param2": "value2"
}
```

OK response: 

```json
{
  "status": "ok",
  "request_id": 1,
  "window_state": { ... },
  "data": { ... }
}
```

Error response:

```json
{
  "status": "error",
  "request_id": 1,
  "message": "Error description"
}
```

The idea is to be able to use bash scripts or even remote port debugging to
do GUI automation, extract text, input text, automate Azul GUI applications,
as well as to use Bash scripts.

## Events Reference

### Sending mouse input

```bash
# move the mouse cursor to a position
curl -X POST http://localhost:8765/ -d '{"op": "mouse_move", "x": 100, "y": 200}'
# mouse down event (at current cursor location)
curl -X POST http://localhost:8765/ -d '{"op": "mouse_down", "x": 100, "y": 200, "button": "left"}'
# mouse up event (at current cursor location)
curl -X POST http://localhost:8765/ -d '{"op": "mouse_up", "x": 100, "y": 200, "button": "left"}'
# double click
curl -X POST http://localhost:8765/ -d '{"op": "double_click", "x": 100, "y": 200}'
# scroll
curl -X POST http://localhost:8765/ -d '{"op": "scroll", "x": 100, "y": 200, "delta_x": 0, "delta_y": -50}'
```

### Sending clicks

Sending mouse clicks is the most common operation (to click through an application 
in an automated way), which is why the `click` command supports various targeting
methods what to click:

```bash
## Click by CSS selector (recommended)
curl -X POST http://localhost:8765/ -d '{"op": "click", "selector": ".button"}'
## Click by CSS ID
curl -X POST http://localhost:8765/ -d '{"op": "click", "selector": "#submit-btn"}'
## Click by text content
curl -X POST http://localhost:8765/ -d '{"op": "click", "text": "Submit"}'
## Click by node ID
curl -X POST http://localhost:8765/ -d '{"op": "click", "node_id": 42}'
## Click by coordinates
curl -X POST http://localhost:8765/ -d '{"op": "click", "x": 100, "y": 200}'
```

| Parameter | Type | Description |
|-----------|------|-------------|
| `selector` | string? | CSS selector (`.class`, `#id`, `tagname`) |
| `text` | string? | Text content to find |
| `node_id` | number? | Direct node ID |
| `x`, `y` | number? | Screen coordinates |
| `button` | string? | `"left"` (default), `"right"`, `"middle"` |

### Keyboard input

```bash
# text input
curl -X POST http://localhost:8765/ -d '{"op": "text_input", "text": "Hello World"}'
# simulate key down
curl -X POST http://localhost:8765/ -d '{"op": "key_down", "key": "Enter"}'
# simulate key up
curl -X POST http://localhost:8765/ -d '{"op": "key_up", "key": "Enter"}'
# using modifiers
curl -X POST http://localhost:8765/ -d '{"op": "key_down", "key": "a", "modifiers": {"ctrl": true}}'
```

| Parameter | Type | Description |
|-----------|------|-------------|
| `key` | string | Key name (e.g., `"a"`, `"Enter"`, `"Tab"`, `"Escape"`) |
| `modifiers.shift` | bool? | Shift key held |
| `modifiers.ctrl` | bool? | Control key held |
| `modifiers.alt` | bool? | Alt/Option key held |
| `modifiers.meta` | bool? | Meta/Command key held |

### Window Events

```bash
# resize window
curl -X POST http://localhost:8765/ -d '{"op": "resize", "width": 1024, "height": 768}'
# move window
curl -X POST http://localhost:8765/ -d '{"op": "move", "x": 100, "y": 100}'
# focus window
curl -X POST http://localhost:8765/ -d '{"op": "focus"}'
# unfocus window
curl -X POST http://localhost:8765/ -d '{"op": "blur"}'
# close window
curl -X POST http://localhost:8765/ -d '{"op": "close"}'
# simulate dpi change
curl -X POST http://localhost:8765/ -d '{"op": "dpi_changed", "dpi": 192}'
```

### Window inspection

```bash
# get_state - returns window dimensions, DPI, focus state, etc.
curl -X POST http://localhost:8765/ -d '{"op": "get_state"}'
# get_dom - Get the DOM structure
curl -X POST http://localhost:8765/ -d '{"op": "get_dom"}'
# hit_test - Returns the node hierarchy at a point
curl -X POST http://localhost:8765/ -d '{"op": "hit_test", "x": 100, "y": 200}'
# get_logs - Get application logs (startup, debug, layout debug messages)
curl -X POST http://localhost:8765/ -d '{"op": "get_logs"}'
# get_logs since specific request
curl -X POST http://localhost:8765/ -d '{"op": "get_logs", "since_request_id": 5}'
```

### DOM inspection

All node inspection commands support multiple targeting methods: `node_id`, `selector` (CSS), or `text`.

```bash
# get_html_string - Get HTML representation of the DOM (with inlined CSS)
curl -X POST http://localhost:8765/ -d '{"op": "get_html_string"}'
# get_node_css_properties - Get computed CSS properties for a node
curl -X POST http://localhost:8765/ -d '{"op": "get_node_css_properties", "node_id": 42}'
curl -X POST http://localhost:8765/ -d '{"op": "get_node_css_properties", "selector": ".button"}'
# get_node_layout - get layout information (position, size) for a node
curl -X POST http://localhost:8765/ -d '{"op": "get_node_layout", "selector": "#my-element"}'
curl -X POST http://localhost:8765/ -d '{"op": "get_node_layout", "text": "Submit"}'
# get_all_nodes_layout - get layout info for all nodes
curl -X POST http://localhost:8765/ -d '{"op": "get_all_nodes_layout"}'
# get_dom_tree - get detailed DOM tree structure.
curl -X POST http://localhost:8765/ -d '{"op": "get_dom_tree"}'
# get_node_hierarchy - get raw node hierarchy (for debugging).
curl -X POST http://localhost:8765/ -d '{"op": "get_node_hierarchy"}'
# get_layout_tree - get the layout tree structure
curl -X POST http://localhost:8765/ -d '{"op": "get_layout_tree"}'
# get_display_list - Get all display list items with clip/scroll depth info
# IMPORTANT: this is very useful for creating "success conditions" and debugging clipping
curl -X POST http://localhost:8765/ -d '{"op": "get_display_list"}'
```

The `get_display_list` response includes a `clip_analysis` object that shows:
- `final_clip_depth`, `final_scroll_depth`, `final_stacking_depth` - should all be 0 if balanced
- `balanced` - true if all push/pop pairs match
- `operations` - list of all clip/scroll/stacking operations with their depths

Each item in the display list also includes:
- `clip_depth` - current clip nesting level when item is rendered
- `scroll_depth` - current scroll frame nesting level
- `content_size` - for scroll frames, the total scrollable content size
- `scroll_id` - unique identifier for scroll frames

Example response structure:
```json
{
  "data": {
    "value": {
      "items": [
        {"index": 0, "type": "push_stacking_context", "clip_depth": 0, "scroll_depth": 0},
        {"index": 1, "type": "rect", "x": 0, "y": 0, "clip_depth": 0, "scroll_depth": 0},
        {"index": 2, "type": "unknown", "clip_depth": 1, "scroll_depth": 0},
        {"index": 3, "type": "unknown", "clip_depth": 1, "scroll_depth": 1}
      ],
      "clip_analysis": {
        "final_clip_depth": 0,
        "final_scroll_depth": 0,
        "final_stacking_depth": 0,
        "balanced": true,
        "operations": [
          {"index": 2, "op": "PushClip", "clip_depth": 1, "scroll_depth": 0, "bounds": {...}},
          {"index": 3, "op": "PushScrollFrame", "clip_depth": 1, "scroll_depth": 1, "content_size": {...}}
        ]
      }
    }
  }
}
```

### Scroll Inspection

All scroll commands support multiple targeting methods: `node_id`, `selector` (CSS), or `text`.

```bash
# get_scroll_states - get current scroll positions
curl -X POST http://localhost:8765/ -d '{"op": "get_scroll_states"}'
# get_scrollable_nodes - Get all scrollable nodes
curl -X POST http://localhost:8765/ -d '{"op": "get_scrollable_nodes"}'
# get_scrollbar_info - Get detailed scrollbar geometry for a node (track, thumb, buttons)
curl -X POST http://localhost:8765/ -d '{"op": "get_scrollbar_info", "selector": ".scrollable"}'
curl -X POST http://localhost:8765/ -d '{"op": "get_scrollbar_info", "node_id": 42, "orientation": "vertical"}'
# scroll_node_by - Scroll a specific node by a delta amount
curl -X POST http://localhost:8765/ -d '{"op": "scroll_node_by", "node_id": 42, "delta_x": 0, "delta_y": 50}'
curl -X POST http://localhost:8765/ -d '{"op": "scroll_node_by", "selector": ".scrollable", "delta_x": 0, "delta_y": 100}'
# scroll_node_to - Scroll a specific node to an absolute position
curl -X POST http://localhost:8765/ -d '{"op": "scroll_node_to", "node_id": 42, "x": 0, "y": 100}'
curl -X POST http://localhost:8765/ -d '{"op": "scroll_node_to", "selector": ".content", "x": 0, "y": 500}'
```

The `get_scrollbar_info` response includes detailed geometry for automation:

| Field | Description |
|-------|-------------|
| `found` | Whether a scrollbar was found for the node |
| `node_id` | The resolved node ID |
| `orientation` | Requested orientation ("horizontal", "vertical", or "both") |
| `horizontal` / `vertical` | Scrollbar geometry objects (if present) |
| `scroll_x`, `scroll_y` | Current scroll position |
| `max_scroll_x`, `max_scroll_y` | Maximum scroll values |
| `container_rect` | The visible viewport rect |
| `content_rect` | The total scrollable content rect |

Each scrollbar geometry object contains:

| Field | Description |
|-------|-------------|
| `visible` | Whether the scrollbar is visible |
| `track_rect` | Full track rectangle (includes buttons) |
| `track_center` | Center point of the track |
| `button_size` | Size of up/down or left/right buttons |
| `top_button_rect` | Top/left button rectangle |
| `bottom_button_rect` | Bottom/right button rectangle |
| `thumb_rect` | The draggable thumb rectangle |
| `thumb_center` | Center point of the thumb |
| `thumb_position_ratio` | Position ratio (0.0 = start, 1.0 = end) |
| `thumb_size_ratio` | Thumb size relative to track |

### Text Selection Inspection

```bash
# get_selection_state - Get current text selection state (selections, cursor positions, rectangles)
curl -X POST http://localhost:8765/ -d '{"op": "get_selection_state"}'
```

The `get_selection_state` response includes:

| Field | Description |
|-------|-------------|
| `has_selection` | Whether any selection exists |
| `selection_count` | Number of DOMs with active selections |
| `selections` | Array of selection info per DOM |

Each selection info object contains:

| Field | Description |
|-------|-------------|
| `dom_id` | The DOM ID |
| `node_id` | The node containing the selection |
| `ranges` | Array of selection ranges |
| `rectangles` | Visual bounds of each selected region |

Each selection range contains:

| Field | Description |
|-------|-------------|
| `selection_type` | `"cursor"`, `"range"`, or `"block"` |
| `cursor_position` | For cursor: character index |
| `start`, `end` | For range: start and end character indices |
| `direction` | `"forward"` or `"backward"` |

Example response:
```json
{
  "data": {
    "type": "selection_state",
    "value": {
      "has_selection": true,
      "selection_count": 1,
      "selections": [
        {
          "dom_id": 0,
          "node_id": 5,
          "ranges": [
            {
              "selection_type": "range",
              "start": 10,
              "end": 50,
              "direction": "forward"
            }
          ],
          "rectangles": [
            {"x": 20.0, "y": 100.0, "width": 200.0, "height": 24.0},
            {"x": 20.0, "y": 124.0, "width": 150.0, "height": 24.0}
          ]
        }
      ]
    }
  }
}
```

### Finding / debugging nodes

```bash
# find_node_by_text - Find a node by text content
curl -X POST http://localhost:8765/ -d '{"op": "find_node_by_text", "text": "Click me"}'
```

Note: For finding nodes by CSS class/selector, use `get_node_layout` with `selector` parameter instead.

### Testing Utilities

```bash
# relayout - Force a layout recalculation
curl -X POST http://localhost:8765/ -d '{"op": "relayout"}'
# redraw - Force a redraw
curl -X POST http://localhost:8765/ -d '{"op": "redraw"}'
# wait_frame - Wait for the next frame to render
curl -X POST http://localhost:8765/ -d '{"op": "wait_frame"}'
# wait - Wait for a specific duration
curl -X POST http://localhost:8765/ -d '{"op": "wait", "ms": 1000}'
```

### App State (JSON Serialization)

These endpoints allow reading and modifying the application state as JSON at runtime.
This requires the app to use `AZ_REFLECT_JSON` (C) or equivalent to register JSON 
serialization/deserialization functions for the app state type.

```bash
# get_app_state - Get the current app state as JSON with metadata
curl -X POST http://localhost:8765/ -d '{"op": "get_app_state"}'

# set_app_state - Set the app state from JSON (triggers relayout)
curl -X POST http://localhost:8765/ -d '{"op": "set_app_state", "state": {"counter": 42}}'
```

#### GetAppState Response

The `get_app_state` response includes full metadata about the RefAny type:

```json
{
  "status": "ok",
  "request_id": 1,
  "data": {
    "type": "app_state",
    "value": {
      "metadata": {
        "type_id": 4375150160,
        "type_name": "MyDataModel",
        "can_serialize": true,
        "can_deserialize": true,
      },
      "state": {
        "counter": 42.0
      }
    }
  }
}
```

If serialization is not supported, the response includes an error:

```json
{
  "data": {
    "type": "app_state",
    "value": {
      "metadata": { ... },
      "state": null,
      "error": { "error_type": "not_serializable" }
    }
  }
}
```

#### SetAppState Response

```json
{
  "status": "ok",
  "request_id": 2,
  "data": {
    "type": "app_state_set",
    "value": {
      "success": true
    }
  }
}
```

On error:

```json
{
  "data": {
    "type": "app_state_set",
    "value": {
      "success": false,
      "error": { "error_type": "type_construction_error", "message": "Missing field: name" }
    }
  }
}
```

Error types:
- `not_serializable`: Type doesn't have a serialize function registered
- `not_deserializable`: Type doesn't have a deserialize function registered
- `serde_error`: JSON parsing or serialization failed
- `type_construction_error`: Valid JSON but couldn't create the target type

### Screenshots

```bash
# take_screenshot - Capture a screenshot (software rendered), similar to reftest
# Returns base64-encoded PNG in the response
curl -X POST http://localhost:8765/ -d '{"op": "take_screenshot"}'
# take_native_screenshot - Capture using native screenshot APIs, includes window frame
curl -X POST http://localhost:8765/ -d '{"op": "take_native_screenshot"}'
```

## Example

```bash
#!/bin/bash
PORT=8765
API="http://localhost:$PORT"

# Wait for app to start
sleep 1
# Check health
curl -s $API/ | jq .
# Click a button
curl -s -X POST $API/ -d '{"op": "click", "selector": ".submit-button"}'
# Wait for animation
curl -s -X POST $API/ -d '{"op": "wait", "ms": 500}'
# Take screenshot
curl -s -X POST $API/ -d '{"op": "take_screenshot"}' | jq -r '.data.screenshot' | base64 -d > screenshot.png
# Check the DOM
curl -s -X POST $API/ -d '{"op": "get_html_string"}' | jq -r '.data.html'
```

## CSS Selector Support

The `selector` parameter in `click` and related events supports full CSS path matching:

| Pattern | Example | Description |
|---------|---------|-------------|
| `.class` | `.button` | Match by CSS class |
| `#id` | `#submit` | Match by element ID |
| `tagname` | `div` | Match by tag name |
| `>` | `.parent > .child` | Direct child combinator |
| ` ` (space) | `.ancestor .descendant` | Descendant combinator |
| `+` | `.prev + .next` | Adjacent sibling combinator |
| `~` | `.prev ~ .sibling` | General sibling combinator |
| `:first` | `li:first` | First child pseudo-selector |
| `:last` | `li:last` | Last child pseudo-selector |
| `:nth-child()` | `li:nth-child(2n+1)` | Nth-child pseudo-selector |

## Troubleshooting

### Server not responding

- Ensure `AZUL_DEBUG` environment variable is set
- Check the port is not in use
- Application must have an open window

### Events not working

- Use `get_state` to verify window state
- Use `hit_test` to verify node positions
- Check logs with `get_logs`

### Click not triggering callbacks

- Verify the node exists with `find_node_by_class` or `find_node_by_text`
- Ensure the callback is registered on the correct event type
- Use `get_dom_tree` to inspect the DOM structure

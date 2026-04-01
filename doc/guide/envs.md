# Environment Variables

All environment variables use the `AZ_` prefix.

## Variables

| Variable | Values | Description |
|----------|--------|-------------|
| `AZ_BACKEND` | `cpu`, `gpu`, `headless` | Rendering backend. Default: `cpu`. `gpu` forces OpenGL/Metal. `headless` runs without a window (for E2E tests). |
| `AZ_DEBUG` | `<port>` | Start HTTP debug server on the given port. Enables E2E commands (`get_state`, `key_down`, `text_input`, `take_screenshot`, etc.) |
| `AZ_RECORD` | `<filepath>` | Write all debug log messages to a file with microsecond timestamps. Enables verbose logging. |
| `AZ_E2E` | `<filepath.json>` | Run E2E tests from a JSON file. Executes test steps, then exits with pass/fail. |

## Examples

```bash
# Force CPU rendering (default)
./my_app

# Force GPU rendering
AZ_BACKEND=gpu ./my_app

# Run with debug server
AZ_DEBUG=8765 ./my_app

# Record all events to file
AZ_RECORD=/tmp/azul_log.txt ./my_app

# Run E2E tests
AZ_E2E=tests/e2e/widgets_headless_test.json ./my_app

# Headless E2E with debug server
AZ_BACKEND=headless AZ_DEBUG=8765 ./my_app

# Full debug: record + debug server
AZ_DEBUG=8765 AZ_RECORD=/tmp/debug.txt ./my_app
```

## E2E Test JSON Format

```json
[
  {
    "name": "test_click_button",
    "steps": [
      { "op": "resize", "width": 800, "height": 600 },
      { "op": "click", "selector": ".button" },
      { "op": "wait", "ms": 200 },
      { "op": "assert_screenshot", "reference": "baseline.png", "threshold": 3 }
    ]
  }
]
```

The viewport starts at 800x600. Use `resize` in test steps to change it.

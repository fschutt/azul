# Environment Variables

Azul reacts to the following environment variables at runtime.
All use the `AZ_` prefix for consistency.

## Debug & Testing

| Variable | Values | Description |
|----------|--------|-------------|
| `AZ_DEBUG` | `<port>` (e.g. `8765`) | Start HTTP debug server on the given port. Enables E2E test commands (`get_state`, `key_down`, `text_input`, `take_screenshot`, etc.) |
| `AZ_RECORD` | `<filepath>` | Write all debug log messages to the given file with microsecond timestamps. Enables verbose logging across all subsystems. |
| `AZ_E2E` | `<filepath.json>` | Run E2E tests from a JSON file. The file contains test steps (click, type, assert_screenshot, etc.). The app runs headless or windowed, executes the steps, and exits with pass/fail. |

## Rendering

| Variable | Values | Description |
|----------|--------|-------------|
| `AZ_BACKEND` | `auto`, `gpu`, `cpu`, `headless` | Select rendering backend. Default: `cpu`. `gpu` forces OpenGL/Metal. `headless` runs without a native window (for E2E tests). |
| `AZ_COMPOSITOR` | `auto`, `gpu`, `cpu` | Override compositor mode selection. Usually auto-detected from `AZ_BACKEND`. |

## Headless Mode

Used when `AZ_BACKEND=headless`. These set the initial viewport for the
headless window. After startup, the viewport can be changed via the debug
server's `resize` command.

| Variable | Values | Default | Description |
|----------|--------|---------|-------------|
| `AZ_HEADLESS_WIDTH` | integer (px) | `800` | Initial headless viewport width |
| `AZ_HEADLESS_HEIGHT` | integer (px) | `600` | Initial headless viewport height |
| `AZ_HEADLESS_DPI` | float | `1.0` | Headless HiDPI scale factor |
| `AZ_HEADLESS_RENDER` | `true`/`false` | `true` | Enable CPU rendering in headless mode |
| `AZ_HEADLESS_MAX_ITER` | integer | `1000` | Max event loop iterations before auto-exit |

## Examples

```bash
# Run with debug server on port 8765
AZ_DEBUG=8765 ./my_app

# Record all events to file for debugging
AZ_RECORD=/tmp/azul_log.txt ./my_app

# Force CPU rendering (default)
AZ_BACKEND=cpu ./my_app

# Force GPU rendering
AZ_BACKEND=gpu ./my_app

# Run E2E tests from JSON file
AZ_E2E=tests/e2e/widgets_headless_test.json ./my_app

# Headless E2E testing with debug server
AZ_BACKEND=headless AZ_DEBUG=8765 ./my_app

# Custom headless viewport
AZ_BACKEND=headless AZ_HEADLESS_WIDTH=1024 AZ_HEADLESS_HEIGHT=768 ./my_app

# Full debug: record + debug server
AZ_DEBUG=8765 AZ_RECORD=/tmp/debug.txt ./my_app
```

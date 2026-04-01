# Environment Variables

Azul reacts to the following environment variables at runtime.
All use the `AZ_` prefix for consistency.

## Debug & Logging

| Variable | Values | Description |
|----------|--------|-------------|
| `AZ_DEBUG` | `<port>` (e.g. `8765`) | Start HTTP debug server on the given port. Enables E2E test commands (get_state, key_down, text_input, take_screenshot, etc.) |
| `AZ_RECORD` | `<filepath>` | Write all debug log messages to the given file with microsecond timestamps. Enables verbose logging across all subsystems. |

## Rendering

| Variable | Values | Description |
|----------|--------|-------------|
| `AZ_BACKEND` | `auto`, `gpu`, `cpu`, `headless` | Select rendering backend. `auto` (default) tries GPU, falls back to CPU. `gpu` forces OpenGL/Metal. `cpu` forces software rendering. `headless` runs without a native window (for E2E tests). |
| `AZ_COMPOSITOR` | `auto`, `gpu`, `cpu` | Override compositor mode selection. Usually auto-detected from `AZ_BACKEND`. |

## Headless Mode

Used when `AZ_BACKEND=headless` for automated testing without a window.

| Variable | Values | Default | Description |
|----------|--------|---------|-------------|
| `AZ_HEADLESS_WIDTH` | integer (px) | `800` | Headless viewport width |
| `AZ_HEADLESS_HEIGHT` | integer (px) | `600` | Headless viewport height |
| `AZ_HEADLESS_DPI` | float | `1.0` | Headless HiDPI scale factor |
| `AZ_HEADLESS_RENDER` | `true`/`false` | `true` | Whether to actually render (CPU) in headless mode |
| `AZ_HEADLESS_MAX_ITER` | integer | `1000` | Max event loop iterations before auto-exit |

## Examples

```bash
# Run with debug server
AZ_DEBUG=8765 ./my_app

# Record all events to file
AZ_RECORD=/tmp/azul_log.txt ./my_app

# Force CPU rendering
AZ_BACKEND=cpu ./my_app

# Headless testing
AZ_BACKEND=headless AZ_DEBUG=8765 ./my_app

# Custom headless viewport
AZ_BACKEND=headless AZ_HEADLESS_WIDTH=1024 AZ_HEADLESS_HEIGHT=768 AZ_HEADLESS_DPI=2.0 ./my_app
```

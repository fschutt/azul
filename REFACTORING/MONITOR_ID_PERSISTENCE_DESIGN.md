# Monitor ID Persistence Design

## Overview

The `MonitorId` structure supports both fast runtime lookup and persistent monitor identification across app restarts and monitor configuration changes.

## Structure

```rust
pub struct MonitorId {
    /// Runtime index of the monitor (may change between sessions)
    pub index: usize,
    /// Stable hash of monitor properties (for persistence)
    pub hash: u64,
}
```

## Design Rationale

### Two-Field Approach

- **`index`**: Fast O(1) lookup during runtime
  - 0-based array index into current monitor list
  - May change if monitors are added/removed/reordered
  - Used for immediate operations like `get_monitors()[index]`

- **`hash`**: Stable identifier for persistence
  - FNV-1a hash of: monitor name + position + size
  - Remains constant unless physical monitor properties change
  - Used for saving/restoring window positions across sessions

### Why Not Single-Field?

**Option 1: Index-only** ❌
- Breaks when monitors added/removed
- Window opens on wrong monitor after config change

**Option 2: Hash-only** ❌
- Requires O(n) search on every monitor query
- No fast path for common operations

**Option 3: Index + Hash** ✅
- Fast runtime lookup (index)
- Stable persistence (hash)
- Application can intelligently handle both

## Usage Patterns

### Runtime Operations (Fast Path)

```rust
// Get current monitor (uses index for O(1) lookup)
let monitor_id = window.get_current_monitor_id();
let monitors = get_monitors();
let current = &monitors[monitor_id.index];
```

### Persistence (Stable Path)

```rust
// Save window state on close
fn on_window_close(data: &mut AppData, info: &mut CallbackInfo) -> Update {
    let monitor_id = info.current_window_state.monitor.id;
    
    // Serialize both index and hash
    let config = WindowConfig {
        monitor_hash: monitor_id.hash,
        position: info.current_window_state.position,
        size: info.current_window_state.size,
    };
    config.save_to_file("window_state.json");
    
    Update::DoNothing
}

// Restore window state on launch
fn restore_window_position(options: &mut WindowCreateOptions) {
    if let Some(config) = WindowConfig::load_from_file("window_state.json") {
        let monitors = get_monitors();
        
        // Strategy 1: Try to find monitor by hash (best match)
        if let Some((index, _)) = monitors.iter().enumerate()
            .find(|(_, m)| m.id.hash == config.monitor_hash) 
        {
            options.state.monitor = Some(MonitorId::from_index_and_hash(index, config.monitor_hash));
            options.state.position = config.position;
            options.state.size = config.size;
            return;
        }
        
        // Strategy 2: Fallback to primary monitor
        options.state.monitor = Some(MonitorId::PRIMARY);
    }
}
```

## Monitor Placement API

### Current Limitations (Linux Wayland)

Wayland **does not support** programmatic window positioning on specific monitors:
- Compositor controls all window placement
- No API to request "place window on monitor X"
- Applications can suggest size/state but not absolute position

### Workaround Strategy

1. **Window creation**: App requests monitor via `WindowCreateOptions.state.monitor`
2. **Framework detection**: After window mapped, framework detects actual monitor
3. **Callback notification**: `current_window_state.monitor` updated
4. **Application reaction**: App can detect mismatch and:
   - Show notification: "Window opened on different monitor"
   - Provide UI to move window manually
   - Save actual monitor for next time

### Cross-Platform Support

| Platform | Monitor Placement | Implementation |
|----------|------------------|----------------|
| **Wayland** | ❌ Not supported | Compositor decides |
| **X11** | ✅ Supported | `XMoveWindow` to monitor position |
| **Windows** | ✅ Supported | `SetWindowPos` with monitor bounds |
| **macOS** | ✅ Supported | `NSWindow.setFrame` with screen rect |

## Implementation Details

### Hash Algorithm (FNV-1a)

```rust
pub fn from_properties(index: usize, name: &str, position: LayoutPoint, size: LayoutSize) -> Self {
    // FNV-1a hash of:
    // - Monitor name (e.g., "DP-1", "HDMI-0")
    // - Position (x, y)
    // - Size (width, height)
    
    let mut hasher = FnvHasher(0xcbf29ce484222325);
    name.hash(&mut hasher);
    position.x.hash(&mut hasher);
    position.y.hash(&mut hasher);
    size.width.hash(&mut hasher);
    size.height.hash(&mut hasher);
    
    Self {
        index,
        hash: hasher.finish()
    }
}
```

### Hash Stability

**Hash changes when:**
- Monitor name changes (e.g., connector renamed)
- Monitor resolution changes
- Monitor position in layout changes

**Hash remains stable when:**
- Monitors reordered in OS settings (index changes, hash doesn't)
- Monitor disconnected/reconnected with same config
- App restarted

### Wayland Event Tracking

```rust
// wl_output events update MonitorState
extern "C" fn wl_output_geometry_handler(...) {
    monitor_state.x = x;
    monitor_state.y = y;
    monitor_state.make = make;   // For better hash stability
    monitor_state.model = model;
}

extern "C" fn wl_output_mode_handler(...) {
    monitor_state.width = width;
    monitor_state.height = height;
}

// wl_surface events track window location
extern "C" fn wl_surface_enter_handler(..., output: *mut wl_output) {
    window.current_outputs.push(output);
    // Update current_window_state.monitor
}

extern "C" fn wl_surface_leave_handler(..., output: *mut wl_output) {
    window.current_outputs.retain(|&o| o != output);
    // Update current_window_state.monitor
}
```

## Application Responsibilities

The framework provides:
- ✅ Stable monitor identification (`hash`)
- ✅ Fast monitor lookup (`index`)
- ✅ Current monitor detection (event-based)

The application must handle:
- 📝 Saving `monitor.hash` to config file
- 🔍 Searching monitors by hash on restore
- ⚠️ Handling monitor not found (fallback logic)
- 🖱️ Detecting monitor changes in callbacks
- 🚪 Graceful degradation on Wayland (no forced placement)

## Example: Multi-Window Multi-Monitor App

```rust
struct AppData {
    window_configs: HashMap<String, WindowConfig>,
}

#[derive(Serialize, Deserialize)]
struct WindowConfig {
    monitor_hash: u64,
    position: WindowPosition,
    size: WindowSize,
}

fn on_window_close(data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    let app_data = data.downcast_mut::<AppData>()?;
    let window_id = info.get_window_id();
    
    // Save current monitor hash
    app_data.window_configs.insert(
        window_id.to_string(),
        WindowConfig {
            monitor_hash: info.current_window_state.monitor.id.hash,
            position: info.current_window_state.position,
            size: info.current_window_state.size,
        }
    );
    
    // Persist to disk
    save_configs(&app_data.window_configs);
    
    Update::DoNothing
}

fn create_window_with_persistence(app: &mut App, window_id: &str) {
    let mut options = WindowCreateOptions::default();
    
    // Try to restore previous monitor
    if let Some(config) = load_config(window_id) {
        if let Some(monitor) = find_monitor_by_hash(config.monitor_hash) {
            options.state.monitor = Some(monitor.id);
            options.state.position = config.position;
            options.state.size = config.size;
        }
    }
    
    app.add_window(options);
}

fn find_monitor_by_hash(hash: u64) -> Option<Monitor> {
    get_monitors()
        .into_iter()
        .find(|m| m.id.hash == hash)
}
```

## Future Enhancements

### Planned Features

- [ ] `WindowCreateOptions.monitor_placement_strategy` enum:
  - `PreferHash`: Try hash, fallback to primary
  - `RequireHash`: Only open if hash found, else error
  - `RequireIndex`: Must have monitor at index, else error
  - `Primary`: Always use primary (current default)

- [ ] `get_monitor_by_hash(hash: u64) -> Option<Monitor>`
  - Helper function for common lookup pattern

- [ ] `Monitor.confidence_score() -> f32`
  - How likely this is the "same" monitor (0.0 - 1.0)
  - Based on EDID data, serial numbers, etc.

### Platform-Specific Improvements

**X11/XRandR**:
- Use EDID data in hash (more stable than position)
- Support monitor hot-plug events

**Windows**:
- Use display device ID in hash
- Handle monitor DPI changes

**macOS**:
- Use `CGDisplaySerialNumber` in hash
- Handle display rotation events

## Testing Strategy

1. **Basic persistence**: Save window, restart, verify same monitor
2. **Monitor addition**: Add monitor, verify indices shift but hashes stable
3. **Monitor removal**: Remove monitor, verify fallback to primary
4. **Resolution change**: Change resolution, verify hash changes (expected)
5. **Position change**: Move monitor in layout, verify hash changes (expected)
6. **Reconnection**: Disconnect/reconnect, verify hash stable

## References

- Wayland protocol: `wl_output` and `wl_surface` interfaces
- FNV-1a hash: http://www.isthe.com/chongo/tech/comp/fnv/
- XRandR: https://www.x.org/wiki/libraries/libxrandr/

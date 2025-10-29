# CSD and Fallback Menu System Design

**Date**: 2025-10-28  
**Status**: 🔨 IMPLEMENTATION IN PROGRESS

## Overview

This document describes a cross-platform Client-Side Decoration (CSD) and fallback menu system that works consistently across all platforms (macOS, Windows, Linux X11, Linux Wayland).

### Goals

1. **Unified Menu System**: Provide consistent menu behavior across all platforms
2. **Client-Side Decorations**: Optional custom window decorations (titlebar, buttons)
3. **Fallback Menus**: Popup menu windows when native menus are unavailable
4. **Cross-Platform**: Single API works identically on all 4 platforms

### Key Principles

- **Flag-Driven**: CSD/menu behavior controlled by `WindowState` flags
- **Window-Based Menus**: Menu popups are separate "always-on-top" windows
- **Automatic Focus Management**: Menus close when focus is lost
- **Position-Aware**: Menu windows positioned based on LayoutWindow hit-test data

## Architecture Components

### 1. Window State Flags

```rust
// In azul-core/src/window.rs
pub struct WindowFlags {
    // Existing flags...
    
    /// Enable client-side decorations (custom titlebar)
    pub has_decorations: bool,
    
    /// Window type for classification
    pub window_type: WindowType,
    
    /// Track if window currently has focus
    pub has_focus: bool,
    
    /// Close was requested (set by callback, processed by shell)
    pub close_requested: bool,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum WindowType {
    /// Normal application window
    Normal,
    
    /// Menu popup window (always-on-top, frameless)
    Menu,
    
    /// Tooltip window (always-on-top, no interaction)
    Tooltip,
    
    /// Dialog window (blocks parent)
    Dialog,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum WindowFrame {
    /// Normal windowed mode
    Normal,
    
    /// Maximized window
    Maximized,
    
    /// Minimized window
    Minimized,
    
    /// Fullscreen window
    Fullscreen,
    
    /// Frameless window (no OS decorations)
    Frameless,
}
```

### 2. Menu Injection API

Each platform implements a unified menu injection function:

```rust
// In dll/src/desktop/shell2/{platform}/mod.rs

/// Inject a menu bar into the window
///
/// This function:
/// - Clones the menu structure from WindowState
/// - Creates platform-specific menu representation
/// - On Linux/fallback: prepares for popup window creation
/// - On macOS/Windows: uses native menu APIs
pub fn inject_menu_bar(
    window: &mut PlatformWindow,
    menu_bar: &azul_core::menu::MenuBar,
) -> Result<(), String>
```

**Platform-Specific Behavior**:

- **macOS**: Creates native `NSMenu` hierarchy attached to window
- **Windows**: Creates native `HMENU` hierarchy attached to window
- **Linux X11**: Stores menu structure for fallback popup window creation
- **Linux Wayland**: Stores menu structure for fallback popup window creation

### 3. CSD Titlebar Handling

When `WindowFlags::has_decorations == true && WindowFrame::Frameless`:

```rust
// Pseudo-code for CSD titlebar injection

if window.state.flags.has_decorations && window.state.frame == WindowFrame::Frameless {
    // 1. Clone menu bar from WindowState
    let menu_bar = window.state.menu_bar.clone();
    
    // 2. Create CSD titlebar DOM nodes
    let titlebar_dom = create_csd_titlebar(
        &window.state.title,
        menu_bar,
        has_close_button: true,
        has_minimize_button: true,
        has_maximize_button: true,
    );
    
    // 3. Inject titlebar into user's DOM (prepend to root)
    let mut user_dom = call_layout_callback(app_data, callback_info);
    user_dom.prepend_child(titlebar_dom);
    
    // 4. Attach built-in callbacks to titlebar buttons
    attach_titlebar_callbacks(&mut user_dom);
}
```

**CSD Titlebar Callbacks**:

```rust
// Built-in callbacks for titlebar buttons

extern "C" fn on_titlebar_close(data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    // Set close_requested flag (processed by shell after callback returns)
    info.window_state_mut().flags.close_requested = true;
    Update::DoNothing
}

extern "C" fn on_titlebar_maximize(data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    let state = info.window_state_mut();
    state.frame = match state.frame {
        WindowFrame::Normal => WindowFrame::Maximized,
        WindowFrame::Maximized => WindowFrame::Normal,
        _ => state.frame,
    };
    Update::RefreshDom
}

extern "C" fn on_titlebar_minimize(data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    info.window_state_mut().frame = WindowFrame::Minimized;
    Update::DoNothing
}
```

### 4. Fallback Menu System (Linux Primary Target)

#### 4.1. Menu Window Creation

When a menu item is clicked and native menus are unavailable:

```rust
// In dll/src/desktop/shell2/linux/menu.rs (new module)

pub struct MenuWindow {
    /// The X11/Wayland window for this menu popup
    pub window: PlatformWindow,
    
    /// Parent menu window (if this is a submenu)
    pub parent: Option<WindowId>,
    
    /// Child menu windows (submenus)
    pub children: Vec<WindowId>,
    
    /// Position where menu was spawned
    pub spawn_position: LogicalPosition,
    
    /// Time when menu was last interacted with
    pub last_interaction: Instant,
}

/// Create a menu popup window at the given position
pub fn create_menu_window(
    menu_items: Vec<MenuItem>,
    position: LogicalPosition,
    parent_window_id: Option<WindowId>,
    fc_cache: Arc<FcFontCache>,
    app_data: Arc<RefCell<RefAny>>,
) -> Result<MenuWindow, String> {
    let options = WindowCreateOptions {
        state: FullWindowState {
            flags: WindowFlags {
                window_type: WindowType::Menu,
                has_decorations: false,
                is_always_on_top: true,
                has_focus: false,
                ..Default::default()
            },
            frame: WindowFrame::Frameless,
            position: Some(position),
            size: calculate_menu_size(&menu_items),
            ..Default::default()
        },
        ..Default::default()
    };
    
    // Create window with menu DOM
    let window = PlatformWindow::new(options, fc_cache, app_data)?;
    
    Ok(MenuWindow {
        window,
        parent: parent_window_id,
        children: Vec::new(),
        spawn_position: position,
        last_interaction: Instant::now(),
    })
}
```

#### 4.2. Menu DOM Generation

```rust
// Generate DOM for menu items

fn create_menu_dom(items: Vec<MenuItem>) -> StyledDom {
    let mut children = Vec::new();
    
    for item in items {
        let item_dom = match item {
            MenuItem::Action { label, callback, .. } => {
                Dom::div()
                    .with_class("menu-item")
                    .with_child(Dom::text(label))
                    .with_callback(On::MouseDown, callback)
            }
            MenuItem::Submenu { label, items, .. } => {
                Dom::div()
                    .with_class("menu-item menu-submenu")
                    .with_child(Dom::text(label))
                    .with_child(Dom::text("▶").with_class("submenu-arrow"))
                    .with_callback(On::MouseEnter, on_submenu_hover)
            }
            MenuItem::Separator => {
                Dom::div().with_class("menu-separator")
            }
        };
        children.push(item_dom);
    }
    
    Dom::div()
        .with_class("menu-popup")
        .with_children(children)
        .style(create_menu_css())
}

fn create_menu_css() -> Css {
    Css::from_str(r#"
        .menu-popup {
            background: #ffffff;
            border: 1px solid #cccccc;
            box-shadow: 0 2px 8px rgba(0,0,0,0.15);
            padding: 4px 0;
        }
        
        .menu-item {
            padding: 6px 32px 6px 12px;
            cursor: pointer;
            position: relative;
        }
        
        .menu-item:hover {
            background: #0078d4;
            color: #ffffff;
        }
        
        .menu-separator {
            height: 1px;
            background: #e0e0e0;
            margin: 4px 0;
        }
        
        .submenu-arrow {
            position: absolute;
            right: 8px;
        }
    "#)
}
```

#### 4.3. Menu Position Calculation

```rust
/// Calculate menu spawn position from LayoutWindow hit-test
pub fn calculate_menu_position(
    layout_window: &LayoutWindow,
    clicked_node: HitTestNode,
    menu_size: LogicalSize,
    screen_size: LogicalSize,
) -> LogicalPosition {
    // Get absolute position of clicked node from layout tree
    let node_rect = layout_window
        .get_node_absolute_rect(clicked_node.dom_id, clicked_node.node_id)
        .unwrap_or(LogicalRect::zero());
    
    // Position menu below the clicked item
    let mut x = node_rect.origin.x;
    let mut y = node_rect.origin.y + node_rect.size.height;
    
    // Adjust if menu would go off-screen (right edge)
    if x + menu_size.width > screen_size.width {
        x = screen_size.width - menu_size.width;
    }
    
    // Adjust if menu would go off-screen (bottom edge)
    if y + menu_size.height > screen_size.height {
        // Position above instead of below
        y = node_rect.origin.y - menu_size.height;
    }
    
    LogicalPosition::new(x.max(0.0), y.max(0.0))
}
```

#### 4.4. Submenu Positioning

```rust
/// Calculate submenu position (appears to the right of parent item)
pub fn calculate_submenu_position(
    parent_menu_rect: LogicalRect,
    parent_item_rect: LogicalRect,
    submenu_size: LogicalSize,
    screen_size: LogicalSize,
) -> LogicalPosition {
    // Default: right side of parent menu, aligned with hovered item
    let mut x = parent_menu_rect.origin.x + parent_menu_rect.size.width;
    let mut y = parent_item_rect.origin.y;
    
    // Adjust if submenu would go off-screen (right edge)
    if x + submenu_size.width > screen_size.width {
        // Position on left side of parent instead
        x = parent_menu_rect.origin.x - submenu_size.width;
    }
    
    // Adjust if submenu would go off-screen (bottom edge)
    if y + submenu_size.height > screen_size.height {
        y = screen_size.height - submenu_size.height;
    }
    
    LogicalPosition::new(x.max(0.0), y.max(0.0))
}
```

### 5. Menu Focus Management

#### 5.1. Focus Tracking

```rust
// In dll/src/desktop/shell2/linux/menu.rs

pub struct MenuChain {
    /// All open menu windows in order (root -> leaf)
    pub windows: Vec<WindowId>,
    
    /// Track which window currently has focus
    pub focused_window: Option<WindowId>,
    
    /// Track mouse position for hover detection
    pub mouse_position: LogicalPosition,
    
    /// Time when mouse last left all menu windows
    pub mouse_left_at: Option<Instant>,
}

impl MenuChain {
    /// Check if any menu window has focus
    pub fn has_focus(&self, windows: &HashMap<WindowId, MenuWindow>) -> bool {
        self.windows.iter().any(|id| {
            windows.get(id)
                .map(|w| w.window.current_window_state.flags.has_focus)
                .unwrap_or(false)
        })
    }
    
    /// Check if mouse is inside any menu window
    pub fn mouse_inside(&self, windows: &HashMap<WindowId, MenuWindow>) -> bool {
        self.windows.iter().any(|id| {
            windows.get(id)
                .map(|w| w.contains_point(self.mouse_position))
                .unwrap_or(false)
        })
    }
    
    /// Determine if menu chain should close
    pub fn should_close(&self, windows: &HashMap<WindowId, MenuWindow>) -> bool {
        // Close if:
        // 1. No menu has focus AND
        // 2. Mouse has been outside all menus for > 100ms
        
        if self.has_focus(windows) {
            return false;
        }
        
        if self.mouse_inside(windows) {
            return false;
        }
        
        if let Some(left_at) = self.mouse_left_at {
            left_at.elapsed() > Duration::from_millis(100)
        } else {
            false
        }
    }
}
```

#### 5.2. Automatic Menu Closing

```rust
// In main event loop (per platform)

pub fn process_menu_windows(
    menu_chain: &mut MenuChain,
    menu_windows: &mut HashMap<WindowId, MenuWindow>,
) {
    // Update mouse position from OS
    if let Some(mouse_pos) = get_current_mouse_position() {
        let was_inside = menu_chain.mouse_inside(menu_windows);
        menu_chain.mouse_position = mouse_pos;
        let now_inside = menu_chain.mouse_inside(menu_windows);
        
        // Track when mouse leaves all menu windows
        if was_inside && !now_inside {
            menu_chain.mouse_left_at = Some(Instant::now());
        } else if now_inside {
            menu_chain.mouse_left_at = None;
        }
    }
    
    // Check if menu chain should close
    if menu_chain.should_close(menu_windows) {
        close_menu_chain(menu_chain, menu_windows);
    }
}

fn close_menu_chain(
    menu_chain: &mut MenuChain,
    menu_windows: &mut HashMap<WindowId, MenuWindow>,
) {
    // Close all menu windows in reverse order (leaf -> root)
    for window_id in menu_chain.windows.iter().rev() {
        if let Some(menu_window) = menu_windows.remove(window_id) {
            menu_window.window.close();
        }
    }
    
    menu_chain.windows.clear();
    menu_chain.focused_window = None;
}
```

### 6. Window Type Query API

```rust
// In dll/src/desktop/shell2/mod.rs (common across platforms)

impl PlatformWindow {
    /// Check if window is a menu popup
    pub fn is_menu_window(&self) -> bool {
        self.current_window_state.flags.window_type == WindowType::Menu
    }
    
    /// Check if window currently has focus
    pub fn has_focus(&self) -> bool {
        self.current_window_state.flags.has_focus
    }
    
    /// Check if close was requested via callback
    pub fn close_requested(&self) -> bool {
        self.current_window_state.flags.close_requested
    }
    
    /// Get all windows of a specific type
    pub fn get_windows_by_type(
        windows: &HashMap<WindowId, PlatformWindow>,
        window_type: WindowType,
    ) -> Vec<WindowId> {
        windows
            .iter()
            .filter(|(_, w)| w.current_window_state.flags.window_type == window_type)
            .map(|(id, _)| *id)
            .collect()
    }
}
```

### 7. Post-Callback Window State Processing

After each callback invocation, check for state changes:

```rust
// In dll/src/desktop/shell2/{platform}/process.rs

pub fn process_window_state_changes(
    window: &mut PlatformWindow,
) -> ProcessEventResult {
    let mut result = ProcessEventResult::DoNothing;
    
    // Check for close request
    if window.close_requested() {
        window.close();
        return ProcessEventResult::WindowClosed;
    }
    
    // Check for frame changes (minimize/maximize)
    let target_frame = window.current_window_state.frame;
    let current_frame = window.get_actual_frame_state();
    
    if target_frame != current_frame {
        match target_frame {
            WindowFrame::Maximized => window.maximize(),
            WindowFrame::Minimized => window.minimize(),
            WindowFrame::Normal => window.restore(),
            WindowFrame::Fullscreen => window.set_fullscreen(true),
            WindowFrame::Frameless => window.set_decorations(false),
        }
        result = ProcessEventResult::ShouldReRender;
    }
    
    result
}
```

## Implementation Plan

### Phase 1: Core Infrastructure (Week 1)

1. **Add WindowFlags and WindowType**
   - [ ] Extend `WindowFlags` in `azul-core/src/window.rs`
   - [ ] Add `WindowType` enum
   - [ ] Add `WindowFrame` enum (if not exists)
   - [ ] Update `FullWindowState` to use new types

2. **Implement Menu Injection API Stubs**
   - [ ] Add `inject_menu_bar()` to macOS window
   - [ ] Add `inject_menu_bar()` to Windows window
   - [ ] Add `inject_menu_bar()` to Linux X11 window
   - [ ] Add `inject_menu_bar()` to Linux Wayland window

3. **Add Window Query APIs**
   - [ ] Implement `is_menu_window()`
   - [ ] Implement `has_focus()`
   - [ ] Implement `close_requested()`
   - [ ] Implement `get_windows_by_type()`

### Phase 2: CSD Titlebar (Week 2)

4. **Create CSD Titlebar Module**
   - [ ] Create `dll/src/desktop/csd.rs`
   - [ ] Implement `create_csd_titlebar()`
   - [ ] Implement titlebar callbacks (close, minimize, maximize)
   - [ ] Add default CSS for titlebar styling

5. **Integrate CSD into Layout Pipeline**
   - [ ] Detect `has_decorations` flag in layout callback
   - [ ] Inject titlebar DOM before user DOM
   - [ ] Attach callbacks to titlebar buttons
   - [ ] Test on all platforms

### Phase 3: Linux Fallback Menus (Week 3-4)

6. **Create Menu Window Module**
   - [ ] Create `dll/src/desktop/shell2/linux/menu.rs`
   - [ ] Implement `MenuWindow` struct
   - [ ] Implement `create_menu_window()`
   - [ ] Implement `create_menu_dom()`

7. **Position Calculation**
   - [ ] Implement `calculate_menu_position()`
   - [ ] Implement `calculate_submenu_position()`
   - [ ] Add screen bounds checking
   - [ ] Test multi-monitor scenarios

8. **Focus Management**
   - [ ] Implement `MenuChain` struct
   - [ ] Add focus tracking to event loop
   - [ ] Implement automatic menu closing logic
   - [ ] Handle mouse leave detection

9. **Integration with Window Registry**
   - [ ] Store menu windows in global registry
   - [ ] Implement menu chain management
   - [ ] Add cleanup on window close
   - [ ] Test nested submenu scenarios

### Phase 4: Platform-Specific Menu Injection (Week 5)

10. **macOS Native Menus**
    - [ ] Implement `inject_menu_bar()` with NSMenu
    - [ ] Convert MenuItem to NSMenuItem
    - [ ] Handle submenu hierarchy
    - [ ] Test with native menu bar

11. **Windows Native Menus**
    - [ ] Implement `inject_menu_bar()` with HMENU
    - [ ] Convert MenuItem to Windows menu structure
    - [ ] Handle submenu hierarchy
    - [ ] Test with Win32 menu bar

12. **Linux Fallback Flag**
    - [ ] Add runtime detection for native menu support
    - [ ] Use native if available, fallback to popup otherwise
    - [ ] Document platform-specific behavior

### Phase 5: Testing & Polish (Week 6)

13. **Comprehensive Testing**
    - [ ] Test CSD on frameless windows
    - [ ] Test menu popups on all platforms
    - [ ] Test nested submenus (3+ levels)
    - [ ] Test menu closing behavior
    - [ ] Test focus management edge cases

14. **Documentation**
    - [ ] Write user-facing API guide
    - [ ] Add examples for CSD windows
    - [ ] Add examples for custom menus
    - [ ] Document platform differences

15. **Performance Optimization**
    - [ ] Profile menu window creation
    - [ ] Optimize DOM generation for large menus
    - [ ] Cache menu layouts where possible
    - [ ] Minimize layout recalculations

## API Examples

### Example 1: Enable CSD Titlebar

```rust
use azul::prelude::*;

fn main() {
    let app = App::new(RefAny::new(MyData::new()), AppConfig::default());
    
    let window_options = WindowCreateOptions {
        state: FullWindowState {
            flags: WindowFlags {
                has_decorations: true,  // Enable CSD titlebar
                ..Default::default()
            },
            frame: WindowFrame::Frameless,  // Remove OS decorations
            ..Default::default()
        },
        ..Default::default()
    };
    
    app.create_window(window_options, layout).run();
}

fn layout(data: &mut RefAny, info: &mut LayoutCallbackInfo) -> StyledDom {
    // CSD titlebar will be automatically prepended to your DOM
    Dom::body()
        .with_child(Dom::text("Hello, CSD!"))
        .style(Css::empty())
}
```

### Example 2: Custom Menu Bar

```rust
use azul::prelude::*;

fn layout(data: &mut RefAny, info: &mut LayoutCallbackInfo) -> StyledDom {
    let menu_bar = MenuBar::new(vec![
        Menu::new("File", vec![
            MenuItem::action("New", on_new),
            MenuItem::action("Open", on_open),
            MenuItem::separator(),
            MenuItem::action("Exit", on_exit),
        ]),
        Menu::new("Edit", vec![
            MenuItem::action("Cut", on_cut),
            MenuItem::action("Copy", on_copy),
            MenuItem::action("Paste", on_paste),
        ]),
    ]);
    
    // Inject menu bar (uses native on macOS/Windows, fallback on Linux)
    inject_menu_bar(info, &menu_bar)?;
    
    Dom::body()
        .with_child(Dom::text("Application content"))
        .style(Css::empty())
}
```

### Example 3: Manual Menu Popup

```rust
// Trigger menu popup from button click
extern "C" fn on_button_click(data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    let menu_items = vec![
        MenuItem::action("Option 1", on_option_1),
        MenuItem::action("Option 2", on_option_2),
        MenuItem::submenu("More Options", vec![
            MenuItem::action("Sub 1", on_sub_1),
            MenuItem::action("Sub 2", on_sub_2),
        ]),
    ];
    
    // Create menu popup at button position
    spawn_menu_popup(info, menu_items)?;
    
    Update::DoNothing
}
```

## Technical Challenges

### Challenge 1: Menu Window Ownership

**Problem**: Menu windows need to exist independently but be tied to parent window lifecycle.

**Solution**: Use global window registry with parent tracking:
```rust
pub struct WindowRegistry {
    windows: HashMap<WindowId, PlatformWindow>,
    menu_chains: HashMap<WindowId, MenuChain>,  // parent_id -> menu chain
}
```

### Challenge 2: Cross-Platform Menu Positioning

**Problem**: Different coordinate systems and DPI scaling on each platform.

**Solution**: Use `LogicalPosition` consistently, convert to physical only at OS boundary:
```rust
let logical_pos = calculate_menu_position(...);
let physical_pos = logical_pos.to_physical(dpi_factor);
create_os_window_at(physical_pos);
```

### Challenge 3: Menu Focus Race Conditions

**Problem**: Focus events may arrive after mouse move events, causing premature close.

**Solution**: Add debounce delay (100ms) before closing unfocused menus:
```rust
if mouse_left_at.elapsed() > Duration::from_millis(100) {
    close_menu_chain();
}
```

### Challenge 4: Submenu Hover Detection

**Problem**: Need to detect when mouse hovers over menu item with submenu.

**Solution**: Use hit-testing to identify hovered node, check if it has submenu data:
```rust
let hovered_node = hit_test(mouse_position);
if let Some(submenu_items) = get_submenu_for_node(hovered_node) {
    spawn_submenu(submenu_items);
}
```

## Testing Strategy

### Unit Tests

```rust
#[test]
fn test_menu_position_calculation() {
    let node_rect = LogicalRect::new(100.0, 100.0, 50.0, 30.0);
    let menu_size = LogicalSize::new(200.0, 150.0);
    let screen_size = LogicalSize::new(1920.0, 1080.0);
    
    let pos = calculate_menu_position(node_rect, menu_size, screen_size);
    
    assert_eq!(pos.x, 100.0);
    assert_eq!(pos.y, 130.0);  // Below the node
}

#[test]
fn test_menu_closes_without_focus() {
    let mut chain = MenuChain::new();
    chain.mouse_left_at = Some(Instant::now() - Duration::from_millis(150));
    
    let windows = HashMap::new();
    assert!(chain.should_close(&windows));
}
```

### Integration Tests

```rust
#[test]
fn test_csd_titlebar_injection() {
    let window = create_test_window(WindowFlags {
        has_decorations: true,
        ..Default::default()
    });
    
    let dom = generate_layout(&window);
    
    // First child should be titlebar
    assert_eq!(dom.nodes[0].class_name, "csd-titlebar");
}
```

### Manual Test Cases

1. **CSD Titlebar**
   - [ ] Titlebar appears on frameless window
   - [ ] Close button closes window
   - [ ] Minimize button minimizes window
   - [ ] Maximize button toggles maximize/restore
   - [ ] Drag titlebar moves window

2. **Menu Popups**
   - [ ] Menu appears at correct position
   - [ ] Menu stays within screen bounds
   - [ ] Submenu appears on hover
   - [ ] Menu closes when clicking outside
   - [ ] Menu closes when focus is lost

3. **Nested Submenus**
   - [ ] Multiple submenu levels work
   - [ ] Submenus position correctly (left/right)
   - [ ] Closing parent closes all children
   - [ ] Focus tracking works across chain

4. **Platform-Specific**
   - [ ] macOS uses native NSMenu
   - [ ] Windows uses native HMENU
   - [ ] Linux uses fallback popups
   - [ ] All behave consistently from user perspective

## Performance Considerations

### Menu Window Pooling

To avoid repeated window creation/destruction:

```rust
pub struct MenuWindowPool {
    available: Vec<PlatformWindow>,
    in_use: HashMap<WindowId, PlatformWindow>,
}

impl MenuWindowPool {
    pub fn acquire(&mut self) -> PlatformWindow {
        self.available.pop().unwrap_or_else(|| create_menu_window())
    }
    
    pub fn release(&mut self, window: PlatformWindow) {
        window.hide();
        self.available.push(window);
    }
}
```

### Lazy Menu DOM Generation

Generate menu DOM only when needed:

```rust
pub struct CachedMenuDom {
    items: Vec<MenuItem>,
    dom: Option<StyledDom>,
    dirty: bool,
}

impl CachedMenuDom {
    pub fn get_or_generate(&mut self) -> &StyledDom {
        if self.dirty || self.dom.is_none() {
            self.dom = Some(create_menu_dom(&self.items));
            self.dirty = false;
        }
        self.dom.as_ref().unwrap()
    }
}
```

## Future Enhancements

1. **Menu Keyboard Navigation**
   - Arrow keys to navigate items
   - Enter to activate item
   - Escape to close menu

2. **Menu Item Icons**
   - Support for ImageRef in MenuItem
   - Automatic icon positioning

3. **Menu Item Shortcuts**
   - Display keyboard shortcuts (Ctrl+C, etc.)
   - Automatic shortcut handling

4. **Animated Menu Transitions**
   - Fade in/out effects
   - Slide animations for submenus

5. **Touch Support**
   - Long-press to open context menu
   - Swipe gestures in menus

6. **Accessibility**
   - Screen reader support
   - High contrast themes
   - Focus indicators

## Success Criteria

- [x] Design document complete
- [ ] All platforms have `inject_menu_bar()` implementation
- [ ] CSD titlebar works on frameless windows
- [ ] Menu popups work on Linux
- [ ] Focus management prevents premature closing
- [ ] Menu positioning handles screen bounds
- [ ] All manual tests pass
- [ ] Performance acceptable (< 16ms menu spawn)
- [ ] Documentation complete with examples

---

**Next Steps**: Begin Phase 1 implementation - Core Infrastructure

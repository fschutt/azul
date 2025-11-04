# macOS Native Context Menu - Remaining Implementation

## Status: Partially Implemented ✅

The basic structure for showing native context menus on macOS is in place in `dll/src/desktop/shell2/macos/events.rs`. The following functionality works:

### ✅ Currently Working:
- Basic menu display at cursor position
- String items with labels
- Separators
- Label items (non-interactive headers)
- Enabled/disabled state for menu items
- Window-based context menu fallback (fully functional)

### 🔧 Needs Implementation:

#### 1. Callback Mechanism for Native Menus

**Location**: `show_native_context_menu_at_position()` in `macos/events.rs`

**What's Needed**:
```rust
// Create a delegate class using objc2's declare_class! macro
declare_class!(
    struct MenuItemDelegate {
        callback: Callback,
        data: RefAny,
    }
    
    unsafe impl ClassType for MenuItemDelegate {
        type Super = NSObject;
        const NAME: &'static str = "AzulMenuItemDelegate";
    }
    
    unsafe impl MenuItemDelegate {
        #[method(menuItemClicked:)]
        fn menu_item_clicked(&self, sender: &NSMenuItem) {
            // Extract callback from self
            // Invoke callback with appropriate CallbackInfo
            // Update window state if needed
        }
    }
);
```

**Steps**:
1. Create delegate class inheriting from `NSObject`
2. Add method `menuItemClicked:` as action selector
3. Store callback pointer and data in delegate instance variables
4. Set `menu_item.setTarget(&delegate)` and `menu_item.setAction(sel!(menuItemClicked:))`
5. In `menuItemClicked:`, construct `CallbackInfo` and invoke callback
6. Handle callback result (e.g., trigger redraw, close menu)

**Challenges**:
- Memory management: Delegates must outlive menu items
- Thread safety: Callbacks may need to run on main thread
- State access: Delegate needs reference to window state

#### 2. Submenu Support

**Location**: `show_native_context_menu_at_position()` in `macos/events.rs`

**What's Needed**:
```rust
fn build_submenu(&self, submenu_item: &MenuItem::Sub, mtm: MainThreadMarker) -> Retained<NSMenu> {
    let submenu = NSMenu::new(mtm);
    submenu.setTitle(&NSString::from_str(&submenu_item.label));
    
    // Recursively add items to submenu
    for child in submenu_item.children.as_slice() {
        match child {
            MenuItem::String(item) => { /* add item */ },
            MenuItem::Sub(sub) => {
                // Recursive call for nested submenus
                let nested_menu = self.build_submenu(sub, mtm);
                let menu_item = NSMenuItem::new(mtm);
                menu_item.setSubmenu(&nested_menu);
                submenu.addItem(&menu_item);
            }
            // ...
        }
    }
    
    submenu
}
```

**Steps**:
1. Create recursive `build_submenu()` helper function
2. For `MenuItem::Sub`, create an `NSMenu` and populate it
3. Create an `NSMenuItem` and call `setSubmenu()`
4. Handle nested submenus with recursion
5. Apply callbacks and state to submenu items

#### 3. Checked State Visualization

**Location**: String item handling in `show_native_context_menu_at_position()`

**What's Needed**:
```rust
// If menu item should show a checkmark
if string_item.checked {
    menu_item.setState(NSControlStateValueOn);
} else {
    menu_item.setState(NSControlStateValueOff);
}
```

**Steps**:
1. Check `string_item.checked` field (needs to be added to `StringMenuItem`)
2. Use `setState()` with `NSControlStateValueOn` or `NSControlStateValueOff`
3. Optionally use `NSControlStateValueMixed` for tri-state items

#### 4. Keyboard Shortcuts (Accelerators)

**Location**: String item handling in `show_native_context_menu_at_position()`

**What's Needed**:
```rust
if let Some(ref accelerator) = string_item.accelerator {
    // Parse accelerator string like "Cmd+C", "Ctrl+Shift+S"
    let (key, modifiers) = parse_accelerator(accelerator);
    
    let key_equiv = NSString::from_str(&key);
    menu_item.setKeyEquivalent(&key_equiv);
    menu_item.setKeyEquivalentModifierMask(modifiers);
}

fn parse_accelerator(accel: &str) -> (String, NSEventModifierFlags) {
    // Parse strings like "Cmd+C" into ("c", NSEventModifierFlagsCommand)
    // Handle: Cmd, Ctrl, Shift, Alt/Option
    // Return lowercase key and modifier flags
}
```

**Steps**:
1. Parse accelerator string format (e.g., "Cmd+C", "Ctrl+Alt+Delete")
2. Extract key character (last component, lowercase)
3. Parse modifier flags (Cmd → NSEventModifierFlagsCommand, etc.)
4. Call `setKeyEquivalent()` and `setKeyEquivalentModifierMask()`

#### 5. Icon Support (Optional)

**What's Needed**:
```rust
if let Some(ref icon_path) = string_item.icon {
    let image = NSImage::initWithContentsOfFile(&NSString::from_str(icon_path));
    if let Some(img) = image {
        menu_item.setImage(&img);
    }
}
```

## Testing Plan

### Manual Testing:
1. Right-click on various UI elements
2. Verify menu appears at cursor position
3. Click menu items and verify callbacks fire
4. Test submenus (hover and click)
5. Test checked/unchecked items
6. Test disabled items (should be grayed out)
7. Test keyboard shortcuts (press shortcut while menu is open)
8. Test menu dismissal (click outside, press Escape)

### Edge Cases:
- Empty menus
- Very long labels (should truncate/wrap)
- Deeply nested submenus (3+ levels)
- Rapid right-clicking (multiple menu requests)
- Right-click while menu is already open

## Alternative: Window-Based Menus

The fallback window-based menu system (using `show_window_based_context_menu()`) is **fully implemented** and works correctly. It provides:
- ✅ Full callback support
- ✅ Arbitrary nesting
- ✅ Custom styling
- ✅ Cross-platform consistency

**Recommendation**: For most applications, the window-based menu system is more flexible and easier to maintain. Native menus should only be used when OS integration is critical (e.g., for professional tools that need to match system UI exactly).

## Priority

**Low-Medium**: The window-based fallback works well. Native menu implementation is a "nice to have" for perfect macOS integration, but not critical for functionality.

**Effort**: ~2-3 days for complete implementation including:
- 1 day: Delegate class and callback wiring
- 0.5 day: Submenu recursion
- 0.5 day: State (checked/disabled) and shortcuts
- 1 day: Testing and polish

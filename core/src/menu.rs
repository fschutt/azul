//! Menu system for context menus, dropdown menus, and application menus.
//!
//! This module provides a cross-platform menu abstraction modeled after the Windows API,
//! supporting hierarchical menus with separators, icons, keyboard accelerators, and callbacks.
//!
//! # Core vs Layout Types
//!
//! This module uses `CoreMenuCallback` with `usize` placeholders instead of function pointers
//! to avoid circular dependencies between `azul-core` and `azul-layout`. The actual function
//! pointers are stored in `azul-layout` and converted via unsafe code with identical memory
//! layout.

extern crate alloc;

use alloc::vec::Vec;
use core::hash::Hash;

use azul_css::AzString;

use crate::{
    callbacks::{CoreCallback, CoreCallbackType},
    refany::RefAny,
    resources::ImageRef,
    window::{
        ContextMenuMouseButton, KeyboardState, OptionVirtualKeyCodeCombo, VirtualKeyCode,
        VirtualKeyCodeCombo,
    },
};

/// Does `combo` describe the chord the keyboard is in, with `pressed` as the
/// key that just went down?
///
/// THE shared accelerator rule, used by every platform that dispatches menu
/// accelerators itself (Windows, X11, Wayland, headless; macOS's menu bar
/// uses `AppKit` key equivalents, its context menus this).
///
/// * The combo names its modifiers with the modifier keys (`LControl`,
///   `LShift`, `LAlt`, `LWin`; the right-hand twins are equivalent) and
///   exactly ONE non-modifier key, which must be `pressed`.
/// * `LWin` / `RWin` mean the platform's PRIMARY shortcut modifier — Cmd on
///   macOS, Ctrl everywhere else — so `[LWin, S]` is Cmd+S on a Mac and
///   Ctrl+S on Windows/Linux from one definition (the MWA-A2 rule behind
///   `KeyboardState::primary_down`). `LControl` stays the Control key on
///   every platform.
/// * The match is EXACT: `Ctrl+S` does not fire while Shift is also held, so
///   `Ctrl+S` and `Ctrl+Shift+S` can coexist in one menu.
#[must_use]
pub fn accelerator_matches(
    combo: &VirtualKeyCodeCombo,
    keyboard: &KeyboardState,
    pressed: VirtualKeyCode,
) -> bool {
    let mut want_ctrl = false;
    let mut want_shift = false;
    let mut want_alt = false;
    let mut want_primary = false;
    let mut main_key: Option<VirtualKeyCode> = None;
    for key in combo.keys.as_ref() {
        match key {
            VirtualKeyCode::LControl | VirtualKeyCode::RControl => want_ctrl = true,
            VirtualKeyCode::LShift | VirtualKeyCode::RShift => want_shift = true,
            VirtualKeyCode::LAlt | VirtualKeyCode::RAlt => want_alt = true,
            VirtualKeyCode::LWin | VirtualKeyCode::RWin => want_primary = true,
            other => {
                if main_key.is_some() {
                    // Two non-modifier keys: not a chord this rule can match.
                    return false;
                }
                main_key = Some(*other);
            }
        }
    }
    if main_key != Some(pressed) {
        return false;
    }
    if want_shift != keyboard.shift_down() || want_alt != keyboard.alt_down() {
        return false;
    }
    if cfg!(target_os = "macos") {
        want_ctrl == keyboard.ctrl_down() && want_primary == keyboard.super_down()
    } else {
        // Ctrl IS the primary modifier here; the Super/Windows key never
        // takes part in an application chord.
        (want_ctrl || want_primary) == keyboard.ctrl_down() && !keyboard.super_down()
    }
}

/// Depth-first search of `items` for the first enabled entry whose
/// accelerator matches the chord (see [`accelerator_matches`]).
fn find_accelerated_in<'a>(
    items: &'a [MenuItem],
    keyboard: &KeyboardState,
    pressed: VirtualKeyCode,
) -> Option<&'a StringMenuItem> {
    for item in items {
        let MenuItem::String(s) = item else {
            continue;
        };
        if s.menu_item_state == MenuItemState::Normal {
            if let OptionVirtualKeyCodeCombo::Some(combo) = &s.accelerator {
                if accelerator_matches(combo, keyboard, pressed) {
                    return Some(s);
                }
            }
        }
        if let Some(found) = find_accelerated_in(s.children.as_ref(), keyboard, pressed) {
            return Some(found);
        }
    }
    None
}

/// Represents a menu (context menu, dropdown menu, or application menu).
///
/// A menu consists of a list of items that can be displayed as a popup or
/// attached to a window's menu bar. Modeled after the Windows API for
/// cross-platform consistency.
///
/// # Fields
///
/// * `items` - The menu items to display
/// * `position` - Where the menu should appear (for popups)
/// * `context_mouse_btn` - Which mouse button triggers the context menu
#[derive(Debug, Default, Clone, PartialEq, PartialOrd, Hash, Eq, Ord)]
#[repr(C)]
pub struct Menu {
    pub items: MenuItemVec,
    pub position: MenuPopupPosition,
    pub context_mouse_btn: ContextMenuMouseButton,
}

impl_option!(
    Menu,
    OptionMenu,
    copy = false,
    [Debug, Clone, PartialEq, PartialOrd, Hash, Eq, Ord]
);

impl Menu {
    /// The first enabled item anywhere in this menu whose accelerator
    /// matches the chord the keyboard is in, `pressed` being the key that
    /// just went down. Greyed / disabled items never match.
    #[must_use]
    pub fn find_accelerated_item<'a>(
        &'a self,
        keyboard: &KeyboardState,
        pressed: VirtualKeyCode,
    ) -> Option<&'a StringMenuItem> {
        find_accelerated_in(self.items.as_ref(), keyboard, pressed)
    }

    /// Creates a new menu with the given items.
    ///
    /// Uses default position (`AutoCursor`) and right mouse button for context menus.
    #[must_use]
    pub const fn create(items: MenuItemVec) -> Self {
        Self {
            items,
            position: MenuPopupPosition::AutoCursor,
            context_mouse_btn: ContextMenuMouseButton::Right,
        }
    }

    /// Builder method to set the popup position.
    #[must_use]
    pub const fn with_position(mut self, position: MenuPopupPosition) -> Self {
        self.position = position;
        self
    }

    /// Computes a 64-bit hash of this menu using the `HighwayHash` algorithm.
    ///
    /// This is used to detect changes in menu structure for caching and optimization.
    #[must_use]
    pub fn get_hash(&self) -> u64 {
        use core::hash::Hasher;
        let mut hasher = crate::hash::DefaultHasher::new();
        self.hash(&mut hasher);
        hasher.finish()
    }
}

/// Specifies where a popup menu should appear relative to the cursor or clicked element.
///
/// This positioning information is ignored for application-level menus (menu bars)
/// and only applies to context menus and dropdowns.
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Hash, Eq, Ord)]
#[repr(C)]
pub enum MenuPopupPosition {
    /// Position menu below and to the left of the cursor
    BottomLeftOfCursor,
    /// Position menu below and to the right of the cursor
    BottomRightOfCursor,
    /// Position menu above and to the left of the cursor
    TopLeftOfCursor,
    /// Position menu above and to the right of the cursor
    TopRightOfCursor,
    /// Position menu below the rectangle that was clicked
    BottomOfHitRect,
    /// Position menu to the left of the rectangle that was clicked
    LeftOfHitRect,
    /// Position menu above the rectangle that was clicked
    TopOfHitRect,
    /// Position menu to the right of the rectangle that was clicked
    RightOfHitRect,
    /// Automatically calculate position based on available screen space near cursor
    AutoCursor,
    /// Automatically calculate position based on available screen space near clicked rect
    AutoHitRect,
}

impl Default for MenuPopupPosition {
    fn default() -> Self {
        Self::AutoCursor
    }
}

/// Describes the interactive state of a menu item.
///
/// Menu items can be in different states that affect their appearance and behavior:
///
/// - Normal items are clickable and render normally
/// - Greyed items are visually disabled (greyed out) and non-clickable
/// - Disabled items are non-clickable but retain normal appearance
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Hash, Eq, Ord)]
#[repr(C)]
pub enum MenuItemState {
    /// Normal menu item (default)
    Normal,
    /// Menu item is greyed out and clicking it does nothing
    Greyed,
    /// Menu item is disabled, but NOT greyed out
    Disabled,
}
#[allow(variant_size_differences)] // repr(C,u8) FFI enum: boxing the large variant would change the C ABI (api.json bindings); size disparity accepted
/// Represents a single item in a menu.
///
/// Menu items can be regular text items with labels and callbacks,
/// visual separators, or line breaks for horizontal menu layouts.
#[derive(Debug, Clone, PartialEq, PartialOrd, Hash, Eq, Ord)]
#[repr(C, u8)]
#[allow(clippy::large_enum_variant)] // #[repr(C,u8)] FFI enum: boxing a variant changes the C ABI/api.json
pub enum MenuItem {
    /// A regular menu item with a label, optional icon, callback, and sub-items
    String(StringMenuItem),
    /// A visual separator line (only rendered in vertical layouts)
    Separator,
    /// Forces a line break when the menu is laid out horizontally
    BreakLine,
}

impl_option!(
    MenuItem,
    OptionMenuItem,
    copy = false,
    [Debug, Clone, PartialEq, PartialOrd, Hash, Eq, Ord]
);

impl_vec!(MenuItem, MenuItemVec, MenuItemVecDestructor, MenuItemVecDestructorType, MenuItemVecSlice, OptionMenuItem);
impl_vec_clone!(MenuItem, MenuItemVec, MenuItemVecDestructor);
impl_vec_debug!(MenuItem, MenuItemVec);
impl_vec_partialeq!(MenuItem, MenuItemVec);
impl_vec_partialord!(MenuItem, MenuItemVec);
impl_vec_hash!(MenuItem, MenuItemVec);
impl_vec_eq!(MenuItem, MenuItemVec);
impl_vec_ord!(MenuItem, MenuItemVec);

/// A menu item with a text label and optional features.
///
/// `StringMenuItem` represents a clickable menu entry that can have:
///
/// - A text label
/// - An optional keyboard accelerator (e.g., Ctrl+C)
/// - An optional callback function
/// - An optional icon (checkbox or image)
/// - A state (normal, greyed, or disabled)
/// - Child menu items (for sub-menus)
#[derive(Debug, Clone, PartialEq, PartialOrd, Hash, Eq, Ord)]
#[repr(C)]
pub struct StringMenuItem {
    /// Label of the menu
    /// (ex. "File", "Edit", "View")
    pub label: AzString,
    /// Optional accelerator combination
    /// (ex. "CTRL + X" = [`VirtualKeyCode::Ctrl`, `VirtualKeyCode::X`]) for keyboard shortcut
    pub accelerator: OptionVirtualKeyCodeCombo,
    /// Optional callback to call
    pub callback: OptionCoreMenuCallback,
    /// State (normal, greyed, disabled)
    pub menu_item_state: MenuItemState,
    /// Optional icon for the menu entry
    pub icon: OptionMenuItemIcon,
    /// Sub-menus of this item (separators and line-breaks can't have sub-menus)
    pub children: MenuItemVec,
}

impl StringMenuItem {
    /// Creates a new menu item with the given label.
    /// All optional fields default to `None` / `Normal`.
    #[must_use]
    pub const fn create(label: AzString) -> Self {
        Self {
            label,
            accelerator: OptionVirtualKeyCodeCombo::None,
            callback: OptionCoreMenuCallback::None,
            menu_item_state: MenuItemState::Normal,
            icon: OptionMenuItemIcon::None,
            children: MenuItemVec::from_const_slice(&[]),
        }
    }

    /// Sets the child menu items for this item, creating a sub-menu.
    #[must_use]
    pub fn with_children(mut self, children: MenuItemVec) -> Self {
        self.children = children;
        self
    }

    /// Adds a single child menu item to this item.
    #[must_use]
    pub fn with_child(mut self, child: MenuItem) -> Self {
        let mut children = self.children.into_library_owned_vec();
        children.push(child);
        self.children = children.into();
        self
    }

    /// Attaches a callback function to this menu item.
    ///
    /// # Parameters
    ///
    /// * `data` - User data passed to the callback
    /// * `callback` - Function pointer (as usize) to invoke when item is clicked
    ///
    /// # Note
    ///
    /// This uses `CoreCallbackType` (usize) instead of a real function pointer
    /// to avoid circular dependencies. The conversion happens in azul-layout.
    #[must_use]
    pub fn with_callback<I: Into<CoreCallback>>(mut self, data: RefAny, callback: I) -> Self {
        self.callback = Some(CoreMenuCallback {
            refany: data,
            callback: callback.into(),
        })
        .into();
        self
    }
}

/// Optional icon displayed next to a menu item.
///
/// Icons can be either:
/// - A checkbox (checked or unchecked)
/// - A custom image (typically 16x16 pixels)
#[derive(Debug, Clone, PartialEq, PartialOrd, Hash, Eq, Ord)]
#[repr(C, u8)]
pub enum MenuItemIcon {
    /// Displays a checkbox, with `true` = checked, `false` = unchecked
    Checkbox(bool),
    /// Displays a custom image (typically 16x16 format)
    Image(ImageRef),
}

impl_option!(
    MenuItemIcon,
    OptionMenuItemIcon,
    copy = false,
    [Debug, Clone, PartialEq, PartialOrd, Hash, Eq, Ord]
);

// Core menu callback types (usize-based placeholders)
//
// Similar to CoreCallback, these use usize instead of function pointers
// to avoid circular dependencies. Will be converted to real function
// pointers in azul-layout.
//
// IMPORTANT: Memory layout must be identical to the real callback types!
// Tests for this are in azul-layout/src/callbacks.rs

/// Menu callback using usize placeholder for function pointer.
///
/// This type is used in `azul-core` to represent menu item callbacks without
/// creating circular dependencies with `azul-layout`. The actual function pointer
/// is stored as a `usize` and converted via unsafe code in `azul-layout`.
#[derive(Debug, Clone, PartialEq, PartialOrd, Hash, Eq, Ord)]
#[repr(C)]
pub struct CoreMenuCallback {
    /// User data passed to the callback when the menu item is clicked
    pub refany: RefAny,
    /// Callback function pointer stored as usize (converted to real fn pointer in azul-layout)
    pub callback: CoreCallback,
}

impl_option!(
    CoreMenuCallback,
    OptionCoreMenuCallback,
    copy = false,
    [Debug, Clone, PartialEq, PartialOrd, Hash, Eq, Ord]
);


#[cfg(test)]
#[path = "menu_test.rs"]
mod menu_test;

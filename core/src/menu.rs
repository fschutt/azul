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
    window::{ContextMenuMouseButton, OptionVirtualKeyCodeCombo},
};

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
    /// Creates a new menu with the given items.
    ///
    /// Uses default position (AutoCursor) and right mouse button for context menus.
    pub fn new(items: MenuItemVec) -> Self {
        Self {
            items,
            position: MenuPopupPosition::AutoCursor,
            context_mouse_btn: ContextMenuMouseButton::Right,
        }
    }

    /// Builder method to set the popup position.
    pub fn with_position(mut self, position: MenuPopupPosition) -> Self {
        self.position = position;
        self
    }
}

impl Menu {
    /// Swaps this menu with a default menu and returns the previous contents.
    ///
    /// This is useful for taking ownership of the menu's contents without cloning.
    pub fn swap_with_default(&mut self) -> Self {
        let mut new = Self::default();
        core::mem::swap(&mut new, self);
        new
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

impl Menu {
    /// Computes a 64-bit hash of this menu using the HighwayHash algorithm.
    ///
    /// This is used to detect changes in menu structure for caching and optimization.
    pub fn get_hash(&self) -> u64 {
        use highway::{HighwayHash, HighwayHasher, Key};
        let mut hasher = HighwayHasher::new(Key([0; 4]));
        self.hash(&mut hasher);
        hasher.finalize64()
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

/// Represents a single item in a menu.
///
/// Menu items can be regular text items with labels and callbacks,
/// visual separators, or line breaks for horizontal menu layouts.
#[derive(Debug, Clone, PartialEq, PartialOrd, Hash, Eq, Ord)]
#[repr(C, u8)]
pub enum MenuItem {
    /// A regular menu item with a label, optional icon, callback, and sub-items
    String(StringMenuItem),
    /// A visual separator line (only rendered in vertical layouts)
    Separator,
    /// Forces a line break when the menu is laid out horizontally
    BreakLine,
}

impl_vec!(MenuItem, MenuItemVec, MenuItemVecDestructor);
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
/// 
#[derive(Debug, Clone, PartialEq, PartialOrd, Hash, Eq, Ord)]
#[repr(C)]
pub struct StringMenuItem {
    /// Label of the menu
    pub label: AzString,
    /// Optional accelerator combination
    /// (ex. "CTRL + X" = [VirtualKeyCode::Ctrl, VirtualKeyCode::X]) for keyboard shortcut
    pub accelerator: OptionVirtualKeyCodeCombo,
    /// Optional callback to call
    pub callback: OptionCoreMenuCallback,
    /// State (normal, greyed, disabled)
    pub state: MenuItemState,
    /// Optional icon for the menu entry
    pub icon: OptionMenuItemIcon,
    /// Sub-menus of this item (separators and line-breaks can't have sub-menus)
    pub children: MenuItemVec,
}

impl StringMenuItem {

    /// Creates a new menu item with the given label and default values.
    ///
    /// Default values:
    /// 
    /// - No accelerator
    /// - No callback
    /// - Normal state
    /// - No icon
    /// - No children
    pub const fn new(label: AzString) -> Self {
        StringMenuItem {
            label,
            accelerator: OptionVirtualKeyCodeCombo::None,
            callback: OptionCoreMenuCallback::None,
            state: MenuItemState::Normal,
            icon: OptionMenuItemIcon::None,
            children: MenuItemVec::from_const_slice(&[]),
        }
    }

    /// Swaps this menu item with a default item and returns the previous contents.
    ///
    /// This is useful for taking ownership without cloning.
    pub fn swap_with_default(&mut self) -> Self {
        let mut default = Self {
            label: AzString::from_const_str(""),
            accelerator: None.into(),
            callback: None.into(),
            state: MenuItemState::Normal,
            icon: None.into(),
            children: Vec::new().into(),
        };
        core::mem::swap(&mut default, self);
        default
    }

    /// Sets the child menu items for this item, creating a sub-menu.
    pub fn with_children(mut self, children: MenuItemVec) -> Self {
        self.children = children;
        self
    }

    /// Adds a single child menu item to this item.
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
    pub fn with_callback(mut self, data: RefAny, callback: CoreCallbackType) -> Self {
        self.callback = Some(CoreMenuCallback {
            data,
            callback: CoreCallback { cb: callback, callable: crate::refany::OptionRefAny::None },
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
    pub data: RefAny,
    pub callback: CoreCallback,
}

impl_option!(
    CoreMenuCallback,
    OptionCoreMenuCallback,
    copy = false,
    [Debug, Clone, PartialEq, PartialOrd, Hash, Eq, Ord]
);

use core::hash::Hash;

use azul_css::AzString;

use crate::{
    callbacks::{CoreCallback, CoreCallbackType, RefAny},
    resources::ImageRef,
    window::{ContextMenuMouseButton, OptionVirtualKeyCodeCombo},
};

/// Menu struct (context menu, dropdown menu, context menu)
///
/// Modeled after the Windows API
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
    pub fn new(items: MenuItemVec) -> Self {
        Self {
            items,
            position: MenuPopupPosition::AutoCursor,
            context_mouse_btn: ContextMenuMouseButton::Right,
        }
    }
}

impl Menu {
    pub fn swap_with_default(&mut self) -> Self {
        let mut new = Self::default();
        core::mem::swap(&mut new, self);
        new
    }
}

/// Position of where the menu should popup on the screen
///
/// Ignored for application-level menus
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Hash, Eq, Ord)]
#[repr(C)]
pub enum MenuPopupPosition {
    // relative to cursor
    BottomLeftOfCursor,
    BottomRightOfCursor,
    TopLeftOfCursor,
    TopRightOfCursor,

    // relative to the rect that was clicked on
    BottomOfHitRect,
    LeftOfHitRect,
    TopOfHitRect,
    RightOfHitRect,

    // calculate the position based on how much space
    // is available for the context menu to either side
    // of the screen
    AutoCursor,
    AutoHitRect,
}

impl Default for MenuPopupPosition {
    fn default() -> Self {
        Self::AutoCursor
    }
}

impl Menu {
    pub fn get_hash(&self) -> u64 {
        use highway::{HighwayHash, HighwayHasher, Key};
        let mut hasher = HighwayHasher::new(Key([0; 4]));
        self.hash(&mut hasher);
        hasher.finalize64()
    }
}

/// Describes the state of the menu
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

#[derive(Debug, Clone, PartialEq, PartialOrd, Hash, Eq, Ord)]
#[repr(C, u8)]
pub enum MenuItem {
    /// Regular menu item
    String(StringMenuItem),
    /// Separator line, only rendered when the direction is vertical
    Separator,
    /// Breaks the menu item into separate lines if laid out horizontally
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
    pub fn new(label: AzString) -> Self {
        StringMenuItem {
            label,
            accelerator: None.into(),
            callback: None.into(),
            state: MenuItemState::Normal,
            icon: None.into(),
            children: MenuItemVec::from_const_slice(&[]),
        }
    }

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

    pub fn with_children(mut self, children: MenuItemVec) -> Self {
        self.children = children;
        self
    }

    pub fn with_callback(mut self, data: RefAny, callback: CoreCallbackType) -> Self {
        self.callback = Some(CoreMenuCallback {
            data,
            callback: CoreCallback { cb: callback },
        })
        .into();
        self
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Hash, Eq, Ord)]
#[repr(C, u8)]
pub enum MenuItemIcon {
    /// Menu item shows a checkbox (either checked or not)
    Checkbox(bool),
    /// Menu item shows a custom image, usually in 16x16 format
    Image(ImageRef),
}

impl_option!(
    MenuItemIcon,
    OptionMenuItemIcon,
    copy = false,
    [Debug, Clone, PartialEq, PartialOrd, Hash, Eq, Ord]
);

// ============================================================================
// CORE MENU CALLBACK TYPES (usize-based placeholders)
// ============================================================================
//
// Similar to CoreCallback, these use usize instead of function pointers
// to avoid circular dependencies. Will be converted to real function
// pointers in azul-layout.

/// Core menu callback - uses usize placeholder for the callback function
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

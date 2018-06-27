//! Note: Application menus currently only works on Windows.
//!
//! Linux has a very complicated and especially undocumented API on how to create menus via DBus,
//! and even then, window managers can just "ignore" the DBus menu if they feel like it,
//! so you have to provide a fallback via borderless windows anyway.
//!
//! So there's no guarantee that the "native" menu actually shows up and I really don't have
//! the time to debug for some random guy on the internet why his custom Gentoo installation
//! with a riced xorg.conf doesn't work correcly... if you feel strongly about this,
//! then please contribute the code yourself, I'm happy to accept any hints on how to
//! correctly implement window menus.
//!
//! For the time being, application menus on Linux will be drawn using borderless windows.
//! Yes, it's a shitty solution, but it's better than nothing.
//!
//! I don't have a Mac, so that's why there are currently no menus for Macs, but I've seen
//! crates providing application menus for Cocoa, so I would be happy to use native menus
//! (the ones where the menu is in the top bar, like in any app)
//!
//! Even for Win32 menus, there is a flaw - you can't currently modify them in any way.
//! The reason being that winit (which I use for window creation and management) only

//! This is attributed to Win32: Each menu item is a command, and you can, in the end
//! switch on the command type when you want to see which menu item was clicked.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CommandId(pub u16);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ApplicationMenu {
    pub(crate) items: Vec<MenuItem<ApplicationMenu>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContextMenu {
    pub(crate) items: Vec<MenuItem<ContextMenu>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MenuItem<T> {
    /// Item, such as "New File"
    ClickableItem { id: CommandId, text: String },
    /// Seperator item
    Seperator,
    /// Submenu
    SubMenu { text: String, menu: Box<T> },
}

pub mod command_ids {
    // "Test" menu
    pub const CMD_TEST: u16 = 9001;
}

// Empty test, for some reason codecov doesn't detect any files (and therefore
// doesn't report codecov % correctly) except if they have at least one test in
// the file. This is an empty test, which should be updated later on
#[test]
fn __codecov_test_menu_file() {

}
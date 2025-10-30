//! macOS menu integration for shell2
//!
//! Provides NSMenu creation and updates with hash-based diff to avoid
//! unnecessary menu recreation.

use std::{cell::RefCell, collections::HashMap, sync::Mutex};

use azul_core::menu::{Menu, MenuItem};
use objc2::{define_class, msg_send_id, rc::Retained, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{NSMenu, NSMenuItem};
use objc2_foundation::{NSObject, NSString};

/// Global queue for pending menu actions
/// When a menu item is clicked, its tag is pushed here and can be retrieved by the event loop
static PENDING_MENU_ACTIONS: Mutex<Vec<isize>> = Mutex::new(Vec::new());

/// Internal state for AzulMenuTarget
pub struct AzulMenuTargetIvars {
    _private: u8,
}

/// Objective-C class that receives menu item clicks
///
/// This class acts as the target for all NSMenuItem actions. When a menu item
/// is clicked, the menuItemAction: method is invoked, which adds the tag to
/// a global queue that the MacOSWindow event loop polls.
define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "AzulMenuTarget"]
    #[ivars = AzulMenuTargetIvars]
    pub struct AzulMenuTarget;

    impl AzulMenuTarget {
        /// Menu item action handler
        ///
        /// This method is called when any menu item with this object as its target
        /// is clicked. It extracts the tag from the sender and adds it to the
        /// global pending actions queue.
        #[unsafe(method(menuItemAction:))]
        fn menu_item_action(&self, sender: Option<&NSMenuItem>) {
            if let Some(menu_item) = sender {
                let tag = menu_item.tag();

                eprintln!("[AzulMenuTarget] Menu item clicked with tag: {}", tag);

                // Push tag to global queue
                if let Ok(mut queue) = PENDING_MENU_ACTIONS.lock() {
                    queue.push(tag);
                }
            }
        }
    }
);

impl AzulMenuTarget {
    /// Create a new AzulMenuTarget instance
    pub fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = mtm.alloc::<Self>();
        let this = this.set_ivars(AzulMenuTargetIvars { _private: 0 });
        unsafe { msg_send_id![super(this), init] }
    }

    /// Get the shared singleton instance
    ///
    /// All menu items use the same target instance to reduce memory overhead
    /// and simplify the notification dispatch system.
    pub fn shared_instance(mtm: MainThreadMarker) -> Retained<Self> {
        thread_local! {
            static SHARED: RefCell<Option<Retained<AzulMenuTarget>>> = RefCell::new(None);
        }

        SHARED.with(|shared| {
            let mut shared = shared.borrow_mut();
            if shared.is_none() {
                *shared = Some(Self::new(mtm));
            }
            shared.as_ref().unwrap().clone()
        })
    }
}

/// Get all pending menu actions and clear the queue
///
/// This should be called from the event loop to process menu callbacks
pub fn take_pending_menu_actions() -> Vec<isize> {
    PENDING_MENU_ACTIONS
        .lock()
        .map(|mut queue| std::mem::take(&mut *queue))
        .unwrap_or_default()
}

/// Menu state tracking for diff-based updates
pub struct MenuState {
    /// Current menu hash
    current_hash: u64,
    /// The NSMenu instance
    ns_menu: Option<Retained<NSMenu>>,
    /// Command ID to callback mapping (tag -> CoreMenuCallback)
    command_map: HashMap<i64, azul_core::menu::CoreMenuCallback>,
}

impl MenuState {
    pub fn new() -> Self {
        Self {
            current_hash: 0,
            ns_menu: None,
            command_map: HashMap::new(),
        }
    }

    /// Update menu if hash changed, returns true if menu was recreated
    pub fn update_if_changed(&mut self, menu: &Menu, mtm: MainThreadMarker) -> bool {
        let new_hash = menu.get_hash();

        if new_hash != self.current_hash {
            // Menu changed, rebuild it
            let (ns_menu, command_map) = create_nsmenu(menu, mtm);
            self.ns_menu = Some(ns_menu);
            self.command_map = command_map;
            self.current_hash = new_hash;
            true
        } else {
            false
        }
    }

    /// Get the current NSMenu (if any)
    pub fn get_nsmenu(&self) -> Option<&Retained<NSMenu>> {
        self.ns_menu.as_ref()
    }

    /// Look up callback for a command tag
    pub fn get_callback_for_tag(&self, tag: i64) -> Option<&azul_core::menu::CoreMenuCallback> {
        self.command_map.get(&tag)
    }
}

/// Create an NSMenu from Azul Menu structure
fn create_nsmenu(
    menu: &Menu,
    mtm: MainThreadMarker,
) -> (Retained<NSMenu>, HashMap<i64, azul_core::menu::CoreMenuCallback>) {
    let ns_menu = NSMenu::new(mtm);
    let mut command_map = HashMap::new();
    let mut next_tag = 1i64;

    // Build menu items recursively
    build_menu_items(&menu.items, &ns_menu, &mut command_map, &mut next_tag, mtm);

    (ns_menu, command_map)
}

/// Recursively build menu items
fn build_menu_items(
    items: &azul_core::menu::MenuItemVec,
    parent_menu: &NSMenu,
    command_map: &mut HashMap<i64, azul_core::menu::CoreMenuCallback>,
    next_tag: &mut i64,
    mtm: MainThreadMarker,
) {
    let items = items.as_slice();
    for (index, item) in items.iter().enumerate() {
        match item {
            MenuItem::String(string_item) => {
                if string_item.children.is_empty() {
                    // Leaf menu item
                    let menu_item = NSMenuItem::new(mtm);
                    let title = NSString::from_str(&string_item.label);
                    menu_item.setTitle(&title);

                    // If has callback, assign tag and connect to target
                    if let Some(callback) = string_item.callback.as_option() {
                        let tag = *next_tag;
                        *next_tag += 1;

                        menu_item.setTag(tag as isize);
                        command_map.insert(tag, callback.clone());

                        // Set action and target for callback dispatch
                        let target = AzulMenuTarget::shared_instance(mtm);
                        unsafe {
                            menu_item.setTarget(Some(&target));
                            menu_item.setAction(Some(objc2::sel!(menuItemAction:)));
                        }
                    }

                    // Set keyboard accelerator if present
                    if let Some(accelerator) = string_item.accelerator.as_option() {
                        set_menu_item_accelerator(&menu_item, accelerator);
                    }

                    parent_menu.addItem(&menu_item);
                } else {
                    // Submenu
                    let submenu = NSMenu::new(mtm);
                    let title = NSString::from_str(&string_item.label);
                    submenu.setTitle(&title);

                    let menu_item = NSMenuItem::new(mtm);
                    menu_item.setTitle(&title);
                    menu_item.setSubmenu(Some(&submenu));

                    // Recursively build children
                    build_menu_items(&string_item.children, &submenu, command_map, next_tag, mtm);

                    parent_menu.addItem(&menu_item);
                }
            }
            MenuItem::Separator => {
                let separator = unsafe { NSMenuItem::separatorItem(mtm) };
                parent_menu.addItem(&separator);
            }
            MenuItem::BreakLine => {
                // BreakLine is not supported in macOS menus, treat as separator
                let separator = unsafe { NSMenuItem::separatorItem(mtm) };
                parent_menu.addItem(&separator);
            }
        }
    }
}

/// Set keyboard accelerator on NSMenuItem
///
/// Converts Azul VirtualKeyCodeCombo to macOS key equivalent and modifier mask.
/// macOS uses single character key equivalents (e.g., "x" for Cmd+X).
fn set_menu_item_accelerator(
    menu_item: &NSMenuItem,
    accelerator: &azul_core::window::VirtualKeyCodeCombo,
) {
    use azul_core::window::VirtualKeyCode;
    use objc2_app_kit::NSEventModifierFlags;

    if accelerator.keys.as_slice().is_empty() {
        return;
    }

    let keys = accelerator.keys.as_slice();
    let mut modifier_flags = NSEventModifierFlags::empty();
    let mut key_equivalent: Option<char> = None;

    // Parse modifier keys and find the main key
    for key in keys {
        match key {
            VirtualKeyCode::LControl | VirtualKeyCode::RControl => {
                modifier_flags.insert(NSEventModifierFlags::Control);
            }
            VirtualKeyCode::LShift | VirtualKeyCode::RShift => {
                modifier_flags.insert(NSEventModifierFlags::Shift);
            }
            VirtualKeyCode::LAlt | VirtualKeyCode::RAlt => {
                modifier_flags.insert(NSEventModifierFlags::Option);
            }
            VirtualKeyCode::LWin | VirtualKeyCode::RWin => {
                // On macOS, Windows key = Command key
                modifier_flags.insert(NSEventModifierFlags::Command);
            }
            // Map letter keys
            VirtualKeyCode::A => key_equivalent = Some('a'),
            VirtualKeyCode::B => key_equivalent = Some('b'),
            VirtualKeyCode::C => key_equivalent = Some('c'),
            VirtualKeyCode::D => key_equivalent = Some('d'),
            VirtualKeyCode::E => key_equivalent = Some('e'),
            VirtualKeyCode::F => key_equivalent = Some('f'),
            VirtualKeyCode::G => key_equivalent = Some('g'),
            VirtualKeyCode::H => key_equivalent = Some('h'),
            VirtualKeyCode::I => key_equivalent = Some('i'),
            VirtualKeyCode::J => key_equivalent = Some('j'),
            VirtualKeyCode::K => key_equivalent = Some('k'),
            VirtualKeyCode::L => key_equivalent = Some('l'),
            VirtualKeyCode::M => key_equivalent = Some('m'),
            VirtualKeyCode::N => key_equivalent = Some('n'),
            VirtualKeyCode::O => key_equivalent = Some('o'),
            VirtualKeyCode::P => key_equivalent = Some('p'),
            VirtualKeyCode::Q => key_equivalent = Some('q'),
            VirtualKeyCode::R => key_equivalent = Some('r'),
            VirtualKeyCode::S => key_equivalent = Some('s'),
            VirtualKeyCode::T => key_equivalent = Some('t'),
            VirtualKeyCode::U => key_equivalent = Some('u'),
            VirtualKeyCode::V => key_equivalent = Some('v'),
            VirtualKeyCode::W => key_equivalent = Some('w'),
            VirtualKeyCode::X => key_equivalent = Some('x'),
            VirtualKeyCode::Y => key_equivalent = Some('y'),
            VirtualKeyCode::Z => key_equivalent = Some('z'),
            // Map number keys
            VirtualKeyCode::Key0 => key_equivalent = Some('0'),
            VirtualKeyCode::Key1 => key_equivalent = Some('1'),
            VirtualKeyCode::Key2 => key_equivalent = Some('2'),
            VirtualKeyCode::Key3 => key_equivalent = Some('3'),
            VirtualKeyCode::Key4 => key_equivalent = Some('4'),
            VirtualKeyCode::Key5 => key_equivalent = Some('5'),
            VirtualKeyCode::Key6 => key_equivalent = Some('6'),
            VirtualKeyCode::Key7 => key_equivalent = Some('7'),
            VirtualKeyCode::Key8 => key_equivalent = Some('8'),
            VirtualKeyCode::Key9 => key_equivalent = Some('9'),

            // Special keys - use Unicode characters
            VirtualKeyCode::Return => key_equivalent = Some('\r'),
            VirtualKeyCode::Tab => key_equivalent = Some('\t'),
            VirtualKeyCode::Back => key_equivalent = Some('\u{0008}'), // Backspace
            VirtualKeyCode::Escape => key_equivalent = Some('\u{001B}'),
            VirtualKeyCode::Delete => key_equivalent = Some('\u{007F}'),

            // Function keys - use special NSF... constants (not directly char mappable)
            // For now, skip function keys as they require special handling
            _ => {}
        }
    }

    // Set key equivalent and modifier mask
    if let Some(ch) = key_equivalent {
        let key_str = NSString::from_str(&ch.to_string());
        unsafe {
            menu_item.setKeyEquivalent(&key_str);
            menu_item.setKeyEquivalentModifierMask(modifier_flags);
        }
    }
}

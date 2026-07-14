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
mod autotest_generated {
    use alloc::{format, string::String, vec, vec::Vec};

    use super::*;
    use crate::{
        refany::RefAny,
        window::{ContextMenuMouseButton, VirtualKeyCodeCombo, VirtualKeyCodeVec},
    };

    const ALL_POSITIONS: [MenuPopupPosition; 10] = [
        MenuPopupPosition::BottomLeftOfCursor,
        MenuPopupPosition::BottomRightOfCursor,
        MenuPopupPosition::TopLeftOfCursor,
        MenuPopupPosition::TopRightOfCursor,
        MenuPopupPosition::BottomOfHitRect,
        MenuPopupPosition::LeftOfHitRect,
        MenuPopupPosition::TopOfHitRect,
        MenuPopupPosition::RightOfHitRect,
        MenuPopupPosition::AutoCursor,
        MenuPopupPosition::AutoHitRect,
    ];

    fn item(label: &str) -> MenuItem {
        MenuItem::String(StringMenuItem::create(label.into()))
    }

    fn item_vec(labels: &[&str]) -> MenuItemVec {
        labels.iter().map(|l| item(l)).collect::<Vec<_>>().into()
    }

    fn labels_of(v: &MenuItemVec) -> Vec<&str> {
        v.as_slice()
            .iter()
            .map(|i| match i {
                MenuItem::String(s) => s.label.as_str(),
                MenuItem::Separator => "<sep>",
                MenuItem::BreakLine => "<br>",
            })
            .collect()
    }

    fn all_distinct(hashes: &[u64]) -> bool {
        let mut sorted = hashes.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        sorted.len() == hashes.len()
    }

    // ---- Menu::create ---------------------------------------------------

    #[test]
    fn menu_create_defaults_and_extreme_inputs() {
        for items in [
            MenuItemVec::new(),
            MenuItemVec::from_const_slice(&[]),
            item_vec(&["a"]),
            vec![MenuItem::Separator; 10_000].into(),
        ] {
            let expected_len = items.len();
            let menu = Menu::create(items);
            assert_eq!(menu.items.len(), expected_len);
            assert_eq!(menu.position, MenuPopupPosition::AutoCursor);
            assert_eq!(menu.context_mouse_btn, ContextMenuMouseButton::Right);
            let _ = menu.get_hash();
        }
    }

    #[test]
    fn menu_create_preserves_item_order_and_contents() {
        let source = vec![
            item("File"),
            MenuItem::Separator,
            item("Edit"),
            MenuItem::BreakLine,
            item(""),
        ];
        let menu = Menu::create(source.clone().into());
        assert_eq!(menu.items.as_slice(), source.as_slice());
        assert_eq!(labels_of(&menu.items), ["File", "<sep>", "Edit", "<br>", ""]);
    }

    #[test]
    fn menu_create_of_empty_vec_equals_default() {
        // `create` documents AutoCursor + Right, which must agree with the derived Default.
        assert_eq!(Menu::create(MenuItemVec::new()), Menu::default());
        assert_eq!(
            Menu::create(MenuItemVec::new()).get_hash(),
            Menu::default().get_hash()
        );
    }

    #[test]
    fn menu_item_vec_roundtrip_through_library_owned_vec() {
        let source = vec![item("a"), MenuItem::Separator, item("\u{1F600}")];
        let decoded = MenuItemVec::from(source.clone()).into_library_owned_vec();
        assert_eq!(decoded, source);

        // The &'static (NoDestructor) backing must decode to an owned, equal Vec as well.
        assert!(MenuItemVec::from_const_slice(&[])
            .into_library_owned_vec()
            .is_empty());
    }

    // ---- Menu::with_position --------------------------------------------

    #[test]
    fn menu_with_position_sets_field_and_keeps_items() {
        for pos in ALL_POSITIONS {
            let menu = Menu::create(item_vec(&["a", "b"])).with_position(pos);
            assert_eq!(menu.position, pos);
            assert_eq!(menu.items.len(), 2);
            assert_eq!(labels_of(&menu.items), ["a", "b"]);
            assert_eq!(menu.context_mouse_btn, ContextMenuMouseButton::Right);
        }
    }

    #[test]
    fn menu_with_position_last_call_wins() {
        let menu = Menu::create(MenuItemVec::new())
            .with_position(MenuPopupPosition::TopOfHitRect)
            .with_position(MenuPopupPosition::LeftOfHitRect)
            .with_position(MenuPopupPosition::AutoHitRect);
        assert_eq!(menu.position, MenuPopupPosition::AutoHitRect);
    }

    // ---- Menu::get_hash --------------------------------------------------

    #[test]
    fn menu_get_hash_is_deterministic() {
        let build = || {
            Menu::create(item_vec(&["File", "Edit"])).with_position(MenuPopupPosition::TopOfHitRect)
        };
        let a = build();
        let b = build();
        assert_eq!(a.get_hash(), a.get_hash(), "repeated calls must agree");
        assert_eq!(
            a.get_hash(),
            b.get_hash(),
            "independently built equal menus must agree"
        );
    }

    #[test]
    fn menu_get_hash_on_empty_and_default_does_not_panic() {
        let heap_empty = Menu::create(MenuItemVec::new());
        let static_empty = Menu::create(MenuItemVec::from_const_slice(&[]));

        // Eq/Hash contract: the two empties compare equal, so they must hash equal.
        // A hash that mixed in `ptr`/`cap`/destructor would break here.
        assert_eq!(heap_empty, static_empty);
        assert_eq!(heap_empty.get_hash(), static_empty.get_hash());
        assert_eq!(Menu::default().get_hash(), heap_empty.get_hash());
    }

    #[test]
    fn menu_get_hash_reacts_to_position_and_mouse_button() {
        let hashes: Vec<u64> = ALL_POSITIONS
            .iter()
            .map(|p| Menu::create(item_vec(&["a"])).with_position(*p).get_hash())
            .collect();
        assert!(all_distinct(&hashes), "each popup position must hash apart");

        let btn_hashes: Vec<u64> = [
            ContextMenuMouseButton::Right,
            ContextMenuMouseButton::Middle,
            ContextMenuMouseButton::Left,
        ]
        .iter()
        .map(|b| {
            let mut menu = Menu::create(item_vec(&["a"]));
            menu.context_mouse_btn = *b;
            menu.get_hash()
        })
        .collect();
        assert!(all_distinct(&btn_hashes));
    }

    #[test]
    fn menu_get_hash_distinguishes_nesting_from_flattening() {
        // [a[b]] and [a, b] contain the same labels; a length-prefixed hash must separate them.
        let nested = Menu::create(MenuItemVec::from_item(MenuItem::String(
            StringMenuItem::create("a".into()).with_child(item("b")),
        )));
        let flat = Menu::create(item_vec(&["a", "b"]));
        assert_ne!(nested, flat);
        assert_ne!(nested.get_hash(), flat.get_hash());
    }

    #[test]
    fn menu_get_hash_distinguishes_item_kinds_and_states() {
        let sep = Menu::create(MenuItemVec::from_item(MenuItem::Separator));
        let br = Menu::create(MenuItemVec::from_item(MenuItem::BreakLine));
        assert_ne!(sep.get_hash(), br.get_hash());

        let with_state = |state: MenuItemState| {
            let mut s = StringMenuItem::create("x".into());
            s.menu_item_state = state;
            Menu::create(MenuItemVec::from_item(MenuItem::String(s))).get_hash()
        };
        assert!(all_distinct(&[
            with_state(MenuItemState::Normal),
            with_state(MenuItemState::Greyed),
            with_state(MenuItemState::Disabled),
        ]));

        let with_icon = |icon: OptionMenuItemIcon| {
            let mut s = StringMenuItem::create("x".into());
            s.icon = icon;
            Menu::create(MenuItemVec::from_item(MenuItem::String(s))).get_hash()
        };
        assert!(all_distinct(&[
            with_icon(OptionMenuItemIcon::None),
            with_icon(Some(MenuItemIcon::Checkbox(false)).into()),
            with_icon(Some(MenuItemIcon::Checkbox(true)).into()),
        ]));
    }

    #[test]
    fn menu_get_hash_separates_absent_accelerator_from_empty_one() {
        // `None` vs `Some(<empty combo>)` are different states and must not collide.
        let mut with_empty_combo = StringMenuItem::create("x".into());
        with_empty_combo.accelerator = Some(VirtualKeyCodeCombo {
            keys: VirtualKeyCodeVec::new(),
        })
        .into();

        let none = Menu::create(MenuItemVec::from_item(MenuItem::String(
            StringMenuItem::create("x".into()),
        )));
        let empty = Menu::create(MenuItemVec::from_item(MenuItem::String(with_empty_combo)));
        assert_ne!(none.get_hash(), empty.get_hash());
    }

    #[test]
    fn menu_get_hash_reacts_to_unicode_and_nul_labels() {
        let labels = [
            "",
            "a",
            "a\u{0}b",
            "a\u{0}",
            "\u{0}a",
            "e\u{301}",  // combining acute
            "\u{e9}",    // precomposed é — different bytes, must hash apart
            "\u{202e}x", // RTL override
            "\u{1F469}\u{200D}\u{1F4BB}", // ZWJ emoji sequence
            "\u{10FFFF}",
        ];
        let hashes: Vec<u64> = labels
            .iter()
            .map(|l| Menu::create(item_vec(&[l])).get_hash())
            .collect();
        assert!(all_distinct(&hashes), "distinct labels must hash apart");

        for l in labels {
            let a = Menu::create(item_vec(&[l]));
            let b = Menu::create(item_vec(&[l]));
            assert_eq!(a.get_hash(), b.get_hash());
        }
    }

    #[test]
    fn menu_get_hash_handles_large_menus() {
        let mut labels: Vec<String> = (0..10_000).map(|i| format!("item-{i}")).collect();
        let big: MenuItemVec = labels
            .iter()
            .map(|l| item(l.as_str()))
            .collect::<Vec<_>>()
            .into();
        let menu = Menu::create(big);
        assert_eq!(menu.items.len(), 10_000);
        assert_eq!(menu.get_hash(), menu.get_hash());

        // A single flipped label anywhere in a 10k menu must change the hash.
        labels[9_999] = String::from("item-flipped");
        let flipped = Menu::create(
            labels
                .iter()
                .map(|l| item(l.as_str()))
                .collect::<Vec<_>>()
                .into(),
        );
        assert_ne!(menu.get_hash(), flipped.get_hash());
    }

    #[test]
    fn menu_get_hash_survives_deeply_nested_submenus() {
        // Hash + drop glue both recurse once per level. Depth is bounded deliberately:
        // an unbounded depth would abort the whole test binary on stack exhaustion
        // rather than fail one test.
        const DEPTH: usize = 200;
        let mut nested = StringMenuItem::create("leaf".into());
        for i in 0..DEPTH {
            nested = StringMenuItem::create(format!("lvl-{i}").as_str().into())
                .with_child(MenuItem::String(nested));
        }
        let menu = Menu::create(MenuItemVec::from_item(MenuItem::String(nested)));
        assert_eq!(menu.get_hash(), menu.get_hash());
    }

    #[test]
    fn menu_get_hash_is_stable_across_clone_without_callbacks() {
        let menu = Menu::create(item_vec(&["File", "Edit"]))
            .with_position(MenuPopupPosition::BottomOfHitRect);
        let cloned = menu.clone();
        assert_eq!(menu, cloned);
        assert_eq!(menu.get_hash(), cloned.get_hash());
    }

    /// FIXED (was: "KNOWN HAZARD — pinned, not weakened"). `RefAny::clone()` still mints a
    /// fresh `instance_id`, but `instance_id` is no longer part of `RefAny`'s `Hash`/`Eq` —
    /// those now key on `sharing_info` alone (see the comment on `RefAny` in
    /// `core/src/refany.rs`). A menu carrying a callback is therefore equal to its own
    /// clone, and `get_hash()` — whose documented job is change detection for caching — no
    /// longer reports "changed" for a structurally identical menu.
    ///
    /// The old test asserted the broken behaviour on purpose, "so that fixing `RefAny`
    /// flips it loudly instead of silently". It did exactly that; this is the flip.
    #[test]
    fn menu_get_hash_is_stable_across_clone_with_callback() {
        let menu = Menu::create(MenuItemVec::from_item(MenuItem::String(
            StringMenuItem::create("Save".into()).with_callback(RefAny::new(1u32), 0xDEAD_usize),
        )));
        let cloned = menu.clone();
        assert_eq!(menu, cloned, "a clone shares the same RefAny data, so it must compare equal");
        assert_eq!(menu.get_hash(), cloned.get_hash());
    }

    // ---- StringMenuItem::create ------------------------------------------

    #[test]
    fn string_menu_item_create_defaults() {
        let it = StringMenuItem::create("File".into());
        assert_eq!(it.label.as_str(), "File");
        assert!(it.accelerator.is_none());
        assert!(it.callback.is_none());
        assert_eq!(it.menu_item_state, MenuItemState::Normal);
        assert!(it.icon.is_none());
        assert!(it.children.is_empty());
        assert_eq!(it.children.len(), 0);
    }

    #[test]
    fn string_menu_item_create_preserves_extreme_labels() {
        let huge = "\u{1F600}".repeat(256 * 1024); // ~1 MiB of 4-byte chars
        for label in [
            String::new(),
            String::from("a\u{0}b"),
            String::from("\u{202e}\u{200d}\u{feff}"),
            "x".repeat(1024 * 1024),
            huge,
        ] {
            let it = StringMenuItem::create(label.as_str().into());
            assert_eq!(it.label.as_str(), label.as_str(), "label must survive byte-for-byte");
            assert_eq!(it.label.as_str().len(), label.len());
            assert!(it.children.is_empty());
        }
    }

    // ---- StringMenuItem::with_children -----------------------------------

    #[test]
    fn with_children_sets_replaces_and_clears() {
        let it = StringMenuItem::create("File".into()).with_children(item_vec(&["New", "Open"]));
        assert_eq!(it.children.len(), 2);
        assert_eq!(labels_of(&it.children), ["New", "Open"]);

        // Last call wins — it is a setter, not an appender.
        let it = it.with_children(item_vec(&["Only"]));
        assert_eq!(it.children.len(), 1);
        assert_eq!(labels_of(&it.children), ["Only"]);

        let it = it.with_children(MenuItemVec::new());
        assert!(it.children.is_empty());
        assert_eq!(it.children.get(0), None);
    }

    #[test]
    fn with_children_keeps_other_fields_and_accepts_large_input() {
        let big: MenuItemVec = (0..5_000)
            .map(|i| item(format!("c{i}").as_str()))
            .collect::<Vec<_>>()
            .into();
        let it = StringMenuItem::create("root".into()).with_children(big);
        assert_eq!(it.label.as_str(), "root");
        assert_eq!(it.children.len(), 5_000);
        assert_eq!(it.menu_item_state, MenuItemState::Normal);
        assert!(it.callback.is_none());
        match it.children.get(4_999) {
            Some(MenuItem::String(s)) => assert_eq!(s.label.as_str(), "c4999"),
            other => panic!("unexpected tail item: {other:?}"),
        }
        assert_eq!(it.children.get(5_000), None, "index at len must be None");
    }

    // ---- StringMenuItem::with_child --------------------------------------

    #[test]
    fn with_child_copies_out_of_the_static_backing() {
        // `create` seeds `children` with a NoDestructor (&'static) vec; the first push must
        // copy out of it rather than write through the static pointer.
        let it = StringMenuItem::create("File".into()).with_child(item("New"));
        assert_eq!(it.children.len(), 1);
        assert_eq!(labels_of(&it.children), ["New"]);
        assert!(it.children.capacity() >= 1);

        // A freshly created sibling must still see an empty child list.
        assert!(StringMenuItem::create("Edit".into()).children.is_empty());
    }

    #[test]
    fn with_child_appends_in_order_and_after_with_children() {
        let it = StringMenuItem::create("File".into())
            .with_child(item("a"))
            .with_child(MenuItem::Separator)
            .with_child(item("b"));
        assert_eq!(labels_of(&it.children), ["a", "<sep>", "b"]);

        // with_child appends onto whatever with_children installed.
        let it = StringMenuItem::create("File".into())
            .with_children(item_vec(&["x"]))
            .with_child(item("y"));
        assert_eq!(labels_of(&it.children), ["x", "y"]);
    }

    #[test]
    fn with_child_does_not_alias_a_clone() {
        let base = StringMenuItem::create("File".into()).with_child(item("shared"));
        let extended = base.clone().with_child(item("extra"));
        assert_eq!(base.children.len(), 1, "clone must not write back into the original");
        assert_eq!(extended.children.len(), 2);
        assert_eq!(labels_of(&extended.children), ["shared", "extra"]);
    }

    #[test]
    fn with_child_repeated_many_times_keeps_contents() {
        // Every call round-trips the vec through into_library_owned_vec(); a mishandled
        // destructor tag here would surface as corruption/UAF (also exercised under Miri).
        const N: usize = 2_000;
        let mut it = StringMenuItem::create("root".into());
        for i in 0..N {
            it = it.with_child(item(format!("c{i}").as_str()));
        }
        assert_eq!(it.children.len(), N);
        assert_eq!(labels_of(&it.children)[0], "c0");
        assert_eq!(labels_of(&it.children)[N - 1], "c1999");
        assert_eq!(it.label.as_str(), "root");
    }

    // ---- StringMenuItem::with_callback -----------------------------------

    #[test]
    fn with_callback_stores_pointer_value_and_data() {
        for cb in [0_usize, 1, usize::MAX, usize::MAX - 1] {
            let mut it = StringMenuItem::create("Save".into()).with_callback(RefAny::new(42u64), cb);
            assert!(it.callback.is_some());
            {
                let stored = it.callback.as_ref().expect("callback set");
                assert_eq!(stored.callback.cb, cb, "raw fn-pointer usize must survive verbatim");
                assert!(
                    stored.callback.ctx.is_none(),
                    "native Rust callbacks carry no FFI ctx"
                );
            }
            let data = it.callback.as_mut().expect("callback set");
            let value = data.refany.downcast_ref::<u64>().expect("payload is u64");
            assert_eq!(*value, 42u64);
        }
    }

    #[test]
    fn with_callback_keeps_other_fields_and_last_call_wins() {
        let it = StringMenuItem::create("Save".into())
            .with_children(item_vec(&["kid"]))
            .with_callback(RefAny::new(1u8), 111_usize)
            .with_callback(RefAny::new(2u8), 222_usize);

        let stored = it.callback.as_ref().expect("callback set");
        assert_eq!(stored.callback.cb, 222_usize);
        assert_eq!(it.label.as_str(), "Save");
        assert_eq!(it.children.len(), 1, "callback must not disturb children");
        assert_eq!(it.menu_item_state, MenuItemState::Normal);
    }

    #[test]
    fn with_callback_changes_menu_hash() {
        let plain = Menu::create(MenuItemVec::from_item(MenuItem::String(
            StringMenuItem::create("Save".into()),
        )));
        let with_cb = Menu::create(MenuItemVec::from_item(MenuItem::String(
            StringMenuItem::create("Save".into()).with_callback(RefAny::new(1u32), 7_usize),
        )));
        assert_ne!(plain.get_hash(), with_cb.get_hash());
    }
}

//! Software menu-bar widget — the Linux fallback when there is no native global
//! menu (GNOME/KDE export their own; Windows uses `HMENU`, macOS the app menu).
//!
//! Renders the window's [`Menu`] (declared by the user via
//! `Dom::with_menu_bar(menu)`) as a horizontal bar of top-level items. Clicking a
//! top-level item opens its children as a dropdown popup positioned directly
//! below the item via [`CallbackInfo::open_menu_for_hit_node`] — which looks up
//! the clicked node's on-screen rect and opens a child window at its bottom-left
//! (the unified [`WindowPosition::RelativeToParentWindow`] path).
//!
//! ## Styling
//!
//! All styling is inline via `with_css(&str)` using the `system:` color namespace
//! (`system:window-background`, `system:text`, `system:selection-background`, …)
//! and the `system:ui` font, so the bar matches the OS theme without threading a
//! `SystemStyle` through. The bar MUST be injected at the *Dom* level (before
//! `StyledDom::create_from_dom`) so `scope_inline_css` scopes these rules in the
//! same flatten pass as the rest of the window — see
//! `shell2::common::layout::regenerate_layout`.
//!
//! ## Backreference pattern
//!
//! The dropdown's items are the user's own [`MenuItem`]s, each carrying the
//! `(RefAny, Callback)` backreference the user attached with
//! `StringMenuItem::with_callback(data, cb)`. Clicking a leaf fires that callback
//! with the user's data — so the bar is a thin shell and behaviour stays in user
//! code (see `doc/guide/en/architecture.md`, "the backreference pattern").
//!
//! [`WindowPosition::RelativeToParentWindow`]: azul_core::window::WindowPosition::RelativeToParentWindow

use azul_core::{
    callbacks::{CoreCallback, CoreCallbackData},
    dom::{Dom, EventFilter, HoverEventFilter, IdOrClass::Class, IdOrClassVec},
    menu::{Menu, MenuItem, MenuItemVec, StringMenuItem},
    refany::{OptionRefAny, RefAny},
};

/// Class on the injected bar root — also the detection marker the renderer / tests
/// use to recognise an injected software menu bar.
pub const MENUBAR_CLASS: &str = "__azul-native-menubar";
/// Class on each top-level bar item.
pub const MENUBAR_ITEM_CLASS: &str = "azul-menubar-item";

/// Inline CSS for the bar root: a full-width horizontal flex row themed from the
/// OS (`system:` colors + `system:ui` font). Bare declarations, so the rule is
/// scoped node-only at flatten time.
const MENUBAR_CSS: &str = "display: flex; \
     flex-direction: row; \
     align-items: stretch; \
     width: 100%; \
     height: 26px; \
     background: system:window-background; \
     color: system:text; \
     font-family: system:ui; \
     font-size: 14px; \
     padding-left: 2px;";

/// Inline CSS for a top-level item: vertically-centered click target with hover
/// feedback (the `:hover` block nests via CSS nesting in `parse_inline`).
const MENUBAR_ITEM_CSS: &str = "display: flex; \
     flex-direction: row; \
     align-items: center; \
     padding-left: 10px; \
     padding-right: 10px; \
     color: system:text; \
     cursor: pointer; \
     :hover { background: system:selection-background; color: system:selection-text; }";

/// Build the software menu-bar DOM from a [`Menu`].
///
/// The bar is a flex row of one item per top-level `MenuItem::String`
/// (separators / break-lines are not rendered in the bar). Inject the returned
/// `Dom` at the Dom level so its `with_css` rules are scoped in the main flatten.
pub fn build_menubar_dom(menu: &Menu) -> Dom {
    let mut bar = Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_vec(vec![
            Class(MENUBAR_CLASS.into()),
            Class("azul-menubar".into()),
        ]))
        .with_css(MENUBAR_CSS);

    for item in menu.items.as_slice() {
        if let MenuItem::String(s) = item {
            bar = bar.with_child(build_menubar_item(s));
        }
    }

    bar
}

/// One clickable top-level bar item. Its `MouseUp` callback opens the item's
/// submenu (its children, or — for a top-level leaf — a one-item menu of itself
/// so the leaf's own callback still fires) below the item.
fn build_menubar_item(item: &StringMenuItem) -> Dom {
    // The submenu carried (by value) into the click callback as its RefAny.
    let submenu = if item.children.as_slice().is_empty() {
        Menu::create(MenuItemVec::from_vec(vec![MenuItem::String(item.clone())]))
    } else {
        Menu::create(item.children.clone())
    };

    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_vec(vec![Class(MENUBAR_ITEM_CLASS.into())]))
        .with_css(MENUBAR_ITEM_CSS)
        .with_child(Dom::create_text(item.label.clone()))
        .with_callbacks(
            vec![CoreCallbackData {
                event: EventFilter::Hover(HoverEventFilter::MouseUp),
                callback: CoreCallback {
                    cb: callbacks::menubar_item_click as usize,
                    ctx: OptionRefAny::None,
                },
                refany: RefAny::new(submenu),
            }]
            .into(),
        )
}

pub(crate) mod callbacks {
    use azul_core::{callbacks::Update, menu::Menu, refany::RefAny};

    use crate::callbacks::CallbackInfo;

    /// Top-level bar item clicked → open its submenu under the item. The submenu
    /// `Menu` is the callback's `data` (a backreference set at build time); its
    /// items carry the user's own callbacks, fired by the menu system on click.
    pub extern "C" fn menubar_item_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
        if let Some(menu) = data.downcast_ref::<Menu>() {
            info.open_menu_for_hit_node(menu.clone());
        }
        Update::DoNothing
    }
}

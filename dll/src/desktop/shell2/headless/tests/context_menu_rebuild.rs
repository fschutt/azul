//! A rebuild that only adds or changes a context menu opens the new menu on a
//! right-click, as a fresh window does.
//!
//! A rebuild whose DOM fingerprints did not move keeps the DOM the window
//! retained (its cascade and layout) and carries the fresh build's payloads
//! onto it: callbacks, datasets, views. A context menu was neither in the
//! fingerprints nor carried, so a box that got a menu, or another one, kept
//! the menu of the build before, and a right-click opened that.

use azul_core::{
    dom::IdOrClass,
    menu::{Menu, MenuItem, StringMenuItem},
};

use super::*;

/// The label of the one item of the box's context menu, or no menu.
struct MenuChoice(Option<&'static str>);

extern "C" fn menu_page(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let label = data.downcast_ref::<MenuChoice>().and_then(|choice| choice.0);
    let target = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Class("target".into())].into())
        .with_css("width: 100px; height: 60px; background: #dddddd;");
    let target = match label {
        Some(label) => target.with_context_menu(Menu::create(
            vec![MenuItem::String(StringMenuItem::create(label.into()))].into(),
        )),
        None => target,
    };
    Dom::create_body()
        .with_css("margin: 0px; width: 100%; height: 100%;")
        .with_child(target)
}

/// A window on the page with the menu `label`, laid out and settled.
fn window_with(label: Option<&'static str>) -> (Arc<RefCell<RefAny>>, HeadlessWindow) {
    let state = Arc::new(RefCell::new(RefAny::new(MenuChoice(label))));
    let mut window = make_window_sized(&state, menu_page, 300.0, 200.0);
    window.regenerate_layout().expect("initial layout");
    window.regenerate_layout().expect("settle");
    (state, window)
}

/// The app's next build carries the menu `label`.
fn choose(state: &Arc<RefCell<RefAny>>, label: Option<&'static str>) {
    let mut guard = state.borrow_mut();
    let data: &mut RefAny = &mut guard;
    if let Some(mut choice) = data.downcast_mut::<MenuChoice>() {
        choice.0 = label;
    }
}

/// The labels of the menu a right-click on the box opens, `None` for no menu.
fn menu_on_right_click(window: &mut HeadlessWindow) -> Option<Vec<String>> {
    let _ = step(window, HeadlessEvent::MouseMove { x: 50.0, y: 30.0 });
    let (_, menu) = window
        .common
        .layout_window
        .as_ref()?
        .context_menu_under_pointer()?;
    Some(
        menu.items
            .as_ref()
            .iter()
            .filter_map(|item| match item {
                MenuItem::String(item) => Some(item.label.as_str().to_string()),
                _ => None,
            })
            .collect(),
    )
}

#[test]
fn a_rebuild_that_only_adds_or_changes_a_context_menu_opens_the_new_menu() {
    let (state, mut window) = window_with(None);
    assert_eq!(
        menu_on_right_click(&mut window),
        None,
        "harness: the box has no menu yet"
    );
    for label in [Some("Copy"), Some("Paste"), None, Some("Copy")] {
        choose(&state, label);
        window.regenerate_layout().expect("the rebuild");
        let (_fresh_state, mut fresh) = window_with(label);
        let opened = menu_on_right_click(&mut window);
        assert_eq!(
            opened,
            menu_on_right_click(&mut fresh),
            "the box's menu is {label:?}, as in a fresh window"
        );
        assert_eq!(
            opened,
            label.map(|l| vec![l.to_string()]),
            "a right-click opens the menu {label:?}"
        );
    }
}

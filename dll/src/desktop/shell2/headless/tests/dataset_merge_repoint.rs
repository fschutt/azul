//! A context menu's item counts into the model a rebuild's merge kept.
//!
//! A widget builds its dataset and the data of its handlers from clones of
//! one `RefAny`. A rebuild merges the fresh build's dataset with the one the
//! window holds, keeps one allocation and points every clone of the other at
//! it: the event handlers, the views, the datasets. The items of a context
//! menu were not among them, so after a rebuild a click on one counted into
//! the copy the merge dropped, and the app's model never saw it.

use azul_core::{
    callbacks::{CoreCallback, Update},
    dom::{DatasetMergeCallbackType, DomId, IdOrClass},
    menu::{Menu, MenuItem, OptionCoreMenuCallback, StringMenuItem},
    refany::OptionRefAny,
};

use super::*;

/// The widget's model: how often its menu item was clicked.
struct Counter {
    clicks: u32,
}

/// Which tree the app builds: the box's width, so that a rebuild keeps the
/// tree (the pre-cascade skip) or changes its style (the full path).
struct PageState {
    wide: bool,
}

/// What the menu item's handler does with its data: one more click.
fn add_a_click(data: &mut RefAny) {
    if let Some(mut counter) = data.downcast_mut::<Counter>() {
        counter.clicks += 1;
    }
}

extern "C" fn count_click(mut data: RefAny, _info: azul_layout::callbacks::CallbackInfo) -> Update {
    add_a_click(&mut data);
    Update::DoNothing
}

/// Keeps the model the window holds: it lives on across rebuilds.
extern "C" fn keep_held(_fresh: RefAny, held: RefAny) -> RefAny {
    held
}

extern "C" fn counter_page(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let wide = data.downcast_ref::<PageState>().is_some_and(|s| s.wide);
    let model = RefAny::new(Counter { clicks: 0 });
    let item = StringMenuItem::create("Count".into()).with_callback(
        model.clone(),
        CoreCallback {
            cb: count_click as usize,
            ctx: OptionRefAny::None,
        },
    );
    // The box keeps its identity (type, class, place) in both trees; only its
    // width differs, which the full path lays out again.
    let width = if wide { "200px" } else { "100px" };
    Dom::create_body().with_css("margin: 0px;").with_child(
        Dom::create_div()
            .with_ids_and_classes(vec![IdOrClass::Class("target".into())].into())
            .with_css(&format!("width: {width}; height: 60px;"))
            .with_dataset(OptionRefAny::Some(model))
            .with_merge_callback(keep_held as DatasetMergeCallbackType)
            .with_context_menu(Menu::create(vec![MenuItem::String(item)].into())),
    )
}

/// The box the page builds, in the window's current DOM.
fn target(window: &HeadlessWindow) -> &azul_core::dom::NodeData {
    window
        .common
        .layout_window
        .as_ref()
        .expect("the window lays out")
        .layout_results[&DomId::ROOT_ID]
        .styled_dom
        .node_data
        .as_ref()
        .iter()
        .find(|node| {
            node.get_ids_and_classes()
                .iter()
                .any(|c| matches!(c, IdOrClass::Class(s) if s.as_str() == "target"))
        })
        .expect("the box is laid out")
}

/// The clicks the app's model holds: the counter of the box's dataset.
fn clicks_in_the_model(window: &HeadlessWindow) -> u32 {
    let mut model = target(window).get_dataset().cloned().expect("the box holds the model");
    let clicks = model.downcast_ref::<Counter>().map(|counter| counter.clicks);
    clicks.expect("the model is a counter")
}

/// A click on the box's menu item: its handler's work, with its data.
fn click_the_menu_item(window: &HeadlessWindow) {
    let menu = target(window).get_context_menu().expect("the box has its menu");
    let Some(MenuItem::String(item)) = menu.items.as_ref().first() else {
        panic!("the menu's item");
    };
    let OptionCoreMenuCallback::Some(callback) = &item.callback else {
        panic!("the item has a handler");
    };
    add_a_click(&mut callback.refany.clone());
}

#[test]
fn a_context_menu_item_counts_into_the_model_a_rebuilds_merge_kept() {
    let state = Arc::new(RefCell::new(RefAny::new(PageState { wide: false })));
    let mut window = make_window_sized(&state, counter_page, 300.0, 200.0);
    window.regenerate_layout().expect("initial layout");
    click_the_menu_item(&window);
    assert_eq!(
        clicks_in_the_model(&window),
        1,
        "harness: in a fresh window the item counts into the model"
    );
    for (wide, rebuild) in [
        (false, "a rebuild of the same tree"),
        (true, "a rebuild of another tree"),
    ] {
        {
            let mut guard = state.borrow_mut();
            let data: &mut RefAny = &mut guard;
            let page = data.downcast_mut::<PageState>();
            if let Some(mut page) = page {
                page.wide = wide;
            }
        }
        window.regenerate_layout().expect("the rebuild");
        let before = clicks_in_the_model(&window);
        assert!(before >= 1, "{rebuild}: the merge keeps the model the window held");
        click_the_menu_item(&window);
        assert_eq!(
            clicks_in_the_model(&window),
            before + 1,
            "{rebuild}: the item counts into the model the merge kept"
        );
    }
}

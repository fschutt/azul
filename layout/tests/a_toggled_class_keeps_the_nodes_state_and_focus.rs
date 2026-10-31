//! A node whose class list changes is the same node, as in React: a rebuild
//! that toggles a class (`active`, `selected`) keeps what the node holds -
//! the state its dataset carries and the focus it has. Before, the classes
//! were part of a keyless node's identity, so the toggled node unmounted,
//! its state started over and the focus fell to nothing.

use azul_core::{
    dom::{DatasetMergeCallbackType, Dom, DomId, DomNodeId, IdOrClass, NodeId, TabIndex},
    geom::LogicalSize,
    refany::{OptionRefAny, RefAny},
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const CSS: &str = "* { margin: 0; padding: 0; } \
                   .item { display: block; width: 100px; height: 20px; } \
                   .active { height: 30px; }";

/// body(0) > div.item(1)
const ITEM: NodeId = NodeId::new(1);

/// The item's state: what the user did with it.
struct Counter {
    clicks: u32,
}

/// Keeps the state the window holds: it lives on across rebuilds.
extern "C" fn keep_held(_fresh: RefAny, held: RefAny) -> RefAny {
    held
}

/// The page with the item, its class `active` on or off. Every build hands
/// the item a fresh counter, as an app's `layout()` does.
fn page(active: bool) -> StyledDom {
    let mut classes = vec![IdOrClass::Class("item".into())];
    if active {
        classes.push(IdOrClass::Class("active".into()));
    }
    let mut item = Dom::create_div()
        .with_ids_and_classes(classes.into())
        .with_dataset(OptionRefAny::Some(RefAny::new(Counter { clicks: 0 })))
        .with_merge_callback(keep_held as DatasetMergeCallbackType);
    item.set_tab_index(TabIndex::Auto);
    let mut dom = Dom::create_body().with_child(item);
    let (css, _) = azul_css::parser2::new_from_str(CSS);
    StyledDom::create(&mut dom, css)
}

/// The app renders the page: the reconciliation, the layout and the
/// reconciliation's completion, the way the shells install a new tree.
fn render(lw: &mut LayoutWindow, active: bool) {
    let window_state = lw.current_window_state.clone();
    let renderer_resources = RendererResources::default();
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let mut debug_messages = Some(Vec::new());
    let mut styled = page(active);
    let pending =
        lw.begin_reconciliation(DomId::ROOT_ID, &mut styled, azul_core::task::Instant::now());
    lw.layout_new_generation(
        styled,
        &window_state,
        &renderer_resources,
        &system_callbacks,
        &mut debug_messages,
    )
    .unwrap();
    lw.finish_reconciliation(DomId::ROOT_ID, &pending);
}

fn window() -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    lw.system_animations_override = Some(azul_core::resources::SystemAnimations::disabled());
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = window_state;
    render(&mut lw, false);
    lw
}

fn item() -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(ITEM)),
    }
}

/// The item's counter, as the window holds it.
fn counter(lw: &LayoutWindow) -> RefAny {
    lw.layout_results[&DomId::ROOT_ID]
        .styled_dom
        .node_data
        .as_ref()[ITEM.index()]
    .get_dataset()
    .cloned()
    .expect("the item holds its counter")
}

fn clicks(lw: &LayoutWindow) -> u32 {
    let mut data = counter(lw);
    let clicks = data.downcast_ref::<Counter>().map(|c| c.clicks);
    clicks.expect("the dataset is a counter")
}

/// Three clicks counted into the item's state, and the focus on it.
fn use_the_item(lw: &mut LayoutWindow) {
    let mut data = counter(lw);
    if let Some(mut c) = data.downcast_mut::<Counter>() {
        c.clicks = 3;
    }
    lw.focus_manager.set_focused_node(Some(item()));
}

#[test]
fn a_toggled_class_keeps_the_nodes_dataset_and_focus() {
    let mut lw = window();
    use_the_item(&mut lw);
    assert_eq!(clicks(&lw), 3, "premise: the item counts its clicks");

    for (active, rebuild) in [(true, "a class added"), (false, "the class removed")] {
        render(&mut lw, active);
        assert_eq!(
            clicks(&lw),
            3,
            "{rebuild}: the item is the item it was, its state merged into the new build"
        );
        assert_eq!(
            lw.focus_manager.focused_node,
            Some(item()),
            "{rebuild}: the item keeps the focus"
        );
    }
}

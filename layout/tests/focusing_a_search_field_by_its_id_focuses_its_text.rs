//! Focusing a search field by the id its app gave it focuses the field's text.
//!
//! Found by PIMDRIVE7 (2026-10-03): AzTasks' Cmd+F (`focus_id(info, ids::SEARCH)`, i.e.
//! `CallbackInfo::set_focus(FocusTarget::Id(..))` on the node `.with_id` named) focused
//! `div#search.__azul-native-search-field` - the ROW a `type=search` TextInput wraps its field and
//! clear button in, which holds no text: the typing that followed went nowhere and Enter searched
//! nothing (get_focus_state: `is_contenteditable: false`). AzContacts' search field is the same;
//! the debug server's `focus_node` refuses that node outright ("cannot hold focus").
//!
//! An app names a widget by the root `dom()` hands it; it cannot reach the inner field. So a
//! focus aimed at a box that cannot hold focus goes to the first box inside it that can (HTML's
//! `delegatesFocus`, a `<label>`'s focus), or the search row forwards it. Owner: EVENTS7 (the
//! focus resolution, `managers::focus_cursor::resolve_focus_target`) or WIDGETS7 (the search
//! row); the test states the behaviour, not the place.

use std::collections::BTreeSet;

use azul_core::{
    callbacks::FocusTarget,
    dom::{Dom, DomId, DomNodeId, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    managers::focus_cursor::{resolve_focus_target, FocusResolution},
    widgets::text_input::TextInput,
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

#[test]
fn focusing_a_search_field_by_its_id_focuses_its_text() {
    let mut dom = Dom::create_html().with_child(
        Dom::create_body()
            .with_css("display: flex; flex-direction: column; margin: 0px;")
            .with_child(TextInput::create_search().dom().with_id("search".into())),
    );
    let styled = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 200.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut Some(Vec::new()),
    )
    .unwrap();

    let nodes = &lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("laid out")
        .styled_dom
        .node_data;
    let root = nodes
        .as_container()
        .internal
        .iter()
        .position(|node| node.has_id("search"))
        .expect("the search field's root carries the app's id");
    let target = FocusTarget::Id(DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(root))),
    });
    let resolved = resolve_focus_target(&target, &lw.layout_results, None, &BTreeSet::new());
    let Ok(FocusResolution::Resolved(focused)) = resolved.clone() else {
        panic!("the focus on #search resolved to {resolved:?}");
    };
    let index = focused
        .node
        .into_crate_internal()
        .expect("a node")
        .index();
    assert!(
        nodes.as_container().internal[index].is_focusable(),
        "the focus aimed at #search landed on node {index}, which cannot hold focus (the search \
         row, not its text field)"
    );
}

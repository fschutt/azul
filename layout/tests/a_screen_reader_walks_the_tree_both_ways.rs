//! A screen reader walks the accessibility tree forwards and backwards.
//!
//! VoiceOver finds the next and the previous element through two links of the
//! platform tree only: an element's parent (`accessibilityParent`) and that
//! parent's children (`accessibilityChildren`). accesskit's macOS adapter
//! answers both from the tree this engine publishes, filtered by
//! `accesskit_consumer::common_filter`. These tests lay out real pages, hand
//! the published tree to that same consumer and walk it the way a screen reader
//! does.
#![cfg(feature = "a11y")]

use accesskit::{NodeId as A11yNodeId, Role};
use accesskit_consumer::{common_filter, Node as PlatformNode, Tree};
use azul_core::{
    callbacks::{VirtualViewCallback, VirtualViewCallbackInfo, VirtualViewReturn},
    dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    refany::RefAny,
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
    task::Instant,
};
use azul_css::{AzString, StringVec};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    widgets::{button::Button, check_box::CheckBox, tabs::TabHeader, text_input::TextInput},
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

fn class(name: &str) -> azul_core::dom::IdOrClassVec {
    vec![IdOrClass::Class(name.into())].into()
}

fn para(text: &str) -> Dom {
    Dom::create_p().with_child(Dom::create_text_do_not_use_without_block_level_wrapper(text))
}

fn dnid(node: usize) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(node))),
    }
}

/// `dom` laid out in a `width` x `height` window with `css`, its scroll
/// containers registered the way a shell registers them after a layout.
fn window(mut dom: Dom, css: &str, width: f32, height: f32) -> LayoutWindow {
    let (css, _) = azul_css::parser2::new_from_str(css);
    let styled_dom = StyledDom::create(&mut dom, css);
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(width, height);
    lw.current_window_state = ws.clone();
    let mut dbg = Some(Vec::new());
    lw.layout_and_generate_display_list(
        styled_dom,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut dbg,
    )
    .unwrap();
    register_scroll_nodes(&mut lw);
    lw
}

fn register_scroll_nodes(lw: &mut LayoutWindow) {
    let now = Instant::from(std::time::Instant::now());
    azul_layout::managers::scroll_registration::register_scroll_nodes(lw, &now);
}

/// The tree as a platform adapter holds it after this frame.
fn platform_tree(lw: &mut LayoutWindow) -> Tree {
    lw.update_a11y_tree();
    Tree::new(lw.a11y_manager.full_tree(), true)
}

fn local_id(node: &PlatformNode<'_>) -> A11yNodeId {
    node.locate().0
}

/// Every element a screen reader reaches moving forwards from the window,
/// in order: the filtered tree, depth first.
fn forward(tree: &Tree) -> Vec<A11yNodeId> {
    fn visit(node: PlatformNode<'_>, out: &mut Vec<A11yNodeId>) {
        for child in node.filtered_children(common_filter) {
            out.push(local_id(&child));
            visit(child, out);
        }
    }
    let mut out = Vec::new();
    visit(tree.state().root(), &mut out);
    out
}

/// The element before `node`, found the way VoiceOver finds it: the node's
/// parent, and its place among that parent's children. An `Err` is a node
/// whose parent does not list it, where VoiceOver gets stuck.
fn previous<'a>(node: PlatformNode<'a>) -> Result<Option<PlatformNode<'a>>, String> {
    let Some(parent) = node.filtered_parent(&common_filter) else {
        return Ok(None);
    };
    let siblings: Vec<PlatformNode<'a>> = parent.filtered_children(common_filter).collect();
    let Some(index) = siblings
        .iter()
        .position(|s| local_id(s) == local_id(&node))
    else {
        return Err(format!(
            "{:?} names {:?} as its parent, which does not list it",
            local_id(&node),
            local_id(&parent)
        ));
    };
    if index == 0 {
        return Ok((!parent.is_root()).then_some(parent));
    }
    let before = siblings[index - 1];
    Ok(Some(
        before
            .deepest_last_filtered_child(&common_filter)
            .unwrap_or(before),
    ))
}

/// Every element a screen reader reaches moving backwards from the last one,
/// put back into forward order.
fn backward(tree: &Tree, last: A11yNodeId) -> Result<Vec<A11yNodeId>, String> {
    let state = tree.state();
    let mut node = state
        .root()
        .deepest_last_filtered_child(&common_filter)
        .ok_or("the tree has no element")?;
    if local_id(&node) != last {
        return Err(format!(
            "the deepest last element is {:?}, the forward walk ended on {last:?}",
            local_id(&node)
        ));
    }
    let mut out = vec![local_id(&node)];
    let mut guard = 0usize;
    while let Some(before) = previous(node)? {
        guard += 1;
        if guard > 100_000 {
            return Err("the backward walk does not end".into());
        }
        out.push(local_id(&before));
        node = before;
    }
    out.reverse();
    Ok(out)
}

/// Every element whose parent does not list it among its children.
fn broken_links(tree: &Tree) -> Vec<String> {
    let mut errors = Vec::new();
    let mut stack = vec![tree.state().root()];
    while let Some(node) = stack.pop() {
        for child in node.filtered_children(common_filter) {
            match child.filtered_parent(&common_filter) {
                Some(parent) if local_id(&parent) == local_id(&node) => {}
                other => errors.push(format!(
                    "{:?} is a child of {:?} but names {:?} as its parent",
                    local_id(&child),
                    local_id(&node),
                    other.map(|p| local_id(&p))
                )),
            }
            stack.push(child);
        }
    }
    errors
}

/// The labels of the reachable elements with `role`, in forward order.
fn labels_with_role(tree: &Tree, role: Role) -> Vec<String> {
    forward(tree)
        .into_iter()
        .filter_map(|id| find(tree, id))
        .filter(|(r, _)| *r == role)
        .map(|(_, label)| label)
        .collect()
}

/// The role and label of the element with local id `id`.
fn find(tree: &Tree, id: A11yNodeId) -> Option<(Role, String)> {
    fn visit(node: PlatformNode<'_>, id: A11yNodeId) -> Option<(Role, String)> {
        if local_id(&node) == id {
            return Some((node.role(), node.label().unwrap_or_default()));
        }
        node.children().find_map(|child| visit(child, id))
    }
    visit(tree.state().root(), id)
}

fn assert_walks_agree(tree: &Tree, what: &str) {
    let errors = broken_links(tree);
    assert!(errors.is_empty(), "{what}: broken parent links: {errors:#?}");
    let forward = forward(tree);
    let last = *forward.last().expect("the page has elements");
    let backward = backward(tree, last).unwrap_or_else(|e| panic!("{what}: {e}"));
    assert_eq!(
        backward, forward,
        "{what}: walking backwards must visit exactly the reverse of walking forwards"
    );
}

const WIDGET_CSS: &str = r#"
    * { margin: 0; padding: 0; }
    body { width: 800px; font-size: 14px; }
"#;

/// A form the way the apps build one: tabs, a named check box, a text field
/// and a button between paragraphs.
fn widget_page() -> Dom {
    Dom::create_body()
        .with_child(para("Settings"))
        .with_child(
            TabHeader::create(StringVec::from_vec(vec![
                AzString::from("General"),
                AzString::from("Advanced"),
            ]))
            .dom(),
        )
        .with_child(
            Dom::create_div()
                .with_child(para("Remember me"))
                .with_child(
                    CheckBox::create(true)
                        .with_accessibility_name("Remember me")
                        .dom(),
                ),
        )
        .with_child(TextInput::create().dom().with_accessibility_name("Name"))
        .with_child(Button::create(AzString::from("Save")).dom())
        .with_child(para("Done"))
}

#[test]
fn walking_backwards_through_a_form_visits_exactly_the_reverse_of_walking_forwards() {
    let mut lw = window(widget_page(), WIDGET_CSS, 800.0, 600.0);
    let tree = platform_tree(&mut lw);
    assert_walks_agree(&tree, "a form with nothing focused");

    // The same with the keyboard on the last control (the widgets decide
    // their node indices, so it is looked up).
    let focusable = lw
        .get_layout_result(&DomId::ROOT_ID)
        .and_then(|lr| {
            lr.styled_dom
                .node_data
                .as_ref()
                .iter()
                .rposition(|n| n.is_focusable())
        })
        .expect("the form has a focusable node");
    lw.focus_manager.set_focused_node(Some(dnid(focusable)));
    let tree = platform_tree(&mut lw);
    assert_walks_agree(&tree, "a form with its last control focused");
}

// =========================================================================
// A scrolled list: the same reach above the visible items as below them
// =========================================================================

const LIST_CSS: &str = r#"
    * { margin: 0; padding: 0; }
    body { width: 800px; height: 600px; font-size: 14px; }
    .header { height: 200px; }
    .list { height: 400px; overflow-y: auto; }
    .item { height: 40px; }
"#;

/// `body(0) > [div.header(1) > p(2) > "Toolbar"(3), div.list(4) > 30 x
/// p.item(5 + 2k) > "item k"(6 + 2k)]`: a toolbar 200 px high, under it a
/// list reaching the bottom of the 600 px window.
const LIST: usize = 4;

fn list_page() -> Dom {
    let mut list = Dom::create_div().with_ids_and_classes(class("list"));
    for k in 0..30 {
        list = list.with_child(para(&format!("item {k}")).with_ids_and_classes(class("item")));
    }
    Dom::create_body()
        .with_child(
            Dom::create_div()
                .with_ids_and_classes(class("header"))
                .with_child(para("Toolbar")),
        )
        .with_child(list)
}

#[test]
fn a_scrolled_list_keeps_one_item_past_each_edge_reachable_on_both_sides() {
    let mut lw = window(list_page(), LIST_CSS, 800.0, 600.0);
    // Scrolled by ten items: items 10..=19 show in the list's box, items 5..=9
    // lie under the toolbar (inside the window, outside the list), items 20..
    // below the window.
    lw.scroll_manager.set_scroll_position(
        DomId::ROOT_ID,
        NodeId::new(LIST),
        LogicalPosition::new(0.0, 400.0),
        Instant::from(std::time::Instant::now()),
    );
    let tree = platform_tree(&mut lw);
    assert_walks_agree(&tree, "a scrolled list");

    let items: Vec<String> = labels_with_role(&tree, Role::Paragraph)
        .into_iter()
        .filter(|label| label.starts_with("item "))
        .collect();
    let expected: Vec<String> = (9..=20).map(|k| format!("item {k}")).collect();
    assert_eq!(
        items, expected,
        "a list clips its items: a screen reader reaches the visible ones and ONE item past \
         each edge (it scrolls that one into view and the next becomes reachable), the same \
         above the list as below it. Moving forwards and backwards must reach the same distance."
    );
}

// =========================================================================
// Focus
// =========================================================================

#[test]
fn with_nothing_focused_the_published_focus_is_the_window() {
    let mut lw = window(
        Dom::create_body()
            .with_child(para("A label"))
            .with_child(Button::create(AzString::from("OK")).dom()),
        WIDGET_CSS,
        400.0,
        300.0,
    );
    lw.focus_manager.set_focused_node(None);
    lw.update_a11y_tree();
    assert_eq!(
        lw.a11y_manager.full_tree().focus,
        lw.a11y_manager.root_id,
        "accesskit: \"if no specific node within the tree has keyboard focus, this must be set \
         to the root\". A label that does not have the keyboard must not be announced as focused."
    );
}

#[test]
fn a_focus_inside_a_hidden_subtree_is_published_as_the_window() {
    // body(0) > [p(1) > "Visible"(2), div[hidden](3) > p(4) > "Secret"(5)]
    let mut lw = window(
        Dom::create_body().with_child(para("Visible")).with_child(
            Dom::create_div()
                .with_attribute(azul_core::dom::AttributeType::Hidden)
                .with_child(para("Secret").with_tab_index(azul_core::dom::TabIndex::Auto)),
        ),
        WIDGET_CSS,
        400.0,
        300.0,
    );
    lw.focus_manager.set_focused_node(Some(dnid(4)));
    let tree = platform_tree(&mut lw);
    assert_walks_agree(&tree, "a focus inside a hidden subtree");
    let focus = lw.a11y_manager.full_tree().focus;
    assert!(
        focus == lw.a11y_manager.root_id || forward(&tree).contains(&focus),
        "the published focus {focus:?} must be an element a screen reader can reach from its \
         parent, or the window: a focus the parent does not list strands the VoiceOver cursor"
    );
}

// =========================================================================
// A virtual view's content is read where the view sits
// =========================================================================

extern "C" fn render_inside(_data: RefAny, info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    let dom = Dom::create_div().with_child(para("inside"));
    let rect = LogicalRect::new(LogicalPosition::zero(), info.bounds.logical_size);
    VirtualViewReturn::with_dom(dom, rect, rect)
}

#[test]
fn a_virtual_views_content_is_read_where_the_view_sits_and_drawn_where_it_shows() {
    let css = r#"
        * { margin: 0; padding: 0; }
        body { width: 400px; font-size: 14px; }
        p { height: 30px; }
    "#;
    let page = Dom::create_body()
        .with_child(para("before"))
        .with_child(
            Dom::create_virtual_view(RefAny::new(()), VirtualViewCallback::create(render_inside))
                .with_css("width: 200px; height: 100px;"),
        )
        .with_child(para("after"));
    let mut lw = window(page, css, 400.0, 300.0);
    let tree = platform_tree(&mut lw);
    assert_walks_agree(&tree, "a page with a virtual view");

    let order = labels_with_role(&tree, Role::Paragraph);
    assert_eq!(
        order,
        vec!["before".to_string(), "inside".into(), "after".into()],
        "the view's content is part of the page at the view's place, not appended after it"
    );

    let update = lw.a11y_manager.full_tree();
    let y0_of = |label: &str| {
        update
            .nodes
            .iter()
            .find(|(_, n)| n.role() == Role::Paragraph && n.label() == Some(label))
            .and_then(|(id, _)| azul_layout::managers::a11y::window_bounds_of(&update, *id))
            .map(|r| (r.y0, r.y1))
    };
    let (_, before_y1) = y0_of("before").expect("the paragraph before the view has bounds");
    let (inside_y0, _) = y0_of("inside").expect("the paragraph inside the view has bounds");
    assert!(
        inside_y0 >= before_y1 - 0.5,
        "the view's content is drawn below the paragraph before the view (y0 {inside_y0} >= \
         {before_y1}): its bounds are in window space, not in the view's own 0-relative space"
    );
}

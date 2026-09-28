//! A secondary click - a right press, or Control + click on macOS - opens the
//! context menu of the node under it, or of its nearest ancestor with one.
//!
//! Device report (AzWidgets, macOS, 2026-09-28): the "Right-click me for a
//! context menu" box in the Menus card opened no menu. Every shell used to
//! pick the node on its own (macOS the front-most hit, X11 / Wayland the
//! highest `NodeId` of the lowest dom, Windows the OUTERMOST node with a
//! menu), so no harness saw what a device would open. The pick now lives in
//! `azul_layout::context_menu` and every shell presents what
//! `LayoutWindow::context_menu_under_pointer` answers - which is what these
//! tests drive: the press router first, the hit test for the event, then
//! the menu, the order the shells run them in.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, IdOrClass, NodeData, NodeId},
    events::MouseButton,
    geom::{LogicalPosition, LogicalRect},
    hit_test::{FullHitTest, HitTest, HitTestItem},
    menu::{Menu, MenuItem},
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
    task::Instant,
};
use azul_css::system::Platform;
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    context_menu::{context_menu_under_hit, is_secondary_press},
    headless::CpuHitTester,
    press_router::PressTarget,
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// A menu told apart from others by its item count.
fn menu_of(items: usize) -> Menu {
    Menu::create(
        (0..items)
            .map(|_| MenuItem::Separator)
            .collect::<Vec<_>>()
            .into(),
    )
}

fn node(dom: usize, n: usize) -> DomNodeId {
    DomNodeId {
        dom: DomId { inner: dom },
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(n))),
    }
}

fn hit_item(depth: u32) -> HitTestItem {
    HitTestItem {
        point_in_viewport: LogicalPosition::zero(),
        point_relative_to_item: Default::default(),
        is_focusable: false,
        is_virtual_view_hit: None,
        hit_depth: depth,
    }
}

/// A hit test of `(dom, node, depth)` triples; depth 0 is the front-most.
fn hit_test(hits: &[(usize, usize, u32)]) -> FullHitTest {
    let mut full = FullHitTest::empty(None);
    for &(dom, n, depth) in hits {
        full.hovered_nodes
            .entry(DomId { inner: dom })
            .or_insert_with(HitTest::empty)
            .regular_hit_test_nodes
            .insert(NodeId::new(n), hit_item(depth));
    }
    full
}

/// The AzWidgets page, cut down to what the Menus card sits in: a column
/// body with a 38px titlebar and a scrolling page (`overflow-y: auto`),
/// the card between two tall fillers, and in it the dashed box that carries
/// the context menu, its label centred.
fn menus_page() -> StyledDom {
    let filler = || {
        Dom::create_div()
            .with_css("height: 900px; margin-bottom: 20px; background-color: #eeeeee;")
    };
    let menu_box = Dom::create_div()
        .with_css(
            "display: flex; align-items: center; justify-content: center; height: 80px; \
             border: 1px dashed #999999; border-radius: 8px; background-color: #ffffff; \
             cursor: context-menu;",
        )
        .with_child(Dom::create_span_with_text(
            "Right-click me for a context menu",
        ))
        .with_context_menu(menu_of(3));
    let labelled = Dom::create_div()
        .with_css("display: flex; flex-direction: column; margin-bottom: 16px;")
        .with_child(
            Dom::create_span_with_text("Context menu")
                .with_css("font-size: 12px; margin-bottom: 6px;"),
        )
        .with_child(menu_box);
    let card = Dom::create_div()
        .with_css(
            "display: flex; flex-direction: column; background-color: #f5f5f5; \
             border-radius: 10px; padding: 18px; margin-bottom: 20px;",
        )
        .with_child(
            Dom::create_div_with_text("Menus").with_css("font-size: 18px; margin-bottom: 14px;"),
        )
        .with_child(labelled);
    let page = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Id("page".into())].into())
        .with_css(
            "display: flex; flex-direction: column; overflow-y: auto; flex-grow: 1; \
             min-height: 0; padding: 24px;",
        )
        .with_child(filler())
        .with_child(card)
        .with_child(filler());
    let titlebar = Dom::create_div().with_css("height: 38px; flex-grow: 0; flex-shrink: 0;");
    StyledDom::create_from_dom(
        Dom::create_body()
            .with_css("margin: 0; display: flex; flex-direction: column; height: 100%;")
            .with_child(titlebar)
            .with_child(page),
    )
}

struct Window {
    lw: LayoutWindow,
    hit_tester: CpuHitTester,
}

impl Window {
    /// `styled_dom` laid out in a `width` x `height` window, followed by the
    /// shells' finalize tail: scroll registration, then the hit-tester
    /// rebuild.
    fn new(styled_dom: StyledDom, width: f32, height: f32) -> Self {
        let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
        let mut window_state = FullWindowState::default();
        window_state.size.dimensions = azul_core::geom::LogicalSize::new(width, height);
        lw.current_window_state = window_state.clone();
        let renderer_resources = RendererResources::default();
        let system_callbacks = ExternalSystemCallbacks::rust_internal();
        let mut dbg = Some(Vec::new());
        lw.layout_and_generate_display_list(
            styled_dom,
            &window_state,
            &renderer_resources,
            &system_callbacks,
            &mut dbg,
        )
        .unwrap();
        lw.current_window_state = window_state;
        let now = Instant::from(std::time::Instant::now());
        azul_layout::managers::scroll_registration::register_scroll_nodes(&mut lw, &now);
        let mut hit_tester = CpuHitTester::new();
        hit_tester.rebuild_from_layout_with_gpu(&lw.layout_results, Some(&lw.gpu_state_manager));
        Self { lw, hit_tester }
    }

    /// The first node of the root dom that `f` accepts.
    fn node_where(&self, f: impl Fn(&NodeData) -> bool) -> NodeId {
        let lr = self
            .lw
            .layout_results
            .get(&DomId::ROOT_ID)
            .expect("root layout result");
        let index = lr
            .styled_dom
            .node_data
            .as_ref()
            .iter()
            .position(|nd| f(nd))
            .expect("the node exists");
        NodeId::new(index)
    }

    /// The border box of `node` as laid out (static space: before any
    /// scroll).
    fn border_box(&self, node: NodeId) -> LogicalRect {
        let lr = self
            .lw
            .layout_results
            .get(&DomId::ROOT_ID)
            .expect("root layout result");
        let idx = *lr
            .layout_tree
            .dom_to_layout
            .get(&node)
            .and_then(|indices| indices.first())
            .expect("the node has a layout box");
        let origin = lr
            .calculated_positions
            .get(idx.index())
            .copied()
            .expect("the node has a position");
        let size = lr
            .layout_tree
            .get(idx)
            .and_then(|n| n.used_size)
            .expect("the node has a size");
        LogicalRect::new(origin, size)
    }

    /// Port of `PlatformWindow::update_hit_test_at` (the same port as
    /// `a_press_on_an_overflowing_field_selects.rs`).
    fn update_hit_test_at(&mut self, position: LogicalPosition) {
        use azul_layout::managers::hover::InputPointId;
        let focused_node = self.lw.focus_manager.get_focused_node().copied();
        let hit_test = {
            let scroll_manager = &self.lw.scroll_manager;
            let gpu = &self.lw.gpu_state_manager;
            let resolve = |d: DomId, n: NodeId| -> Option<LogicalPosition> {
                scroll_manager.get_current_offset(d, n)
            };
            let resolve_tf = |d: DomId, n: NodeId| {
                gpu.caches
                    .get(&d)
                    .and_then(|c| c.css_current_transform_values.get(&n))
                    .copied()
            };
            let hits = self
                .hit_tester
                .hit_test_scrolled(position, &resolve, &resolve_tf);
            azul_layout::headless::convert_cpu_hit_test_to_full(
                &self.hit_tester,
                &hits,
                focused_node,
                &self.lw.layout_results,
                position,
                &resolve,
                &resolve_tf,
            )
        };
        self.lw
            .hover_manager
            .push_hit_test(InputPointId::Mouse, hit_test);
    }

    /// A secondary click at `at` the way the shells run it: the press router
    /// first (a secondary press never works a scrollbar), the hit test for
    /// the event, then the menu the engine picks.
    fn secondary_click(
        &mut self,
        at: LogicalPosition,
        button: MouseButton,
    ) -> Option<(DomNodeId, Menu)> {
        let now = Instant::from(std::time::Instant::now());
        assert!(
            matches!(self.lw.route_press(at, button, now), PressTarget::Content),
            "the press at {at:?} went to a scrollbar"
        );
        self.update_hit_test_at(at);
        self.lw.context_menu_under_pointer()
    }
}

/// The Menus card of a scrolled page: the page scrolled until the box is
/// in the middle of the window, and the two points a user right-clicks -
/// the box's own surface (off the label) and the label.
fn scrolled_menus_card() -> (Window, NodeId, LogicalPosition, LogicalPosition) {
    let mut w = Window::new(menus_page(), 800.0, 600.0);
    let menu_box = w.node_where(NodeData::has_context_menu);
    let page = w.node_where(|nd| {
        nd.get_ids_and_classes()
            .as_ref()
            .iter()
            .any(|c| matches!(c, IdOrClass::Id(id) if id.as_str() == "page"))
    });

    let at_rest = w.border_box(menu_box);
    let scroll_y = at_rest.origin.y - 250.0;
    let now = Instant::from(std::time::Instant::now());
    w.lw.scroll_manager.set_scroll_position(
        DomId::ROOT_ID,
        page,
        LogicalPosition::new(0.0, scroll_y),
        now,
    );
    let offset = w
        .lw
        .scroll_manager
        .get_current_offset(DomId::ROOT_ID, page)
        .unwrap_or_default();
    assert!(
        (offset.y - scroll_y).abs() < 0.5,
        "the page must scroll to {scroll_y}, is at {offset:?}"
    );

    // Where the box is on screen now.
    let on_screen_y = at_rest.origin.y - offset.y;
    let surface = LogicalPosition::new(at_rest.origin.x + 8.0, on_screen_y + 8.0);
    let label = LogicalPosition::new(
        at_rest.origin.x + at_rest.size.width / 2.0,
        on_screen_y + at_rest.size.height / 2.0,
    );
    (w, menu_box, surface, label)
}

/// The device report, headless: a right press on the box - on its surface
/// and on its label - opens the box's menu, with the page scrolled the way
/// it is when the Menus card is on screen.
#[test]
fn a_right_press_on_the_menu_box_of_a_scrolled_page_opens_its_menu() {
    let (mut w, menu_box, surface, label) = scrolled_menus_card();
    let expected = node(0, menu_box.index());

    for (what, at) in [("surface", surface), ("label", label)] {
        let (owner, menu) = w
            .secondary_click(at, MouseButton::Right)
            .unwrap_or_else(|| panic!("a right press on the box's {what} at {at:?} opened nothing"));
        assert_eq!(owner, expected, "a right press on the box's {what} opened another node's menu");
        assert_eq!(menu.items.as_slice().len(), 3);
    }
}

/// Control + click is the macOS secondary click: the same press with the
/// primary button opens the same menu there.
#[test]
fn a_control_click_on_the_menu_box_opens_its_menu_on_macos() {
    assert!(is_secondary_press(&Platform::MacOs, MouseButton::Left, true));

    let (mut w, menu_box, surface, _) = scrolled_menus_card();
    let (owner, _) = w
        .secondary_click(surface, MouseButton::Left)
        .expect("a Control + click on the box opened nothing");
    assert_eq!(owner, node(0, menu_box.index()));
}

/// What a secondary click is: the right button everywhere, Control +
/// primary on macOS only (Command + click extends a selection there, and
/// Control + click is a multi-select elsewhere).
#[test]
fn a_control_click_is_the_secondary_click_on_macos_and_only_there() {
    assert!(is_secondary_press(&Platform::MacOs, MouseButton::Right, false));
    assert!(is_secondary_press(&Platform::Windows, MouseButton::Right, false));
    assert!(is_secondary_press(&Platform::Unknown, MouseButton::Right, true));
    assert!(is_secondary_press(&Platform::MacOs, MouseButton::Left, true));
    assert!(!is_secondary_press(&Platform::MacOs, MouseButton::Left, false));
    assert!(!is_secondary_press(&Platform::Windows, MouseButton::Left, true));
    assert!(!is_secondary_press(&Platform::Unknown, MouseButton::Left, true));
    assert!(!is_secondary_press(&Platform::MacOs, MouseButton::Middle, true));
}

/// Nested menus: the innermost wins. (Windows used to open the OUTERMOST -
/// the first node with a menu in `NodeId` order.)
#[test]
fn the_innermost_of_two_nested_context_menus_opens() {
    // body(0, a 1-item menu) > box(1, a 2-item menu) > label(2)
    let dom = StyledDom::create_from_dom(
        Dom::create_body()
            .with_context_menu(menu_of(1))
            .with_child(
                Dom::create_div()
                    .with_context_menu(menu_of(2))
                    .with_child(Dom::create_div()),
            ),
    );
    let hit = hit_test(&[(0, 0, 2), (0, 1, 1), (0, 2, 0)]);
    let (owner, menu) = context_menu_under_hit(
        &hit,
        &|d: DomId| (d.inner == 0).then_some(&dom),
        &|_: DomId| -> Option<(DomId, NodeId)> { None },
    )
    .expect("the label sits inside two menus");
    assert_eq!(owner, node(0, 1));
    assert_eq!(menu.items.as_slice().len(), 2);
}

/// A child dom (a `VirtualView` page: a video, a progress bar, a
/// virtualized list) inside a box with a context menu: a right press on the
/// page opens the BOX's menu, as a right press on any other content of the
/// box does. The page's own dom has no menu; its host does.
#[test]
fn a_right_press_on_a_page_that_a_menu_box_hosts_opens_the_boxs_menu() {
    // dom 0: body(0) > box(1, a 3-item menu) > host(2)
    // dom 1, hosted by node 2 of dom 0: body(0) > div(1) > div(2)
    let host = StyledDom::create_from_dom(
        Dom::create_body().with_child(
            Dom::create_div()
                .with_context_menu(menu_of(3))
                .with_child(Dom::create_div()),
        ),
    );
    let page = StyledDom::create_from_dom(
        Dom::create_body().with_child(Dom::create_div().with_child(Dom::create_div())),
    );
    // The page is composited over its host: its hits are the front-most.
    let hit = hit_test(&[(0, 0, 5), (0, 1, 4), (0, 2, 3), (1, 0, 2), (1, 1, 1), (1, 2, 0)]);
    let doms = |d: DomId| match d.inner {
        0 => Some(&host),
        1 => Some(&page),
        _ => None,
    };
    let host_of = |d: DomId| (d.inner == 1).then_some((DomId { inner: 0 }, NodeId::new(2)));

    let (owner, menu) = context_menu_under_hit(&hit, &doms, &host_of)
        .expect("a right press on the page opened nothing - its host's menu was not found");
    assert_eq!(owner, node(0, 1));
    assert_eq!(menu.items.as_slice().len(), 3);
}

/// Nothing under the pointer has a menu: nothing opens.
#[test]
fn a_right_press_where_no_node_has_a_menu_opens_nothing() {
    let dom = StyledDom::create_from_dom(
        Dom::create_body().with_child(Dom::create_div().with_child(Dom::create_div())),
    );
    let hit = hit_test(&[(0, 0, 2), (0, 1, 1), (0, 2, 0)]);
    assert!(context_menu_under_hit(
        &hit,
        &|d: DomId| (d.inner == 0).then_some(&dom),
        &|_: DomId| -> Option<(DomId, NodeId)> { None }
    )
    .is_none());
}

//! A rebuild that changes who listens hits and tags like a fresh window.
//!
//! An app rebuilds its page with a click handler added to one box, or taken
//! from another. Nothing a box is laid out or painted from moved, so the
//! display list is patched: every box keeps the items it painted last.
//!
//! A click still lands where a fresh window puts it. The pointer is resolved
//! by the layout's own hit tester (`CpuHitTester`, the only one, on every
//! backend), from the boxes laid out, and a box's hit-test tag is its node's
//! index (`generate_tag_ids`), not a running count: a box that starts
//! listening moves no other box's tag.
//!
//! The display list has to carry the hit-test areas of the page as it is now
//! as well. A box's area is painted with its other items, so a box that starts
//! listening has none in the items it keeps, and one that stops keeps its
//! old one. The bounds a menu is anchored to and a scripted click is aimed at
//! (`get_node_hit_test_bounds`) are read from those areas, and so is what the
//! WebRender hit tester is handed.

use azul_core::{
    callbacks::Update,
    dom::{Dom, DomId, DomNodeId, EventFilter, HoverEventFilter, NodeId},
    geom::{LogicalPosition, LogicalRect},
    hit_test::TAG_TYPE_DOM_NODE,
    refany::RefAny,
    styled_dom::NodeHierarchyItemId,
};
use azul_layout::{
    callbacks::{Callback, CallbackInfo},
    headless::CpuHitTester,
    solver3::display_list::DisplayListItem,
    window::LayoutWindow,
};

use crate::{
    a_measured_label_flows_its_text_only_at_a_new_width::{node_with_class, rebuild, rect},
    a_one_box_slide_does_not_re_lay_out_the_page::window,
};

extern "C" fn clicked(_data: RefAny, _info: CallbackInfo) -> Update {
    Update::DoNothing
}

/// A 60 x 20 box of class `class`, listening for a click when `listens`.
fn button(class: &'static str, listens: bool) -> Dom {
    let dom = Dom::create_div()
        .with_class(class.into())
        .with_css("width: 60px; height: 20px; background: #bbbbbb;");
    if listens {
        dom.with_callback(
            EventFilter::Hover(HoverEventFilter::Click),
            RefAny::new(class),
            Callback::from_ptr(clicked).to_core(),
        )
    } else {
        dom
    }
}

/// The boxes `a`, `x`, `b` and `c` in a column: `b` and `c` listen, `a` when
/// `a_listens`, `x` (before `b`) when `x_listens`.
fn page((a_listens, x_listens): (bool, bool)) -> Dom {
    Dom::create_html().with_child(
        Dom::create_body()
            .with_css("margin: 0px;")
            .with_child(button("a", a_listens))
            .with_child(button("x", x_listens))
            .with_child(button("b", true))
            .with_child(button("c", true)),
    )
}

/// Whether `a` and `x` listen.
type Listening = (bool, bool);

/// The page before and after each rebuild, and what the rebuild changes.
const REBUILDS: [(Listening, Listening, &str); 2] = [
    ((true, false), (true, true), "`x` starts listening"),
    ((true, false), (false, false), "`a` stops listening"),
];

/// A window opened on `before` and rebuilt with `after`.
fn rebuilt(before: (bool, bool), after: (bool, bool)) -> LayoutWindow {
    let mut lw = window(page(before));
    rebuild(&mut lw, page(after));
    lw
}

/// The nodes a click at `point` lands on, front to back.
fn hits(lw: &LayoutWindow, point: LogicalPosition) -> Vec<NodeId> {
    let mut tester = CpuHitTester::new();
    tester.rebuild_from_layout(&lw.layout_results);
    tester
        .hit_test(point)
        .into_iter()
        .filter(|(dom, _)| *dom == DomId::ROOT_ID)
        .map(|(_, node)| node)
        .collect()
}

fn centre(r: LogicalRect) -> LogicalPosition {
    LogicalPosition::new(r.origin.x + r.size.width / 2.0, r.origin.y + r.size.height / 2.0)
}

/// Every node hit-test area of the window's display list: its tag and its
/// rect, sorted.
fn node_areas(lw: &LayoutWindow) -> Vec<(u64, [f32; 4])> {
    let mut areas: Vec<(u64, [f32; 4])> = lw.layout_results[&DomId::ROOT_ID]
        .display_list
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayListItem::HitTestArea { bounds, tag } if tag.1 == TAG_TYPE_DOM_NODE => {
                let r = bounds.0;
                Some((tag.0, [r.origin.x, r.origin.y, r.size.width, r.size.height]))
            }
            _ => None,
        })
        .collect();
    areas.sort_by(|p, q| p.partial_cmp(q).expect("finite rects"));
    areas
}

/// The id of the box of class `class`.
fn id_of(lw: &LayoutWindow, class: &str) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(node_with_class(lw, class))),
    }
}

/// A click on each box after the rebuild lands on that box, as in a fresh
/// window opened on the page, and the box it lands on holds the handler the
/// page gave it.
#[test]
fn a_click_lands_where_a_fresh_window_puts_it_after_a_rebuild_that_changes_who_listens() {
    for (before, after, what) in REBUILDS {
        let lw = rebuilt(before, after);
        let fresh = window(page(after));
        for class in ["a", "x", "b", "c"] {
            let node = node_with_class(&lw, class);
            let point = centre(rect(&lw, node));
            let landed = hits(&lw, point);
            assert_eq!(
                landed.first(),
                Some(&node),
                "{what}: a click on `{class}` lands on it: {landed:?}"
            );
            assert_eq!(
                landed,
                hits(&fresh, point),
                "{what}: a click on `{class}` lands as in a fresh window"
            );
        }
        let c = node_with_class(&lw, "c");
        assert!(
            lw.layout_results[&DomId::ROOT_ID]
                .styled_dom
                .node_data
                .as_container()[c]
                .get_callbacks()
                .iter()
                .any(|cb| cb.event == EventFilter::Hover(HoverEventFilter::Click)),
            "{what}: the box a click on `c` lands on holds `c`'s handler"
        );
    }
}

/// The patched display list carries the hit-test areas a fresh window paints:
/// one for the box that started listening, none for the one that stopped.
#[test]
fn a_patched_display_list_carries_the_hit_test_areas_of_who_listens_now() {
    for (before, after, what) in REBUILDS {
        let lw = rebuilt(before, after);
        assert!(
            lw.layout_cache.last_build_was_patched,
            "harness: {what}: the rebuild patches the display list"
        );
        let fresh = window(page(after));
        assert_eq!(
            node_areas(&lw),
            node_areas(&fresh),
            "{what}: the hit-test areas of the page as it is now"
        );
        for class in ["a", "x", "b", "c"] {
            assert_eq!(
                lw.get_node_hit_test_bounds(id_of(&lw, class)),
                fresh.get_node_hit_test_bounds(id_of(&fresh, class)),
                "{what}: the hit-test bounds of `{class}`"
            );
        }
    }
}

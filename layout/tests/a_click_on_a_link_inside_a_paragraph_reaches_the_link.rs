//! A click on a link inside a paragraph reaches the link.
//!
//! AzMail exploration (scripts/ideas/AZMAIL_EXPLORATION_2026_09_30.md, 1.4
//! "Links", gap E10; design notes in scripts/ideas/todo-ledger.md E10): a
//! non-replaced inline element (`<a>`, `<span>`) has no box - its text is
//! laid out in the paragraph's inline formatting context - so the hit
//! tester had no rect for it. A click on a link in a mail reached the
//! paragraph: no callback on the `<a>`, no pointer cursor, and an app that
//! listens on the mail's container found no `<a>` among the hovered nodes to
//! read the `href` from.
//!
//! The link's fragments are its text runs, one per line: a link wrapped
//! over two lines is hit on both, and the paragraph's text next to it is
//! not the link.
//!
//! Not compiled by the author (house rule); expected RED before the fix.

use azul_core::{
    dom::{DomId, DomNodeId, IdOrClass, NodeData, NodeId},
    geom::{LogicalPosition, LogicalSize},
    hit_test::TAG_TYPE_CURSOR,
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
    window::MouseCursorType,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    headless::{convert_cpu_hit_test_to_full, CpuHitTester},
    hit_test::CursorTypeHitTest,
    solver3::display_list::DisplayListItem,
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// A narrow column, so the link wraps onto a second line.
const MAIL: &str = "<html><head></head><body>\
<div style=\"width: 260px; font-size: 16px; line-height: 20px;\">\
<p id=\"p\" style=\"margin: 0;\">Read the <a id=\"link\" href=\"https://example.org/q3\">\
quarterly report for the third quarter</a> before Friday.</p>\
</div></body></html>";

struct Laid {
    lw: LayoutWindow,
    tester: CpuHitTester,
    p: NodeId,
    link: NodeId,
    link_text: NodeId,
    read_the: NodeId,
}

fn node_with_id(sd: &StyledDom, id: &str) -> NodeId {
    sd.node_data
        .as_ref()
        .iter()
        .position(|nd: &NodeData| {
            nd.get_ids_and_classes()
                .iter()
                .any(|c| matches!(c, IdOrClass::Id(s) if s.as_str() == id))
        })
        .map(NodeId::new)
        .unwrap_or_else(|| panic!("no element with id {id}"))
}

fn laid_out() -> Laid {
    let parsed = azul_layout::xml::parse_xml(MAIL).expect("the mail parses");
    let styled = StyledDom::create_from_dom(azul_layout::xml::dom_from_parsed_xml(parsed));
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(640.0, 300.0);
    lw.current_window_state = ws.clone();
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    let mut dbg = Some(Vec::new());
    lw.layout_and_generate_display_list(styled, &ws, &rr, &sc, &mut dbg)
        .expect("the mail lays out");
    let mut tester = CpuHitTester::new();
    tester.rebuild_from_layout(&lw.layout_results);
    let sd = &lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("laid out")
        .styled_dom;
    let p = node_with_id(sd, "p");
    let link = node_with_id(sd, "link");
    // The paragraph's children: "Read the ", <a>, " before Friday."; the
    // link's only child is its text.
    let read_the = NodeId::new(p.index() + 1);
    let link_text = NodeId::new(link.index() + 1);
    Laid {
        lw,
        tester,
        p,
        link,
        link_text,
        read_the,
    }
}

/// The centres of the text runs of `text_node`, from the display list's
/// cursor areas (one per line and style run).
fn run_centres(laid: &Laid, text_node: NodeId) -> Vec<LogicalPosition> {
    let result = laid
        .lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("laid out");
    result
        .display_list
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayListItem::HitTestArea { bounds, tag }
                if tag.1 & 0xFF00 == TAG_TYPE_CURSOR
                    && (tag.0 & 0xFFFF_FFFF) as usize == text_node.index() =>
            {
                let r = bounds.0;
                Some(LogicalPosition::new(
                    r.origin.x + r.size.width / 2.0,
                    r.origin.y + r.size.height / 2.0,
                ))
            }
            _ => None,
        })
        .collect()
}

fn hits(laid: &Laid, at: LogicalPosition) -> Vec<NodeId> {
    laid.tester
        .hit_test(at)
        .into_iter()
        .filter(|(d, _)| *d == DomId::ROOT_ID)
        .map(|(_, n)| n)
        .collect()
}

#[test]
fn every_line_of_a_wrapped_link_is_the_link() {
    let laid = laid_out();
    let centres = run_centres(&laid, laid.link_text);
    assert!(
        centres.len() >= 2,
        "the link wraps onto two lines in a 260px column: {centres:?}"
    );
    for at in centres {
        let got = hits(&laid, at);
        assert!(
            got.contains(&laid.link),
            "a click on the link's text at {at:?} reaches the <a> (node {}): hits {got:?}",
            laid.link.index()
        );
        assert!(
            got.contains(&laid.p),
            "...and, bubbling, the paragraph: hits {got:?}"
        );
    }
}

#[test]
fn the_paragraphs_own_text_is_not_the_link() {
    let laid = laid_out();
    let centres = run_centres(&laid, laid.read_the);
    assert!(!centres.is_empty(), "\"Read the \" paints");
    for at in centres {
        let got = hits(&laid, at);
        assert!(got.contains(&laid.p), "hits {got:?}");
        assert!(
            !got.contains(&laid.link),
            "the text before the link is not the link: hits {got:?}"
        );
    }
}

#[test]
fn a_link_shows_the_pointer_and_hands_the_app_its_href() {
    let laid = laid_out();
    let at = run_centres(&laid, laid.link_text)
        .into_iter()
        .next()
        .expect("the link paints");
    let raw = laid
        .tester
        .hit_test_scrolled(at, &|_, _| None, &|_, _| None);
    let full = convert_cpu_hit_test_to_full(
        &laid.tester,
        &raw,
        None,
        &laid.lw.layout_results,
        at,
        &|_, _| None,
        &|_, _| None,
    );
    // What an app's callback on the mail container reads
    // (`CallbackInfo::get_hovered_nodes`): the link, nearest first.
    let hovered = full.hovered_node_ids();
    let link = DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(laid.link)),
    };
    assert_eq!(
        hovered.first(),
        Some(&link),
        "the <a> is the frontmost node under the pointer: {hovered:?}"
    );
    let sd = &laid
        .lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("laid out")
        .styled_dom;
    let href = sd.node_data.as_container()[laid.link]
        .attributes()
        .as_ref()
        .iter()
        .find_map(|a| match a {
            azul_core::dom::AttributeType::Href(h) => Some(h.as_str().to_string()),
            _ => None,
        });
    assert_eq!(href.as_deref(), Some("https://example.org/q3"));
    assert_eq!(
        CursorTypeHitTest::new(&full, &laid.lw).cursor_icon,
        MouseCursorType::Hand,
        "a link shows the pointer (`:link {{ cursor: pointer }}`)"
    );
}

//! A frame of a ribbon opening lays out the boxes whose size the opening
//! changes, and nothing else.
//!
//! AzDrive's window: a ribbon whose content band opens by a tween of its
//! height, over a row holding the navigation pane and the folder list. Every
//! frame of the opening resizes the band and the row under it: the ribbon's
//! groups are stretched to the band, the pane and the list to the row. What
//! they hold keeps its size - a group's row of buttons and its caption row,
//! the pane's places, the list's rows - so it keeps its layout: no width is
//! asked again, no text is laid out again, and its display list items are
//! copied. Only the boxes the band resizes, and their ancestors, are laid out.

use azul_core::dom::{Dom, DomId, DomNodeId, NodeId};
use azul_css::props::{layout::LayoutHeight, property::CssProperty};
use azul_layout::{
    overlay::ContentChange,
    probe::{Event, Probe},
    window::LayoutWindow,
};

use crate::a_one_box_slide_does_not_re_lay_out_the_page::{
    count, relayout, restore_ambient_recording, window,
};

/// The band's height while the ribbon is closed, and once it is open.
const CLOSED_PX: isize = 40;
const OPEN_PX: isize = 106;

// ---- the window ----

/// A ribbon command: its icon over its label.
fn button(group: usize, b: usize) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; align-items: center; padding: 2px 4px;")
        .with_child(Dom::create_div().with_css("width: 32px; height: 32px; background: #8899aa;"))
        .with_child(
            Dom::create_div_with_text(format!("Command {group}.{b}"))
                .with_css("font-size: 12px;"),
        )
}

/// A ribbon group as `Ribbon` builds one: a column of its row of buttons (68
/// px tall, its buttons at the top) and its caption row (18 px).
fn group(g: usize) -> Dom {
    let mut buttons = Dom::create_div().with_css(
        "display: flex; flex-direction: row; height: 68px; align-items: flex-start; \
         flex-shrink: 0;",
    );
    for b in 0..3 {
        buttons = buttons.with_child(button(g, b));
    }
    Dom::create_div()
        .with_css(
            "display: flex; flex-direction: column; flex-shrink: 0; padding: 3px 2px 0px 2px; \
             border-right: 1px solid #c8c8c8;",
        )
        .with_child(buttons)
        .with_child(
            Dom::create_div()
                .with_css(
                    "display: flex; flex-direction: row; height: 18px; align-items: center; \
                     justify-content: center; flex-shrink: 0;",
                )
                .with_child(
                    Dom::create_div_with_text(format!("Group {g}")).with_css("font-size: 11px;"),
                ),
        )
}

/// The ribbon: its tab row over its content band, `band_px` tall, the band
/// holding `groups` groups in a row that fills it. The band tweens its height.
fn ribbon(groups: usize, band_px: f32) -> Dom {
    let mut tabs = Dom::create_div().with_css(
        "display: flex; flex-direction: row; height: 28px; align-items: center; flex-shrink: 0;",
    );
    for tab in ["File", "Home", "Share", "View"] {
        tabs = tabs.with_child(Dom::create_div_with_text(tab).with_css("padding: 0px 10px;"));
    }
    let mut row = Dom::create_div()
        .with_css("display: flex; flex-direction: row; flex-grow: 1; align-items: stretch;");
    for g in 0..groups {
        row = row.with_child(group(g));
    }
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-shrink: 0; background: #f4f0e8;")
        .with_child(tabs)
        .with_child(
            Dom::create_div()
                .with_class("band".into())
                .with_css(&format!(
                    "display: flex; flex-direction: column; height: {band_px}px; \
                     overflow-y: hidden; padding: 4px 6px 0px 6px; \
                     animation: height 200ms linear;"
                ))
                .with_child(row),
        )
}

/// The navigation pane: a scrolling column of `places` places, each an icon
/// and its name, as tall as its text.
fn pane(places: usize) -> Dom {
    let mut pane = Dom::create_div().with_css(
        "display: flex; flex-direction: column; width: 228px; flex-shrink: 0; \
         overflow-y: auto; background: #faf8f4;",
    );
    for p in 0..places {
        pane = pane.with_child(
            Dom::create_div()
                .with_css(
                    "display: flex; flex-direction: row; align-items: center; \
                     padding: 4px 8px; flex-shrink: 0;",
                )
                .with_child(Dom::create_div().with_css(
                    "width: 16px; height: 16px; margin-right: 6px; background: #6688aa;",
                ))
                .with_child(Dom::create_div_with_text(format!("Place {p}"))),
        );
    }
    pane
}

/// The folder list: a scrolling block of `rows` rows in block flow, each a
/// name, a size and a date, `row_css` added to every row's style.
fn list(rows: usize, row_css: &str) -> Dom {
    let mut list = Dom::create_div()
        .with_css("display: block; flex-grow: 1; overflow-y: auto; background: #ffffff;");
    for r in 0..rows {
        list = list.with_child(
            Dom::create_div()
                .with_css(&format!("display: flex; flex-direction: row; padding: 3px 6px; {row_css}"))
                .with_child(
                    Dom::create_div_with_text(format!("Document {r}.pdf"))
                        .with_css("flex-grow: 1;"),
                )
                .with_child(
                    Dom::create_div_with_text(format!("{} KB", 10 + r)).with_css("width: 80px;"),
                )
                .with_child(Dom::create_div_with_text("24.10.2026").with_css("width: 120px;")),
        );
    }
    list
}

/// The window: the ribbon over the row of the pane and the list, the two
/// filling the window's height under the ribbon.
fn page(groups: usize, places: usize, rows: usize, band_px: isize) -> Dom {
    page_with(groups, places, rows, band_px as f32, "")
}

/// [`page`] with the band `band_px` tall and `row_css` added to the style of
/// every row of the list.
fn page_with(groups: usize, places: usize, rows: usize, band_px: f32, row_css: &str) -> Dom {
    Dom::create_html().with_child(
        Dom::create_body()
            .with_css("display: flex; flex-direction: column; height: 100%; margin: 0px;")
            .with_child(ribbon(groups, band_px))
            .with_child(
                Dom::create_div()
                    .with_css("display: flex; flex-direction: row; flex-grow: 1; min-height: 0px;")
                    .with_child(pane(places))
                    .with_child(list(rows, row_css)),
            ),
    )
}

/// The ribbon alone over an empty box filling the rest of the window.
fn ribbon_page(groups: usize, band_px: isize) -> Dom {
    Dom::create_html().with_child(
        Dom::create_body()
            .with_css("display: flex; flex-direction: column; height: 100%; margin: 0px;")
            .with_child(ribbon(groups, band_px as f32))
            .with_child(Dom::create_div().with_css("flex-grow: 1; background: #ffffff;")),
    )
}

// ---- the harness ----

fn node_with_class(lw: &LayoutWindow, class: &str) -> NodeId {
    let sd = &lw.layout_results[&DomId::ROOT_ID].styled_dom;
    let node_data = sd.node_data.as_container();
    (0..node_data.len())
        .map(NodeId::new)
        .find(|n| {
            node_data[*n]
                .get_ids_and_classes()
                .iter()
                .any(|c| c.as_class() == Some(class))
        })
        .expect("the node exists")
}

/// Every node of `lw` is laid out where `fresh` (the same window, laid out
/// from scratch) puts it.
fn assert_same_layout(lw: &LayoutWindow, fresh: &LayoutWindow, what: &str) {
    let nodes = |w: &LayoutWindow| {
        w.layout_results[&DomId::ROOT_ID]
            .styled_dom
            .node_data
            .as_container()
            .len()
    };
    assert_eq!(nodes(lw), nodes(fresh), "harness: {what}: the same window");
    let close = |a: f32, b: f32| (a - b).abs() < 0.01;
    for n in 0..nodes(lw) {
        let id = DomNodeId {
            dom: DomId::ROOT_ID,
            node: Some(NodeId::new(n)).into(),
        };
        let (is, fresh_box) = (lw.get_node_layout_rect(id), fresh.get_node_layout_rect(id));
        let same = match (is, fresh_box) {
            (Some(a), Some(b)) => {
                close(a.origin.x, b.origin.x)
                    && close(a.origin.y, b.origin.y)
                    && close(a.size.width, b.size.width)
                    && close(a.size.height, b.size.height)
            }
            (None, None) => true,
            _ => false,
        };
        assert!(
            same,
            "{what}: node {n} is laid out at {is:?}, a fresh window puts it at {fresh_box:?}"
        );
    }
}

/// A window showing `page` (its ribbon closed), the band's opening tween
/// seeded (the write of its open height), and one frame of it run: the
/// frames measured after it are mid-tween frames like every other.
fn opening(page: Dom) -> LayoutWindow {
    let mut lw = window(page);
    let band = node_with_class(&lw, "band");
    let _ = lw.apply_content_change(ContentChange::NodeCss {
        dom_id: DomId::ROOT_ID,
        node_id: band,
        props: vec![CssProperty::const_height(LayoutHeight::const_px(OPEN_PX))],
        override_only: false,
    });
    assert!(
        lw.css_transitions.iter().any(|t| t.node == band),
        "harness: the write of the band's height seeds its tween"
    );
    let _ = frame(&mut lw);
    lw
}

/// One frame of the opening: the tween steps 16 ms and the window lays out
/// again. Returns the spans of that layout.
fn frame(lw: &mut LayoutWindow) -> Vec<Event> {
    let _ = lw.tick_animations(0.016);
    assert!(
        lw.take_transition_relayout(),
        "harness: a step of the band's height owes a relayout"
    );
    Probe::set_recording(true);
    let _ = Probe::drain();
    relayout(lw);
    let events = Probe::drain();
    restore_ambient_recording();
    assert!(
        lw.layout_cache.last_reconcile_was_skipped,
        "harness: a frame of the tween lays out the retained tree"
    );
    events
}

/// What one frame laid out again, counted from its spans.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FrameWork {
    /// Inline formatting contexts laid out (`layout_ifc`, at any width).
    inline_layouts: usize,
    /// Text broken into lines again.
    text_flows: usize,
    /// Width-only questions a flex row answered by running its algorithm.
    width_only_rows: usize,
    /// Boxes in block flow whose size the per-node cache did not hold.
    block_size_misses: usize,
    /// Flex and grid items laid out again: taffy cache misses, and final
    /// layouts that could not be served.
    flex_items_laid_out: usize,
    /// Boxes whose intrinsic sizes were computed again.
    intrinsic_computes: usize,
    /// Box visits of the display list builder that painted again instead of
    /// copying the box's items from the last list.
    items_painted_again: usize,
}

fn work(events: &[Event]) -> FrameWork {
    let visits = count(events, "dl_bg_border") + count(events, "dl_content");
    let copies = count(events, "dl_patch_copy") + count(events, "dl_patch_empty");
    FrameWork {
        inline_layouts: count(events, "ifc_visit_definite")
            + count(events, "ifc_visit_min")
            + count(events, "ifc_visit_max"),
        text_flows: count(events, "text_layout_flow"),
        width_only_rows: count(events, "taffy_width_only_row"),
        block_size_misses: count(events, "size_cache_miss"),
        flex_items_laid_out: count(events, "taffy_cache_get_miss")
            + count(events, "taffy_final_layout_stale"),
        intrinsic_computes: count(events, "intrinsic_node_compute"),
        items_painted_again: visits.saturating_sub(copies),
    }
}

/// The work of three mid-tween frames of the opening of `page`.
fn frames_of(page: Dom) -> Vec<FrameWork> {
    let mut lw = opening(page);
    (0..3).map(|_| work(&frame(&mut lw))).collect()
}

// ---- the tests ----

/// The ribbon's groups are stretched to the band on every frame, and laid
/// out again at their new height; the rows they hold - the row of buttons,
/// the caption row - keep their size, and the width they answered when the
/// group asked for it. The question is not asked of them again: a row
/// answering it runs the whole row algorithm over its items, and marks its
/// own layout stale. Nor is it asked again of the row holding the groups:
/// it is taller each frame, but its width does not follow its height.
#[test]
fn a_ribbon_frame_asks_no_row_its_width_again() {
    let _serialised = crate::probe_lock();
    if !Probe::enabled() {
        eprintln!("[a_growing_ribbon] the probe is compiled out: nothing to count");
        return;
    }
    let frames = frames_of(ribbon_page(8, CLOSED_PX));
    for (i, w) in frames.iter().enumerate() {
        assert_eq!(
            w.width_only_rows, 0,
            "frame {i}: {} rows ran their algorithm to answer a width they answered before: \
             {w:?}",
            w.width_only_rows
        );
        assert_eq!(
            (w.inline_layouts, w.text_flows),
            (0, 0),
            "frame {i}: the ribbon's labels kept their size and lines, none is laid out again: \
             {w:?}"
        );
    }
}

/// The navigation pane under the ribbon is shorter on every frame and laid
/// out again at its height; its places keep their size and their layout. A
/// frame costs the same on a pane with twice the places.
#[test]
fn a_pane_under_a_growing_ribbon_lays_out_none_of_its_places() {
    let _serialised = crate::probe_lock();
    if !Probe::enabled() {
        eprintln!("[a_growing_ribbon] the probe is compiled out: nothing to count");
        return;
    }
    let short = frames_of(page(1, 30, 0, CLOSED_PX));
    let long = frames_of(page(1, 60, 0, CLOSED_PX));
    for (i, (s, l)) in short.iter().zip(&long).enumerate() {
        assert_eq!(
            (l.inline_layouts, l.text_flows),
            (0, 0),
            "frame {i}: the pane's places only moved with the pane - none is laid out again: \
             {l:?}"
        );
        assert_eq!(
            s.width_only_rows, l.width_only_rows,
            "frame {i}: a pane with twice the places asked more rows their width ({s:?} / \
             {l:?}) - every place kept its width"
        );
    }
}

/// The folder list under the ribbon is shorter on every frame; its rows sit
/// in its block flow, which hands them the list's height to resolve
/// percentages against. They have none: their layout does not follow that
/// height, and the per-node cache serves it whatever the list's height is.
/// A frame costs the same on a list twice as long.
#[test]
fn a_list_under_a_growing_ribbon_keeps_the_layout_of_its_rows() {
    let _serialised = crate::probe_lock();
    if !Probe::enabled() {
        eprintln!("[a_growing_ribbon] the probe is compiled out: nothing to count");
        return;
    }
    let short = frames_of(page(1, 0, 20, CLOSED_PX));
    let long = frames_of(page(1, 0, 40, CLOSED_PX));
    for (i, (s, l)) in short.iter().zip(&long).enumerate() {
        assert_eq!(
            (l.inline_layouts, l.text_flows),
            (0, 0),
            "frame {i}: the list's rows only moved with the list - no text of theirs is laid out \
             again: {l:?}"
        );
        assert_eq!(
            s.block_size_misses, l.block_size_misses,
            "frame {i}: a list twice as long sized more rows again ({s:?} / {l:?}) - every row \
             kept its size"
        );
    }
}

/// The whole of AzDrive's window: a frame lays out the band, the boxes the
/// band resizes and their ancestors - their intrinsic sizes, their flex
/// lines - and copies the display list items of everything that only moved.
/// The same on a window with twice the places and rows. And the window the
/// tween ends on is laid out exactly as a window opened with the band open.
#[test]
fn an_azdrive_frame_lays_out_only_what_the_ribbon_resizes() {
    let _serialised = crate::probe_lock();
    if !Probe::enabled() {
        eprintln!("[a_growing_ribbon] the probe is compiled out: nothing to count");
        return;
    }
    let (groups, places, rows) = (8, 30, 40);
    let mut lw = opening(page(groups, places, rows, CLOSED_PX));
    let mut frames = Vec::new();
    for i in 0..3 {
        let w = work(&frame(&mut lw));
        let mismatches = lw.verify_patched_display_list(DomId::ROOT_ID);
        assert!(
            mismatches.is_empty(),
            "frame {i}: the patched list must equal a wholesale build of the same layout: \
             {mismatches:?}"
        );
        frames.push(w);
    }
    let twice = frames_of(page(groups, 2 * places, 2 * rows, CLOSED_PX));
    for (i, (w, w2)) in frames.iter().zip(&twice).enumerate() {
        assert_eq!(
            (w.inline_layouts, w.text_flows),
            (0, 0),
            "frame {i}: nothing that kept its size is laid out again: {w:?}"
        );
        // One row is asked its width at a new height on every frame and
        // answers it again: the row of the pane and the list, stretched under
        // the ribbon. It holds scroll containers, whose width may follow
        // their height (a bar appears), so its answer is not kept across
        // heights. No other row is asked again.
        assert!(
            w.width_only_rows <= 1,
            "frame {i}: {} rows answered their width again - only the row holding the pane \
             and the list was asked at a new height: {w:?}",
            w.width_only_rows
        );
        // The band and its ancestors: the band, the ribbon, the body, the
        // root (and an anonymous box or two around them).
        assert!(
            w.intrinsic_computes <= 8,
            "frame {i}: {} boxes had their intrinsic sizes computed again - only the band's \
             ancestors depend on its height: {w:?}",
            w.intrinsic_computes
        );
        assert_eq!(
            (
                w.block_size_misses,
                w.flex_items_laid_out,
                w.items_painted_again,
                w.width_only_rows
            ),
            (
                w2.block_size_misses,
                w2.flex_items_laid_out,
                w2.items_painted_again,
                w2.width_only_rows
            ),
            "frame {i}: a window with twice the places and rows did more work ({w:?} / {w2:?}) \
             - what only moved keeps its layout and its items"
        );
    }

    // Run the tween out and compare with a window opened open.
    for _ in 0..64 {
        if lw.css_transitions.is_empty() {
            break;
        }
        let _ = lw.tick_animations(0.016);
        if lw.take_transition_relayout() {
            relayout(&mut lw);
        }
    }
    assert!(
        lw.css_transitions.is_empty(),
        "harness: the band's tween ends"
    );
    let fresh = window(page(groups, places, rows, OPEN_PX));
    assert_same_layout(&lw, &fresh, "the band opened");
}
}

/// What a frame keeps is what a fresh window lays out: at three heights of
/// the band mid-tween, every box of the window is where a window opened with
/// the band at that height puts it. The list's rows are sized by the list
/// here (`height: 6%` of its height, which the band moves): they must follow
/// it, while everything that only moved keeps its layout.
#[test]
fn every_frame_of_the_opening_lays_out_as_a_fresh_window() {
    let _serialised = crate::probe_lock();
    let (groups, places, rows) = (4, 12, 12);
    let sized_by_the_list = "height: 6%;";
    let mut lw = opening(page_with(
        groups,
        places,
        rows,
        CLOSED_PX as f32,
        sized_by_the_list,
    ));
    for i in 0..3 {
        let _ = frame(&mut lw);
        let band = node_with_class(&lw, "band");
        let band_box = lw
            .get_node_layout_rect(DomNodeId {
                dom: DomId::ROOT_ID,
                node: Some(band).into(),
            })
            .expect("the band is laid out");
        // The band's height: its border box less its 4 px top padding.
        let band_px = band_box.size.height - 4.0;
        let fresh = window(page_with(groups, places, rows, band_px, sized_by_the_list));
        assert_same_layout(&lw, &fresh, &format!("frame {i}, the band {band_px} px tall"));
    }
}

//! A label that is measured before it is laid out flows its text only when
//! its final width changes.
//!
//! A flex container asks its items how tall their content is at min-content
//! and at max-content width before it lays them out at their final width, and
//! it asks again in every pass that lays the container out anew. A hover that
//! restyles a place of AzDrive's navigation pane is such a pass: the place is
//! built anew, and the label it holds is measured again. Each measure is a
//! layout of the label's text at that width. They are kept beside the label's
//! final layout, so a measure asked again costs no flow of the text, and the
//! final layout - the one painted - is never replaced by a measure.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId},
    geom::LogicalRect,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    probe::Probe,
    solver3::{display_list::DisplayListItem, LayoutNodeId},
    text3::cache::AvailableSpace,
    window::LayoutWindow,
};

use crate::a_one_box_slide_does_not_re_lay_out_the_page::{
    count, restore_ambient_recording, window,
};

/// The width of the pane's header box, wider than any place: the pane keeps
/// this width whatever a hover does to a place.
const HEADER_PX: f32 = 260.0;

// ---- the window ----

/// A place of the navigation pane: a column holding the place's name. The
/// place under the pointer is tinted and indented by 12 px on the left, where
/// AzDrive shows the handle of the hovered row.
fn place(p: usize, name: &str, hovered: bool) -> Dom {
    let css = if hovered {
        "display: flex; flex-direction: column; flex-shrink: 0; padding: 4px 8px 4px 20px; \
         background: #dde4ee;"
    } else {
        "display: flex; flex-direction: column; flex-shrink: 0; padding: 4px 8px;"
    };
    Dom::create_div().with_css(css).with_child(
        Dom::create_div_with_text(name.to_string()).with_class(format!("label-{p}").into()),
    )
}

/// How the pane next to the list is sized.
#[derive(Debug, Clone, Copy)]
enum Pane {
    /// As wide as its header box, which no place outgrows.
    Fixed,
    /// As wide as its widest place: its max-content width.
    Content,
    /// Squeezed by a list wider than the window to its min-content width:
    /// every label wraps at its longest word.
    Squeezed,
}

/// The window: the navigation pane holding one place per name, `hovered`
/// the place under the pointer, beside the folder list. The pane is a flex
/// column, so the window's row measures it at min- and max-content width,
/// and the pane measures its places' labels with it.
fn page(names: &[String], hovered: Option<usize>, pane: Pane) -> Dom {
    let mut column = Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-shrink: 1; background: #faf8f4;");
    if matches!(pane, Pane::Fixed) {
        column = column.with_child(
            Dom::create_div()
                .with_css(&format!("width: {HEADER_PX}px; height: 24px; flex-shrink: 0;")),
        );
    }
    for (p, name) in names.iter().enumerate() {
        column = column.with_child(place(p, name, hovered == Some(p)));
    }
    let list = if matches!(pane, Pane::Squeezed) {
        "width: 900px; flex-shrink: 0; background: #ffffff;"
    } else {
        "flex-grow: 1; background: #ffffff;"
    };
    Dom::create_html().with_child(
        Dom::create_body()
            .with_css("display: flex; flex-direction: row; height: 100%; margin: 0px;")
            .with_child(column)
            .with_child(Dom::create_div().with_css(list)),
    )
}

fn names(places: usize) -> Vec<String> {
    (0..places).map(|p| format!("Place number {p}")).collect()
}

// ---- the harness ----

/// Lay the window out with a rebuilt page, as an app's `RefreshDom` hands
/// one over: the reconcile keeps what did not change.
pub(crate) fn rebuild(lw: &mut LayoutWindow, mut dom: Dom) {
    let styled_dom = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let window_state = lw.current_window_state.clone();
    lw.layout_and_generate_display_list(
        styled_dom,
        &window_state,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the rebuilt page lays out");
}

pub(crate) fn node_with_class(lw: &LayoutWindow, class: &str) -> NodeId {
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

pub(crate) fn rect(lw: &LayoutWindow, node: NodeId) -> LogicalRect {
    lw.get_node_layout_rect(DomNodeId {
        dom: DomId::ROOT_ID,
        node: Some(node).into(),
    })
    .expect("laid out")
}

/// Every glyph the window paints: `(glyph, x, y)`.
pub(crate) fn painted_glyphs(lw: &LayoutWindow) -> Vec<(u32, f32, f32)> {
    lw.layout_results[&DomId::ROOT_ID]
        .display_list
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayListItem::Text { glyphs, .. } => Some(glyphs),
            _ => None,
        })
        .flatten()
        .map(|g| (g.index, g.point.x, g.point.y))
        .collect()
}

/// The final inline layout of every element that holds one: its DOM node,
/// the width it was laid out at, and where it put each of its items.
fn final_inline_layouts(lw: &LayoutWindow) -> Vec<(usize, AvailableSpace, Vec<(f32, f32)>)> {
    let tree = lw
        .layout_cache
        .tree
        .as_ref()
        .expect("the window keeps its layout tree");
    let mut layouts: Vec<_> = tree
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(i, hot)| {
            let dom = hot.dom_node_id?;
            let cached = tree
                .warm(LayoutNodeId::new(i))?
                .inline_layout_result
                .as_deref()?;
            let items: Vec<(f32, f32)> = cached
                .materialized()
                .items
                .iter()
                .map(|item| (item.position.x, item.position.y))
                .collect();
            Some((dom.index(), cached.available_width, items))
        })
        .collect();
    layouts.sort_by_key(|(dom, _, _)| *dom);
    layouts
}

/// Every box, every painted glyph and every final inline layout of `lw` is
/// what a fresh window, which keeps nothing, lays out for `page`.
pub(crate) fn assert_lays_out_like_a_fresh_window(lw: &LayoutWindow, page: Dom, what: &str) {
    let fresh = window(page);
    let nodes = |w: &LayoutWindow| {
        w.layout_results[&DomId::ROOT_ID]
            .styled_dom
            .node_data
            .as_container()
            .len()
    };
    assert_eq!(nodes(lw), nodes(&fresh), "harness: {what}: the same page");
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

    let (painted, fresh_painted) = (painted_glyphs(lw), painted_glyphs(&fresh));
    assert_eq!(
        painted.len(),
        fresh_painted.len(),
        "{what}: the window paints {} glyphs, a fresh window {}",
        painted.len(),
        fresh_painted.len()
    );
    for (a, b) in painted.iter().zip(&fresh_painted) {
        assert!(
            a.0 == b.0 && close(a.1, b.1) && close(a.2, b.2),
            "{what}: a glyph paints as {a:?}, a fresh window paints it as {b:?}"
        );
    }

    let (layouts, fresh_layouts) = (final_inline_layouts(lw), final_inline_layouts(&fresh));
    assert!(
        !fresh_layouts.is_empty(),
        "harness: {what}: the labels hold inline layouts"
    );
    assert_eq!(
        layouts.iter().map(|(dom, _, _)| *dom).collect::<Vec<_>>(),
        fresh_layouts.iter().map(|(dom, _, _)| *dom).collect::<Vec<_>>(),
        "{what}: the same elements hold a final inline layout as in a fresh window"
    );
    let same_width = |a: AvailableSpace, b: AvailableSpace| match (a, b) {
        (AvailableSpace::Definite(x), AvailableSpace::Definite(y)) => (x - y).abs() < 0.1,
        (x, y) => x == y,
    };
    for ((dom, width, items), (_, fresh_width, fresh_items)) in layouts.iter().zip(&fresh_layouts)
    {
        assert!(
            same_width(*width, *fresh_width),
            "{what}: node {dom}'s final inline layout is at {width:?}, a fresh window's at \
             {fresh_width:?}"
        );
        assert_eq!(
            items.len(),
            fresh_items.len(),
            "{what}: node {dom}'s final inline layout holds {} items, a fresh window's {}",
            items.len(),
            fresh_items.len()
        );
        for (a, b) in items.iter().zip(fresh_items) {
            assert!(
                close(a.0, b.0) && close(a.1, b.1),
                "{what}: node {dom} places an item at {a:?}, a fresh window at {b:?}"
            );
        }
    }
}

// ---- the tests ----

/// Hovering a place lays the pane out anew: the place is built fresh and its
/// label is measured again at min- and max-content width, then laid out at
/// its final width, 12 px narrower under the hover's indent. That is one flow
/// of the label's text, at its new width. The measures find what the label's
/// earlier measures kept; before they were kept, each measure took the place
/// of the label's final layout and flowed the text, and the final layout
/// flowed it once more.
#[test]
fn a_hovered_label_flows_its_text_once_however_often_it_is_measured() {
    let _serialised = crate::probe_lock();
    if !Probe::enabled() {
        eprintln!("[a_measured_label] the probe is compiled out: nothing to count");
        return;
    }
    let names = names(8);
    let mut lw = window(page(&names, None, Pane::Fixed));
    let label = node_with_class(&lw, "label-3");
    let before = rect(&lw, label);

    Probe::set_recording(true);
    let _ = Probe::drain();
    rebuild(&mut lw, page(&names, Some(3), Pane::Fixed));
    let events = Probe::drain();
    restore_ambient_recording();

    let after = rect(&lw, label);
    assert!(
        (after.origin.x - before.origin.x - 12.0).abs() < 0.5
            && (before.size.width - after.size.width - 12.0).abs() < 0.5,
        "harness: the hover indents the label by 12 px and narrows it as much, {before:?} -> \
         {after:?}"
    );
    let measures = count(&events, "ifc_visit_min") + count(&events, "ifc_visit_max");
    assert!(
        measures >= 1,
        "harness: the hover measures the label at min- or max-content width"
    );
    let flows = count(&events, "text_layout_flow");
    assert!(
        flows <= 1,
        "the hover flowed text {flows} times for one label laid out at a new width after {measures} \
         measures: a measure took the place of the label's final layout"
    );
    assert_lays_out_like_a_fresh_window(&lw, page(&names, Some(3), Pane::Fixed), "the hover");
}

/// The labels' measures and final layouts are kept apart, and what is kept
/// stays true: through hovers that indent a place, a name that grows and
/// shrinks again (its measures describe the old text), and a pane that is as
/// wide as its widest label or squeezed until every label wraps, every
/// label's final layout is the one a fresh window lays out.
#[test]
fn every_label_is_laid_out_as_in_a_fresh_window_after_mixed_measures() {
    let long = "Place number 1, renamed to a name longer than every other".to_string();
    for pane in [Pane::Content, Pane::Squeezed] {
        let short = names(6);
        let mut renamed = short.clone();
        renamed[1].clone_from(&long);
        let steps: [(&str, &[String], Option<usize>); 5] = [
            ("place 2 hovered", &short, Some(2)),
            ("place 1 renamed while place 4 is hovered", &renamed, Some(4)),
            ("place 1 hovered under its long name", &renamed, Some(1)),
            ("place 1 named back, nothing hovered", &short, None),
            ("place 1 hovered under its short name", &short, Some(1)),
        ];
        let mut lw = window(page(&short, None, pane));
        for (what, names, hovered) in steps {
            rebuild(&mut lw, page(names, hovered, pane));
            assert_lays_out_like_a_fresh_window(
                &lw,
                page(names, hovered, pane),
                &format!("{pane:?} pane, {what}"),
            );
        }
    }
}

#![cfg(feature = "text_layout")]
//! Task #19 (live run 2026-08-11): at narrow widths the ribbon's group
//! containers OVERLAP — the Styles group's content painted over the
//! Editing group's labels ("the tab merges divs"). Groups are
//! `flex-shrink: 0` in a row band, so the geometry law is simple: laid
//! sibling groups must tile left-to-right without intersecting, at
//! EVERY width. This walks the real `Ribbon` widget (adaptive chrome,
//! the production `dom()` path) and asserts exactly that.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_css::AzString;
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    widgets::ribbon::{
        Ribbon, RibbonButton, RibbonGallery, RibbonGalleryCell, RibbonGroup, RibbonItem, RibbonTab,
    },
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

fn layout_dom(dom: Dom, width: f32, height: f32) -> LayoutWindow {
    let (css, _) = azul_css::parser2::new_from_str("");
    let mut dom = dom;
    let styled_dom = StyledDom::create(&mut dom, css);

    let mut layout_window = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(width, height);
    layout_window.current_window_state = window_state.clone();
    let renderer_resources = RendererResources::default();
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let mut debug_messages = Some(Vec::new());

    layout_window
        .layout_and_generate_display_list(
            styled_dom,
            &window_state,
            &renderer_resources,
            &system_callbacks,
            &mut debug_messages,
        )
        .unwrap();
    layout_window
}

/// A HOME-tab-like ribbon: several content-sized groups plus one
/// `fills_space` gallery group — the live-run shape that overlapped.
fn word_like_ribbon() -> Dom {
    let group = |label: &str, buttons: &[&str]| {
        let mut g = RibbonGroup::new(label.into());
        for b in buttons {
            g = g.with_item(RibbonItem::LargeButton(RibbonButton::new(
                "layers".into(),
                (*b).into(),
            )));
        }
        g
    };
    // The live-run overlap named the Styles GALLERY's spinner — build
    // the real gallery, not plain buttons.
    let cell = |sample: &str, name: &str| {
        RibbonGalleryCell::new(
            Dom::create_text_do_not_use_without_block_level_wrapper(AzString::from(sample))
                .with_css("font-size: 14px;"),
            name.into(),
        )
    };
    let gallery = RibbonGallery::new(
        vec![
            cell("AaBbCcDc", "Normal"),
            cell("AaBbCcDc", "No Spac..."),
            cell("AaBbCc", "Heading 1"),
            cell("AaBbCcD", "Heading 2"),
            cell("AaB", "Title"),
            cell("AaBbCcD", "Subtitle"),
        ]
        .into(),
    );
    let mut styles = RibbonGroup::new("Styles".into()).with_item(RibbonItem::Gallery(gallery));
    styles.fills_space = true;

    let tab = RibbonTab::new("HOME".into())
        .with_group(group("Clipboard", &["Paste", "Cut", "Copy"]))
        .with_group(group("Font", &["Bold", "Italic", "Underline"]))
        .with_group(group("Paragraph", &["Left", "Center", "Right"]))
        .with_group(styles)
        .with_group(group("Editing", &["Find", "Replace", "Select"]));

    let ribbon = Ribbon::new(vec![tab].into());

    Dom::create_body().with_child(ribbon.dom())
}

/// Collect the laid rects of every `__azul-native-ribbon-group` node.
fn group_rects(lw: &LayoutWindow) -> Vec<(usize, azul_core::geom::LogicalRect)> {
    let layout_result = lw.layout_results.get(&DomId::ROOT_ID).expect("root layout");
    let node_data = layout_result.styled_dom.node_data.as_container();
    let mut out = Vec::new();
    for i in 0..node_data.len() {
        let is_group = node_data[NodeId::new(i)]
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .any(|ic| match ic {
                IdOrClass::Class(c) => c.as_str() == "__azul-native-ribbon-group",
                IdOrClass::Id(_) => false,
            });
        if !is_group {
            continue;
        }
        let id = DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(i))),
        };
        if let Some(rect) = lw.get_node_layout_rect(id) {
            // display:none / zero-sized variants don't participate.
            if rect.size.width > 0.0 && rect.size.height > 0.0 {
                out.push((i, rect));
            }
        }
    }
    out
}

/// The law: visible sibling groups tile without intersection, at every
/// width — INCLUDING the narrow band where the live run overlapped.
#[test]
fn ribbon_groups_never_overlap_at_any_width() {
    for width in [1280.0_f32, 900.0, 700.0, 600.0] {
        let lw = layout_dom(word_like_ribbon(), width, 400.0);
        let mut rects = group_rects(&lw);
        assert!(
            !rects.is_empty(),
            "no laid ribbon groups found at {width}px — the walk is vacuous"
        );
        rects.sort_by(|a, b| a.1.origin.x.partial_cmp(&b.1.origin.x).unwrap());
        for pair in rects.windows(2) {
            let (ia, a) = &pair[0];
            let (ib, b) = &pair[1];
            let a_right = a.origin.x + a.size.width;
            assert!(
                b.origin.x >= a_right - 1.0,
                "GROUP OVERLAP at {width}px: node {ia} spans x={}..{} but node {ib} \
                 starts at x={} (the live-run 'merged divs')",
                a.origin.x,
                a_right,
                b.origin.x
            );
        }
    }
}

/// The live-run shape: the overlap appeared AFTER RESIZING an existing
/// window (wide -> narrow), i.e. through the incremental/reuse relayout
/// path — not on a fresh layout (the test above proves fresh is clean).
#[test]
fn ribbon_groups_never_overlap_after_resize_shrink() {
    let (css, _) = azul_css::parser2::new_from_str("");
    let mut dom = word_like_ribbon();
    let styled_dom = StyledDom::create(&mut dom, css);

    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(1280.0, 400.0);
    lw.current_window_state = window_state.clone();
    let renderer_resources = RendererResources::default();
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let mut debug_messages = Some(Vec::new());

    lw.layout_and_generate_display_list(
        styled_dom.clone(),
        &window_state,
        &renderer_resources,
        &system_callbacks,
        &mut debug_messages,
    )
    .unwrap();

    for width in [1100.0_f32, 900.0, 780.0, 700.0, 640.0, 600.0] {
        lw.current_window_state.size.dimensions = LogicalSize::new(width, 400.0);
        lw.resize_window(
            styled_dom.clone(),
            LogicalSize::new(width, 400.0),
            &renderer_resources,
            &system_callbacks,
            &mut debug_messages,
        )
        .unwrap();

        let mut rects = group_rects(&lw);
        assert!(
            !rects.is_empty(),
            "no laid ribbon groups after resize to {width}px — vacuous walk"
        );
        rects.sort_by(|a, b| a.1.origin.x.partial_cmp(&b.1.origin.x).unwrap());
        for pair in rects.windows(2) {
            let (ia, a) = &pair[0];
            let (ib, b) = &pair[1];
            let a_right = a.origin.x + a.size.width;
            assert!(
                b.origin.x >= a_right - 1.0,
                "GROUP OVERLAP after resize to {width}px: node {ia} spans x={}..{}                  but node {ib} starts at x={}",
                a.origin.x,
                a_right,
                b.origin.x
            );
        }
    }
}

/// Probe: at narrow width, do gallery cells geometrically overflow the
/// overflow:hidden strip, and where does the Styles group end vs the
/// Editing group start? (Diagnostic printout; the clip assertion
/// follows from what this shows.)
#[test]
fn probe_gallery_strip_overflow_geometry() {
    let lw = layout_dom(word_like_ribbon(), 600.0, 400.0);
    let layout_result = lw.layout_results.get(&DomId::ROOT_ID).expect("root layout");
    let node_data = layout_result.styled_dom.node_data.as_container();
    let classes_of = |i: usize| -> Vec<String> {
        node_data[NodeId::new(i)]
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .filter_map(|ic| match ic {
                IdOrClass::Class(c) => Some(c.as_str().to_string()),
                IdOrClass::Id(_) => None,
            })
            .collect()
    };
    for i in 0..node_data.len() {
        let cls = classes_of(i);
        let interesting = cls
            .iter()
            .any(|c| c.contains("ribbon-group") || c.contains("gallery"));
        if !interesting {
            continue;
        }
        let id = DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(i))),
        };
        if let Some(rect) = lw.get_node_layout_rect(id) {
            eprintln!(
                "[PROBE] node {i} {:?} x={:.0}..{:.0} y={:.0} w={:.0}",
                cls,
                rect.origin.x,
                rect.origin.x + rect.size.width,
                rect.origin.y,
                rect.size.width
            );
        }
    }
}

/// THE CLIP LAW (the actual live-run defect): the gallery strip is
/// `overflow: hidden`, its fixed-width cells legitimately overflow in
/// LAYOUT — so at PAINT time every cell primitive must be clipped to
/// the strip. The probe above shows cells spanning to x=1187 in a
/// 600px window; if any paintable primitive's VISIBLE region (bounds
/// intersected with its effective clip stack) escapes past the Styles
/// group's right edge into the Editing group's band, the ribbon
/// "merges divs" exactly as the live run showed.
#[test]
fn gallery_overflow_is_clipped_at_paint_time() {
    use azul_layout::solver3::display_list::DisplayListItem;

    let lw = layout_dom(word_like_ribbon(), 600.0, 400.0);
    let layout_result = lw.layout_results.get(&DomId::ROOT_ID).expect("root layout");

    // The Styles group + strip rects anchor the law.
    let rects = group_rects(&lw);
    let styles_group = rects
        .iter()
        .map(|(_, r)| *r)
        .filter(|r| r.size.width > 100.0)
        .max_by(|a, b| a.origin.x.partial_cmp(&b.origin.x).unwrap());
    // Simpler anchor: the strip node's rect via class walk.
    let node_data = layout_result.styled_dom.node_data.as_container();
    let mut strip_right = None;
    let mut band = None;
    for i in 0..node_data.len() {
        let is_strip = node_data[NodeId::new(i)]
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .any(|ic| matches!(ic, IdOrClass::Class(c) if c.as_str() == "__azul-native-ribbon-gallery-strip"));
        if is_strip {
            let id = DomNodeId {
                dom: DomId::ROOT_ID,
                node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(i))),
            };
            if let Some(r) = lw.get_node_layout_rect(id) {
                strip_right = Some(r.origin.x + r.size.width);
                band = Some((r.origin.y, r.origin.y + r.size.height));
            }
        }
    }
    let strip_right = strip_right.expect("gallery strip laid out");
    let (band_top, band_bottom) = band.expect("strip band");
    let _ = styles_group;

    // The set of nodes INSIDE the strip subtree (cells + previews +
    // labels): only their primitives are held to the strip clip; the
    // spinner and the Editing group legitimately paint to its right.
    let hierarchy = layout_result.styled_dom.node_hierarchy.as_container();
    let mut strip_nodes = std::collections::HashSet::new();
    for i in 0..node_data.len() {
        let mut cur = Some(NodeId::new(i));
        while let Some(c) = cur {
            let is_strip = node_data[c]
                .get_ids_and_classes()
                .as_ref()
                .iter()
                .any(|ic| matches!(ic, IdOrClass::Class(cl) if cl.as_str() == "__azul-native-ribbon-gallery-strip"));
            if is_strip {
                strip_nodes.insert(NodeId::new(i));
                break;
            }
            cur = hierarchy[c].parent_id();
        }
    }

    // Walk the DL with a clip stack; find paintable items in the strip
    // band whose VISIBLE right edge escapes the strip.
    let dl = &layout_result.display_list;
    let mut clip_stack: Vec<(f32, f32, f32, f32)> = Vec::new(); // (x0,y0,x1,y1)
    let mut leaks = Vec::new();
    for (idx, item) in dl.items.iter().enumerate() {
        match item {
            DisplayListItem::PushClip { bounds, .. } => {
                let b = bounds.0;
                let nb = (
                    b.origin.x,
                    b.origin.y,
                    b.origin.x + b.size.width,
                    b.origin.y + b.size.height,
                );
                let eff = clip_stack.last().map_or(nb, |c| {
                    (c.0.max(nb.0), c.1.max(nb.1), c.2.min(nb.2), c.3.min(nb.3))
                });
                clip_stack.push(eff);
            }
            DisplayListItem::PopClip => {
                clip_stack.pop();
            }
            DisplayListItem::Rect { bounds, .. } | DisplayListItem::Border { bounds, .. } => {
                let b = bounds.0;
                let x1 = b.origin.x + b.size.width;
                let y0 = b.origin.y;
                let y1 = b.origin.y + b.size.height;
                // Only primitives in the strip's vertical band.
                if y1 <= band_top || y0 >= band_bottom {
                    continue;
                }
                let visible_x1 = clip_stack.last().map_or(x1, |c| x1.min(c.2));
                let from_strip = dl
                    .node_mapping
                    .get(idx)
                    .and_then(|n| *n)
                    .is_some_and(|n| strip_nodes.contains(&n));
                if from_strip && visible_x1 > strip_right + 1.0 {
                    leaks.push((idx, b.origin.x, visible_x1));
                }
            }
            _ => {}
        }
    }
    assert!(
        leaks.is_empty(),
        "gallery paint LEAKS past the overflow:hidden strip (right edge {strip_right}): \
         {leaks:?} — the live-run 'Styles drawn over Editing'"
    );
}

/// #15 yield probe: what does the CASCADE actually cost at current
/// scale? (StyledDom::create on the ribbon DOM, timed.) Informational.
#[test]
fn probe_cascade_cost() {
    let (css, _) = azul_css::parser2::new_from_str("");
    for _ in 0..3 {
        let mut dom = word_like_ribbon();
        let t0 = std::time::Instant::now();
        let styled = StyledDom::create(&mut dom, css.clone());
        let dt = t0.elapsed();
        eprintln!(
            "[CASCADE-PROBE] {} nodes cascaded in {:?}",
            styled.node_data.as_ref().len(),
            dt
        );
    }
}

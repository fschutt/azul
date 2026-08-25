//! Regression: auto-width flex containers must size to their text content.
//!
//! Distilled from the MS-ribbon widget (2026-08-01): a ribbon group column
//! holds icon+label button rows ("Cut" / "Copy" / "Format Painter"). The
//! column and each row are auto-width flex containers; the row's width must
//! be the ONE-LINE width of its label (max-content), and a `flex-grow: 1`
//! sibling must not squeeze a later auto-width column below its content
//! size. In the live ribbon, "Format Painter" wrapped at min-content (the
//! row sized to the widest WORD, not the line) and the trailing "Editing"
//! column was pushed to zero width.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

fn layout_dom(dom: Dom, css_str: &str, width: f32, height: f32) -> LayoutWindow {
    let (css, _) = azul_css::parser2::new_from_str(css_str);
    let mut dom = dom;
    let styled_dom = StyledDom::create(&mut dom, css);

    let mut layout_window = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(width, height);
    // The hosts keep this in sync; a runtime relayout (e.g. after a CSS
    // patch through the content chokepoint) reads the window size from here.
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

fn node_id(n: usize) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(n))),
    }
}

fn class(name: &str) -> azul_core::dom::IdOrClassVec {
    vec![IdOrClass::Class(name.into())].into()
}

const CSS: &str = r#"
    body { display: flex; flex-direction: row; background: white; }
    .col { display: flex; flex-direction: column; flex-grow: 0; }
    .btn { display: flex; flex-direction: row; flex-grow: 0;
           align-items: center; height: 22px; padding: 1px 3px; }
    .ico { width: 16px; height: 16px; flex-grow: 0; background: #888; }
    .lbl { font-family: sans-serif; font-size: 12px; margin-left: 5px; flex-grow: 0; }
    .grow { flex-grow: 1; background: #eee; }
"#;

fn btn_row(label: &str) -> Dom {
    Dom::create_div()
        .with_ids_and_classes(class("btn"))
        .with_child(Dom::create_div().with_ids_and_classes(class("ico")))
        .with_child(
            Dom::create_text_do_not_use_without_block_level_wrapper(label).with_ids_and_classes(class("lbl")),
        )
}

/// body(row) > [ col[Cut, Copy, Format Painter], grow, col[Replace] ]
fn ribbon_like_dom() -> Dom {
    let clipboard_col = Dom::create_div()
        .with_ids_and_classes(class("col"))
        .with_child(btn_row("Cut"))
        .with_child(btn_row("Copy"))
        .with_child(btn_row("Format Painter"));

    let grow = Dom::create_div().with_ids_and_classes(class("grow"));

    let editing_col = Dom::create_div()
        .with_ids_and_classes(class("col"))
        .with_child(btn_row("Replace"));

    Dom::create_body()
        .with_child(clipboard_col)
        .with_child(grow)
        .with_child(editing_col)
}

// DFS indices for ribbon_like_dom:
// 0 body, 1 col, 2 btn(Cut), 3 ico, 4 lbl, 5 btn(Copy), 6 ico, 7 lbl,
// 8 btn(FP), 9 ico, 10 lbl, 11 grow, 12 col, 13 btn(Replace), 14 ico, 15 lbl
const COL: usize = 1;
const BTN_FP: usize = 8;
const LBL_FP: usize = 10;
const GROW: usize = 11;
const COL_EDITING: usize = 12;
const BTN_REPLACE: usize = 13;

#[test]
fn auto_width_flex_rows_size_to_one_line_text() {
    let lw = layout_dom(ribbon_like_dom(), CSS, 800.0, 200.0);

    let fp_btn = lw.get_node_layout_rect(node_id(BTN_FP)).expect("FP row rect");
    let fp_lbl = lw.get_node_layout_rect(node_id(LBL_FP)).expect("FP label rect");
    let col = lw.get_node_layout_rect(node_id(COL)).expect("col rect");
    println!("col = {col:?}\nfp_btn = {fp_btn:?}\nfp_lbl = {fp_lbl:?}");

    // "Format Painter" at 12px is ONE line: the label must be notably wider
    // than the widest single word ("Painter" ≈ 45px) and the row must not
    // grow taller than its 22px + borders (wrapping = failure).
    assert!(
        fp_lbl.size.width > 60.0,
        "label wrapped at min-content: width = {} (one line is ~80px)",
        fp_lbl.size.width
    );
    assert!(
        fp_btn.size.height < 30.0,
        "button row over-tall (= label wrapped): height = {}",
        fp_btn.size.height
    );
    // The column adopts its widest child.
    assert!(
        col.size.width >= fp_btn.size.width - 0.5,
        "column ({}) narrower than its widest row ({})",
        col.size.width,
        fp_btn.size.width
    );
}

/// Same geometry, but through the REAL Ribbon widget (NodeType::Button
/// containers, icon nodes, inline conditional styles, fixed-height rows) —
/// the distilled div version above passes while the live ribbon wrapped
/// "Format Painter" and pushed the Editing group off-screen.
#[test]
fn ribbon_widget_rows_size_to_one_line_text() {
    use azul_core::dom::NodeType;
    use azul_layout::widgets::ribbon::{
        Ribbon, RibbonButton, RibbonColumn, RibbonGallery, RibbonGalleryCell, RibbonGroup,
        RibbonItem, RibbonTab, RibbonTabVec,
    };

    let clipboard = RibbonGroup::new("Clipboard".into())
        .with_item(RibbonItem::LargeButton(RibbonButton::new(
            "content_paste".into(),
            "Paste".into(),
        )))
        .with_item(RibbonItem::Column(
            RibbonColumn::new()
                .with_item(RibbonItem::SmallButton(RibbonButton::new(
                    "content_cut".into(),
                    "Cut".into(),
                )))
                .with_item(RibbonItem::SmallButton(RibbonButton::new(
                    "content_copy".into(),
                    "Copy".into(),
                )))
                .with_item(RibbonItem::SmallButton(RibbonButton::new(
                    "format_paint".into(),
                    "Format Painter".into(),
                ))),
        ));

    let styles = RibbonGroup::new("Styles".into())
        .with_item(RibbonItem::Gallery(RibbonGallery::new(
            vec![
                RibbonGalleryCell::new(Dom::create_text_do_not_use_without_block_level_wrapper("AaBbCcDc"), "Normal".into()),
                RibbonGalleryCell::new(Dom::create_text_do_not_use_without_block_level_wrapper("AaBbCcDc"), "No Spacing".into()),
            ]
            .into(),
        )))
        .with_fills_space(true);

    let editing = RibbonGroup::new("Editing".into()).with_item(RibbonItem::Column(
        RibbonColumn::new()
            .with_item(RibbonItem::SmallButton(RibbonButton::new(
                "search".into(),
                "Find".into(),
            )))
            .with_item(RibbonItem::SmallButton(RibbonButton::new(
                "find_replace".into(),
                "Replace".into(),
            ))),
    ));

    let tab = RibbonTab::new("HOME".into())
        .with_group(clipboard)
        .with_group(styles)
        .with_group(editing);
    let dom = Dom::create_body()
        .with_child(Ribbon::new(RibbonTabVec::from_vec(vec![tab])).dom());

    let lw = layout_dom(dom, "", 1000.0, 200.0);
    let result = lw
        .layout_results
        .get(&DomId::ROOT_ID)
        .expect("root layout result");

    // Locate labels by text content; the text sits inside its `<p>` block
    // wrapper (the label box carrying the rect), whose parent is the Button
    // container.
    let node_data = result.styled_dom.node_data.as_container();
    let hierarchy = result.styled_dom.node_hierarchy.as_container();
    let find_label = |needle: &str| -> (usize, usize) {
        for i in 0..node_data.len() {
            if let NodeType::Text(t) = node_data[NodeId::new(i)].get_node_type() {
                if t.as_ref().as_str() == needle {
                    let p = hierarchy[NodeId::new(i)]
                        .parent_id()
                        .expect("label has a <p> wrapper");
                    let button = hierarchy[p]
                        .parent_id()
                        .expect("the <p> wrapper has a parent");
                    return (p.index(), button.index());
                }
            }
        }
        panic!("label {needle:?} not found in the ribbon dom");
    };

    let (fp_lbl, fp_btn) = find_label("Format Painter");
    let (rp_lbl, rp_btn) = find_label("Replace");

    let fp_lbl_rect = lw.get_node_layout_rect(node_id(fp_lbl)).expect("fp label rect");
    let fp_btn_rect = lw.get_node_layout_rect(node_id(fp_btn)).expect("fp button rect");
    let rp_lbl_rect = lw.get_node_layout_rect(node_id(rp_lbl)).expect("replace label rect");
    let rp_btn_rect = lw.get_node_layout_rect(node_id(rp_btn)).expect("replace button rect");
    println!("FP  label = {fp_lbl_rect:?}\nFP  btn   = {fp_btn_rect:?}");
    println!("RPL label = {rp_lbl_rect:?}\nRPL btn   = {rp_btn_rect:?}");

    assert!(
        fp_lbl_rect.size.width > 60.0,
        "'Format Painter' wrapped at min-content: label width = {}",
        fp_lbl_rect.size.width
    );
    assert!(
        fp_btn_rect.size.height < 30.0,
        "'Format Painter' button over-tall (label wrapped): height = {}",
        fp_btn_rect.size.height
    );
    // The Editing group must stay inside the 1000px window (the greedy
    // gallery may only take the leftover).
    assert!(
        rp_btn_rect.origin.x + rp_btn_rect.size.width <= 1000.5,
        "Editing group pushed off-screen: Replace button at x={} w={}",
        rp_btn_rect.origin.x,
        rp_btn_rect.size.width
    );
    assert!(
        rp_lbl_rect.size.width > 40.0,
        "'Replace' label squeezed: width = {}",
        rp_lbl_rect.size.width
    );
}

/// The Word overflow contract: when the groups' natural widths exceed the
/// window, the groups stay RIGID (flex-shrink: 0) and only the gallery
/// yields — labels never wrap, the trailing group never leaves the screen.
/// (First seen live: an 8-cell gallery overflowed 1388px and default
/// flex-shrink squeezed every group, wrapping "Format Painter".)
#[test]
fn ribbon_overflow_shrinks_only_the_gallery() {
    use azul_core::dom::NodeType;
    use azul_layout::widgets::ribbon::{
        Ribbon, RibbonButton, RibbonColumn, RibbonGallery, RibbonGalleryCell, RibbonGroup,
        RibbonItem, RibbonTab, RibbonTabVec,
    };

    let wide_col = |labels: [&str; 3]| {
        RibbonItem::Column(
            labels
                .into_iter()
                .fold(RibbonColumn::new(), |c, l| {
                    c.with_item(RibbonItem::SmallButton(RibbonButton::new(
                        "content_cut".into(),
                        l.into(),
                    )))
                }),
        )
    };

    // Three wide fixed groups + a gallery whose 8 cells (8 × 120px) push the
    // natural content width far beyond the 1000px window.
    let g1 = RibbonGroup::new("Clipboard".into())
        .with_item(wide_col(["Cut", "Copy", "Format Painter"]));
    let g2 = RibbonGroup::new("Font".into())
        .with_item(wide_col(["Grow Font", "Shrink Font", "Clear Formatting"]));
    let cells: Vec<RibbonGalleryCell> = (0..8)
        .map(|i| RibbonGalleryCell::new(Dom::create_text_do_not_use_without_block_level_wrapper("AaBbCcDc"), format!("Style {i}").into()))
        .collect();
    let g3 = RibbonGroup::new("Styles".into())
        .with_item(RibbonItem::Gallery(RibbonGallery::new(cells.into())))
        .with_fills_space(true);
    let g4 = RibbonGroup::new("Editing".into())
        .with_item(wide_col(["Find", "Replace", "Select"]));

    let tab = RibbonTab::new("HOME".into())
        .with_group(g1)
        .with_group(g2)
        .with_group(g3)
        .with_group(g4);
    let dom = Dom::create_body()
        .with_child(Ribbon::new(RibbonTabVec::from_vec(vec![tab])).dom());

    let lw = layout_dom(dom, "", 1000.0, 200.0);
    let result = lw
        .layout_results
        .get(&DomId::ROOT_ID)
        .expect("root layout result");

    let node_data = result.styled_dom.node_data.as_container();
    let hierarchy = result.styled_dom.node_hierarchy.as_container();
    let find_label = |needle: &str| -> (usize, usize) {
        for i in 0..node_data.len() {
            if let NodeType::Text(t) = node_data[NodeId::new(i)].get_node_type() {
                if t.as_ref().as_str() == needle {
                    let p = hierarchy[NodeId::new(i)]
                        .parent_id()
                        .expect("label has a <p> wrapper");
                    let button = hierarchy[p]
                        .parent_id()
                        .expect("the <p> wrapper has a parent");
                    return (p.index(), button.index());
                }
            }
        }
        panic!("label {needle:?} not found");
    };

    // Rigid groups: the widest labels stay on ONE line even under overflow.
    for label in ["Format Painter", "Clear Formatting", "Replace"] {
        let (lbl, btn) = find_label(label);
        let lbl_rect = lw.get_node_layout_rect(node_id(lbl)).expect("label rect");
        let btn_rect = lw.get_node_layout_rect(node_id(btn)).expect("button rect");
        assert!(
            btn_rect.size.height < 30.0,
            "{label:?} wrapped under overflow: button height = {} (label w = {})",
            btn_rect.size.height,
            lbl_rect.size.width
        );
    }

    // The trailing Editing group stays fully on-screen...
    let (_, replace_btn) = find_label("Replace");
    let r = lw.get_node_layout_rect(node_id(replace_btn)).expect("replace rect");
    assert!(
        r.origin.x + r.size.width <= 1000.5,
        "Editing group pushed off-screen: x={} w={}",
        r.origin.x,
        r.size.width
    );

    // ...because the gallery yielded: it must be narrower than its natural
    // 8-cell width yet keep at least its one-cell + spinner floor.
    let (_, cell0) = find_label("Style 0");
    let gallery_cell = lw.get_node_layout_rect(node_id(cell0)).expect("cell rect");
    let strip = hierarchy[NodeId::new(cell0)].parent_id().expect("strip");
    let frame = hierarchy[strip].parent_id().expect("frame");
    let frame_rect = lw
        .get_node_layout_rect(node_id(frame.index()))
        .expect("gallery frame rect");
    println!("gallery frame = {frame_rect:?}, cell = {gallery_cell:?}");
    assert!(
        frame_rect.size.width < 8.0 * 120.0,
        "gallery did not yield: width = {}",
        frame_rect.size.width
    );
    assert!(
        frame_rect.size.width >= 130.0,
        "gallery collapsed below its floor: width = {}",
        frame_rect.size.width
    );
}

#[test]
fn flex_grow_sibling_does_not_squeeze_a_later_auto_column() {
    let lw = layout_dom(ribbon_like_dom(), CSS, 800.0, 200.0);

    let grow = lw.get_node_layout_rect(node_id(GROW)).expect("grow rect");
    let editing = lw.get_node_layout_rect(node_id(COL_EDITING)).expect("editing rect");
    let replace = lw.get_node_layout_rect(node_id(BTN_REPLACE)).expect("replace rect");
    println!("grow = {grow:?}\nediting = {editing:?}\nreplace = {replace:?}");

    // The Editing column keeps its content width ("Replace" ≈ 50px + icon);
    // the greedy sibling only takes the LEFTOVER space.
    assert!(
        editing.size.width > 55.0,
        "auto column squeezed by flex-grow sibling: width = {}",
        editing.size.width
    );
    assert!(
        replace.size.height < 30.0,
        "Replace row wrapped: height = {}",
        replace.size.height
    );
    assert!(
        grow.size.width > 400.0,
        "grow sibling should still take the leftover: width = {}",
        grow.size.width
    );
}

/// Minimal probe: a `flex-shrink: 1; overflow: hidden` item with rigid
/// children must shrink below its min-content (CSS: overflow other than
/// visible zeroes the automatic minimum size), leaving room for the
/// trailing fixed sibling.
#[test]
fn overflow_hidden_item_shrinks_below_min_content() {
    const PROBE_CSS: &str = r#"
        body { display: flex; flex-direction: row; }
        .strip { display: flex; flex-direction: row; overflow: hidden; flex-grow: 1; }
        .cell { width: 120px; height: 40px; flex-grow: 0; flex-shrink: 0; background: #ccc; }
        .tail { width: 120px; height: 40px; flex-grow: 0; flex-shrink: 0; background: #77c; }
    "#;

    let mut strip = Dom::create_div().with_ids_and_classes(class("strip"));
    for _ in 0..8 {
        strip = strip.with_child(Dom::create_div().with_ids_and_classes(class("cell")));
    }
    let dom = Dom::create_body()
        .with_child(strip)
        .with_child(Dom::create_div().with_ids_and_classes(class("tail")));

    let lw = layout_dom(dom, PROBE_CSS, 500.0, 100.0);
    // 0 body, 1 strip, 2..=9 cells, 10 tail
    let strip_rect = lw.get_node_layout_rect(node_id(1)).expect("strip rect");
    let tail_rect = lw.get_node_layout_rect(node_id(10)).expect("tail rect");
    println!("strip = {strip_rect:?}\ntail = {tail_rect:?}");

    assert!(
        strip_rect.size.width < 400.0,
        "overflow:hidden item did not shrink below min-content: width = {} \
         (expected ~380 in a 500px row with a 120px rigid tail)",
        strip_rect.size.width
    );
    assert!(
        tail_rect.origin.x + tail_rect.size.width <= 500.5,
        "tail pushed off-screen: x={} w={}",
        tail_rect.origin.x,
        tail_rect.size.width
    );
}

/// Nested variant: the shrunk flex item is a FRAME (overflow: visible) whose
/// CHILD is the scroll container. Browsers collapse the frame's min-content
/// contribution here (bounded by the child scroll container's zeroed
/// minimum); taffy 0.10 does NOT — the frame keeps its full min-content and
/// refuses to shrink. KNOWN ENGINE GAP (upstream taffy): kept as an ignored
/// characterization test. Workaround for widgets: make the direct flex item
/// the scroll container (see the ribbon gallery frame).
#[test]
#[ignore = "taffy 0.10 gap: nested scroll-container min-content contribution is not collapsed"]
fn frame_around_overflow_hidden_strip_shrinks_too() {
    const PROBE_CSS: &str = r#"
        body { display: flex; flex-direction: row; }
        .frame { display: flex; flex-direction: row; flex-grow: 1; }
        .strip { display: flex; flex-direction: row; overflow: hidden; flex-grow: 1; }
        .cell { width: 120px; height: 40px; flex-grow: 0; flex-shrink: 0; background: #ccc; }
        .spinner { width: 15px; height: 40px; flex-grow: 0; flex-shrink: 0; background: #c77; }
        .tail { width: 120px; height: 40px; flex-grow: 0; flex-shrink: 0; background: #77c; }
    "#;

    let mut strip = Dom::create_div().with_ids_and_classes(class("strip"));
    for _ in 0..8 {
        strip = strip.with_child(Dom::create_div().with_ids_and_classes(class("cell")));
    }
    let frame = Dom::create_div()
        .with_ids_and_classes(class("frame"))
        .with_child(strip)
        .with_child(Dom::create_div().with_ids_and_classes(class("spinner")));
    let dom = Dom::create_body()
        .with_child(frame)
        .with_child(Dom::create_div().with_ids_and_classes(class("tail")));

    let lw = layout_dom(dom, PROBE_CSS, 500.0, 100.0);
    // 0 body, 1 frame, 2 strip, 3..=10 cells, 11 spinner, 12 tail
    let frame_rect = lw.get_node_layout_rect(node_id(1)).expect("frame rect");
    let tail_rect = lw.get_node_layout_rect(node_id(12)).expect("tail rect");
    println!("frame = {frame_rect:?}\ntail = {tail_rect:?}");

    assert!(
        frame_rect.size.width < 400.0,
        "frame around a scroll container did not shrink: width = {}",
        frame_rect.size.width
    );
    assert!(
        tail_rect.origin.x + tail_rect.size.width <= 500.5,
        "tail pushed off-screen: x={} w={}",
        tail_rect.origin.x,
        tail_rect.size.width
    );
}

/// Two adjacent TEXT children in an auto-width flex row (exactly what a
/// resolved font icon + label produce in the live ribbon). If the engine
/// merges them into one anonymous IFC and measures its max-content as
/// min-content, the label wraps ("Format / Painter") even with unlimited
/// space — the live-app truncation that the div-icon variant above misses.
#[test]
fn two_text_children_in_a_flex_row_stay_on_one_line() {
    const PROBE_CSS: &str = r#"
        body { display: flex; flex-direction: row; }
        .btn { display: flex; flex-direction: row; flex-grow: 0; flex-shrink: 0;
               align-items: center; height: 22px; padding: 1px 3px; }
        .glyph { font-family: sans-serif; font-size: 16px; flex-grow: 0; }
        .lbl { font-family: sans-serif; font-size: 12px; margin-left: 5px; flex-grow: 0; }
    "#;

    let btn = Dom::create_div()
        .with_ids_and_classes(class("btn"))
        .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("X").with_ids_and_classes(class("glyph")))
        .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("Format Painter").with_ids_and_classes(class("lbl")));
    let dom = Dom::create_body().with_child(btn);

    let lw = layout_dom(dom, PROBE_CSS, 800.0, 100.0);
    // 0 body, 1 btn, 2 glyph, 3 lbl
    let btn_rect = lw.get_node_layout_rect(node_id(1)).expect("btn rect");
    let lbl_rect = lw.get_node_layout_rect(node_id(3)).expect("lbl rect");
    println!("btn = {btn_rect:?}\nlbl = {lbl_rect:?}");

    assert!(
        lbl_rect.size.width > 60.0,
        "label beside a text glyph wrapped at min-content: width = {}",
        lbl_rect.size.width
    );
    assert!(
        btn_rect.size.height < 30.0,
        "button over-tall (label wrapped): height = {}",
        btn_rect.size.height
    );
}

/// The FULL live-app ingredient set: the icon is a real resolved font icon
/// (a text node whose font-family is an embedded `FontRef`), produced by the
/// actual icon-resolution pass, sitting beside the label in an auto-width
/// flex row. Reproduces the shipped ribbon truncation ("Past", "Format")
/// that none of the div/text stand-ins above trigger.
#[test]
#[cfg(all(feature = "text_layout", feature = "font_loading"))]
fn label_beside_a_resolved_fontref_icon_stays_on_one_line() {
    use azul_core::icon::{resolve_icons_in_styled_dom, IconProviderHandle, SharedIconProvider};
    use azul_css::system::SystemStyle;

    const KOHO_LIGHT: &[u8] = include_bytes!("../../examples/assets/fonts/KoHo-Light.ttf");
    let parsed =
        azul_layout::font::parsed::ParsedFont::from_bytes(KOHO_LIGHT, 0, &mut Vec::new())
            .expect("bundled face parses");
    let font = azul_layout::parsed_font_to_font_ref(parsed);

    let mut provider = IconProviderHandle::new();
    provider.set_resolver(azul_layout::icon::default_icon_resolver);
    azul_layout::icon::register_font_icon(&mut provider, "test", "content_cut", font, "X");
    let provider = SharedIconProvider::from_handle(provider);

    const PROBE_CSS: &str = r#"
        body { display: flex; flex-direction: row; }
        .btn { display: flex; flex-direction: row; flex-grow: 0; flex-shrink: 0;
               align-items: center; height: 22px; padding: 1px 3px; }
        .glyph { font-size: 16px; flex-grow: 0; }
        .lbl { font-family: sans-serif; font-size: 12px; margin-left: 5px; flex-grow: 0; }
    "#;

    let btn = Dom::create_div()
        .with_ids_and_classes(class("btn"))
        .with_child(Dom::create_icon("content_cut").with_ids_and_classes(class("glyph")))
        .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("Format Painter").with_ids_and_classes(class("lbl")));
    let mut dom = Dom::create_body().with_child(btn);

    let (css, _) = azul_css::parser2::new_from_str(PROBE_CSS);
    let mut styled = StyledDom::create(&mut dom, css);
    resolve_icons_in_styled_dom(&mut styled, &provider, &SystemStyle::default());

    let mut layout_window = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(800.0, 100.0);
    let renderer_resources = RendererResources::default();
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let mut debug_messages = Some(Vec::new());
    layout_window
        .layout_and_generate_display_list(
            styled,
            &window_state,
            &renderer_resources,
            &system_callbacks,
            &mut debug_messages,
        )
        .unwrap();

    // 0 body, 1 btn, 2 glyph (icon -> span), 3 its text leaf, 4 lbl
    let btn_rect = layout_window.get_node_layout_rect(node_id(1)).expect("btn rect");
    let lbl_rect = layout_window.get_node_layout_rect(node_id(4)).expect("lbl rect");
    println!("btn = {btn_rect:?}\nlbl = {lbl_rect:?}");

    assert!(
        lbl_rect.size.width > 60.0,
        "label beside a RESOLVED FontRef icon wrapped at min-content: width = {}",
        lbl_rect.size.width
    );
    assert!(
        btn_rect.size.height < 30.0,
        "button over-tall (label wrapped): height = {}",
        btn_rect.size.height
    );
}

/// Contract for every show/hide widget (combobox list, popover, ribbon
/// gallery panel): patching `display` at runtime through the content
/// chokepoint must RELAYOUT, so a `display: none` node gains a real layout
/// rect. Without this, `set_css_property(display)` silently does nothing
/// visible and the widget looks dead.
#[test]
fn runtime_display_patch_relayouts_a_hidden_node() {
    use azul_core::dom::{DomId as CoreDomId, NodeId as CoreNodeId};
    use azul_css::props::{layout::LayoutDisplay, property::CssProperty};
    use azul_layout::overlay::{ContentChange, ContentDirtyTier};

    const PROBE_CSS: &str = r#"
        body { display: flex; flex-direction: column; }
        .panel { display: none; width: 200px; height: 60px; background: #ccc; }
    "#;

    let dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_ids_and_classes(class("panel"))
            .with_child(Dom::create_div()),
    );

    let mut lw = layout_dom(dom, PROBE_CSS, 400.0, 300.0);
    // 0 body, 1 panel, 2 child
    assert!(
        lw.get_node_layout_rect(node_id(1)).is_none_or(|r| r.size.height == 0.0),
        "a display:none panel must not occupy space before the patch"
    );

    let result = lw.apply_content_change(ContentChange::NodeCss {
        dom_id: CoreDomId::ROOT_ID,
        node_id: CoreNodeId::new(1),
        props: vec![CssProperty::const_display(LayoutDisplay::Flex)],
        override_only: false,
    });

    assert_eq!(
        result.tier,
        ContentDirtyTier::Relayout,
        "a display change must be charged as a relayout, not a repaint"
    );

    let rect = lw
        .get_node_layout_rect(node_id(1))
        .expect("the shown panel must have a layout rect");
    assert!(
        rect.size.height > 0.0 && rect.size.width > 0.0,
        "shown panel still has no size: {rect:?}"
    );
}

/// A fixed-height child of an AUTO-height column flex container must keep
/// its height.
///
/// The root's `used_size` is content-derived and is still 0 when the flex
/// algorithm runs, so passing it as a DEFINITE main size made taffy see
/// negative free space and shrink every item with the default
/// `flex-shrink: 1` to zero — a fixed-height toolbar under
/// `body { display: flex; flex-direction: column }` (the first thing a real
/// app writes) simply vanished. CSS Flexbox 9.7: an indefinite main size
/// uses hypothetical main sizes and performs no shrinking.
#[test]
fn fixed_height_children_survive_an_auto_height_flex_root() {
    const CSS_ROOT_FLEX: &str = r#"
        body { display: flex; flex-direction: column; }
        .panel { display: flex; width: 200px; height: 60px; }
    "#;
    const CSS_ROOT_BLOCK: &str = r#"
        body { display: block; }
        .panel { display: flex; width: 200px; height: 60px; }
    "#;
    const CSS_NESTED: &str = r#"
        body { display: flex; flex-direction: column; }
        .wrap { display: flex; flex-direction: column; }
        .panel { display: flex; width: 200px; height: 60px; }
    "#;
    let mk = || {
        Dom::create_div()
            .with_ids_and_classes(class("panel"))
            .with_child(Dom::create_div())
    };

    // Direct child of a flex-column body (the regressing case).
    let lw = layout_dom(Dom::create_body().with_child(mk()), CSS_ROOT_FLEX, 400.0, 300.0);
    let panel = lw.get_node_layout_rect(node_id(1)).expect("panel rect");
    let body = lw.get_node_layout_rect(node_id(0)).expect("body rect");
    assert_eq!(
        panel.size.height, 60.0,
        "fixed-height flex item collapsed under an auto-height flex root: {panel:?}"
    );
    assert!(
        body.size.height >= 60.0,
        "the auto-height root must grow to its content: {body:?}"
    );

    // Controls: the same item under a block root, and one level deeper.
    let lw = layout_dom(Dom::create_body().with_child(mk()), CSS_ROOT_BLOCK, 400.0, 300.0);
    assert_eq!(
        lw.get_node_layout_rect(node_id(1)).expect("panel").size.height,
        60.0,
        "block-root control regressed"
    );

    let lw = layout_dom(
        Dom::create_body().with_child(
            Dom::create_div().with_ids_and_classes(class("wrap")).with_child(mk()),
        ),
        CSS_NESTED,
        400.0,
        300.0,
    );
    assert_eq!(
        lw.get_node_layout_rect(node_id(2)).expect("panel").size.height,
        60.0,
        "nested-container control regressed"
    );
}


/// A synthetic click aimed at a node's own centre must HIT that node.
///
/// The E2E `click`/`double_click` ops resolve a selector to
/// `get_node_hit_test_bounds(node).centre()` and then click that point, so
/// the hit tester and the bounds lookup have to agree. In the live ribbon
/// they did not: clicking the gallery's "More" button (a `Button` inside an
/// `overflow: hidden` frame) reported a *gallery cell* as the hit node.
#[test]
fn hit_testing_a_nodes_own_centre_returns_that_node() {
    use azul_core::dom::DomId as CoreDomId;
    use azul_layout::headless::CpuHitTester;
    use azul_layout::widgets::ribbon::{
        Ribbon, RibbonGallery, RibbonGalleryCell, RibbonGroup, RibbonItem, RibbonTab,
        RibbonTabVec,
    };

    let cells: Vec<RibbonGalleryCell> = (0..8)
        .map(|i| RibbonGalleryCell::new(Dom::create_text_do_not_use_without_block_level_wrapper("AaBbCcDc"), format!("Style {i}").into()))
        .collect();
    let tab = RibbonTab::new("HOME".into()).with_group(
        RibbonGroup::new("Styles".into())
            .with_item(RibbonItem::Gallery(RibbonGallery::new(cells.into())))
            .with_fills_space(true),
    );
    let dom = Dom::create_body()
        .with_child(Ribbon::new(RibbonTabVec::from_vec(vec![tab])).dom());

    // NARROW window: 8 cells x 120px overflow the `overflow: hidden`
    // gallery frame, so the frame is a real scroll container with content
    // wider than its viewport.
    let lw = layout_dom(dom, "", 700.0, 260.0);
    let result = lw.layout_results.get(&CoreDomId::ROOT_ID).expect("root");

    // Locate the "More" button by class.
    let node_data = result.styled_dom.node_data.as_container();
    let mut more_idx = None;
    for i in 0..node_data.len() {
        let is_more = node_data[NodeId::new(i)]
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .any(|c| matches!(c, IdOrClass::Class(s)
                if s.as_str() == "__azul-native-ribbon-gallery-more"));
        if is_more {
            more_idx = Some(i);
            break;
        }
    }
    let more_idx = more_idx.expect("the gallery renders a More button");

    let layout_rect = lw
        .get_node_layout_rect(node_id(more_idx))
        .expect("More has a layout rect");
    let hit_bounds = lw
        .get_node_hit_test_bounds(node_id(more_idx))
        .expect("More has hit-test bounds");
    println!("More layout = {layout_rect:?}\nMore hit    = {hit_bounds:?}");

    assert!(
        (layout_rect.origin.x - hit_bounds.origin.x).abs() < 1.0
            && (layout_rect.origin.y - hit_bounds.origin.y).abs() < 1.0,
        "hit-test bounds disagree with the layout rect: layout {layout_rect:?} vs hit {hit_bounds:?}"
    );

    // ...and clicking that centre must actually hit the More button.
    let mut tester = CpuHitTester::new();
    tester.rebuild_from_layout(&lw.layout_results);
    let centre = azul_core::geom::LogicalPosition::new(
        hit_bounds.origin.x + hit_bounds.size.width / 2.0,
        hit_bounds.origin.y + hit_bounds.size.height / 2.0,
    );
    let hits = tester.hit_test(centre);
    println!("hits at {centre:?}: {hits:?} (More = node {more_idx})");
    assert!(
        hits.iter().any(|(_, n)| n.index() == more_idx),
        "a click at the More button's own centre {centre:?} hit {hits:?} instead of node {more_idx}"
    );
}

/// `overflow: hidden` must clip HIT TESTING, not just painting.
///
/// A cell scrolled/overflowing past its container's right edge is invisible,
/// so a click beyond that edge must reach whatever is painted there — not
/// the clipped-away cell. In the live ribbon this made the gallery's "More"
/// button unclickable: the 5th (clipped) gallery cell answered clicks that
/// landed on the button drawn beside the strip.
#[test]
fn overflow_hidden_clips_hit_testing_not_just_painting() {
    use azul_layout::headless::CpuHitTester;

    const PROBE_CSS: &str = r#"
        body { display: flex; flex-direction: row; }
        /* flex-GROWN scroll container (the ribbon gallery strip): its width
           is decided by the flex algorithm, not by an explicit length. */
        .strip { display: flex; flex-direction: row; overflow: hidden;
                 height: 60px; flex-grow: 1; }
        .cell { width: 120px; height: 60px; flex-grow: 0; flex-shrink: 0; background: #ccc; }
        .side { width: 100px; height: 60px; flex-grow: 0; flex-shrink: 0; background: #77c; }
    "#;

    let mut strip = Dom::create_div().with_ids_and_classes(class("strip"));
    for _ in 0..6 {
        strip = strip.with_child(Dom::create_div().with_ids_and_classes(class("cell")));
    }
    let dom = Dom::create_body()
        .with_child(strip)
        .with_child(Dom::create_div().with_ids_and_classes(class("side")));

    let lw = layout_dom(dom, PROBE_CSS, 600.0, 200.0);
    // 0 body, 1 strip, 2..=7 cells, 8 side
    let side = lw.get_node_layout_rect(node_id(8)).expect("side rect");
    let strip_rect = lw.get_node_layout_rect(node_id(1)).expect("strip rect");
    println!("strip = {strip_rect:?}\nside = {side:?}");

    let mut tester = CpuHitTester::new();
    tester.rebuild_from_layout(&lw.layout_results);

    // A point over the side panel, i.e. OUTSIDE the 300px-wide strip.
    let p = azul_core::geom::LogicalPosition::new(
        side.origin.x + side.size.width / 2.0,
        side.origin.y + side.size.height / 2.0,
    );
    let hits = tester.hit_test(p);
    println!("hits at {p:?}: {hits:?}");

    let cell_hits: Vec<usize> = hits
        .iter()
        .map(|(_, n)| n.index())
        .filter(|i| (2..=7).contains(i))
        .collect();
    assert!(
        cell_hits.is_empty(),
        "clipped-away cells {cell_hits:?} are still hit-testable at {p:?} \
         (outside their overflow:hidden container)"
    );
    assert!(
        hits.iter().any(|(_, n)| n.index() == 8),
        "the visible side panel must be hit at {p:?}, got {hits:?}"
    );
}

/// Hit-test clipping must use the INNERMOST clip, not just the outermost.
///
/// The ribbon gallery nests two scroll containers: an `overflow: hidden`
/// frame holding an `overflow: hidden` cell strip plus a spinner column
/// beside it. If the overflowing cells are only clipped by the outer frame,
/// their hit rects spill across the strip's edge and cover the spinner —
/// which is exactly why clicking the gallery's "More" button dispatched to
/// a gallery cell in the live app.
#[test]
fn nested_overflow_containers_clip_hit_testing_at_the_inner_edge() {
    use azul_layout::headless::CpuHitTester;

    const PROBE_CSS: &str = r#"
        body { display: flex; flex-direction: row; }
        .frame { display: flex; flex-direction: row; overflow: hidden;
                 width: 300px; height: 60px; flex-grow: 0; flex-shrink: 0; }
        .strip { display: flex; flex-direction: row; overflow: hidden;
                 height: 60px; flex-grow: 1; }
        .cell { width: 120px; height: 60px; flex-grow: 0; flex-shrink: 0; background: #ccc; }
        .spinner { width: 100px; height: 60px; flex-grow: 0; flex-shrink: 0; background: #77c; }
    "#;

    let mut strip = Dom::create_div().with_ids_and_classes(class("strip"));
    for _ in 0..6 {
        strip = strip.with_child(Dom::create_div().with_ids_and_classes(class("cell")));
    }
    // frame > [strip, spinner] — the spinner sits INSIDE the outer clip but
    // OUTSIDE the inner one.
    let frame = Dom::create_div()
        .with_ids_and_classes(class("frame"))
        .with_child(strip)
        .with_child(Dom::create_div().with_ids_and_classes(class("spinner")));
    let dom = Dom::create_body().with_child(frame);

    let lw = layout_dom(dom, PROBE_CSS, 600.0, 200.0);
    // 0 body, 1 frame, 2 strip, 3..=8 cells, 9 spinner
    let strip_rect = lw.get_node_layout_rect(node_id(2)).expect("strip rect");
    let spinner = lw.get_node_layout_rect(node_id(9)).expect("spinner rect");
    println!("strip = {strip_rect:?}\nspinner = {spinner:?}");

    let mut tester = CpuHitTester::new();
    tester.rebuild_from_layout(&lw.layout_results);

    let p = azul_core::geom::LogicalPosition::new(
        spinner.origin.x + spinner.size.width / 2.0,
        spinner.origin.y + spinner.size.height / 2.0,
    );
    let hits = tester.hit_test(p);
    println!("hits at {p:?}: {hits:?}");

    let cell_hits: Vec<usize> = hits
        .iter()
        .map(|(_, n)| n.index())
        .filter(|i| (3..=8).contains(i))
        .collect();
    assert!(
        cell_hits.is_empty(),
        "cells {cell_hits:?} clipped away by the INNER strip are still hit at {p:?} \
         (over the spinner) — the inner clip was ignored"
    );
    assert!(
        hits.iter().any(|(_, n)| n.index() == 9),
        "the spinner must be hit at {p:?}, got {hits:?}"
    );
}

/// `get_node_hit_test_bounds` must return the node's OWN area.
///
/// It resolves a node to its tag, then scans the display list for the first
/// `HitTestArea` carrying that tag. With the full five-group ribbon (the
/// live example, ~200 nodes) that lookup returned a *gallery cell's* rect
/// for the gallery "More" button, so every selector-targeted synthetic
/// click aimed at the button landed on the cell instead.
#[test]
fn hit_test_bounds_match_the_layout_rect_in_a_full_ribbon() {
    use azul_core::dom::DomId as CoreDomId;
    use azul_layout::widgets::ribbon::{
        Ribbon, RibbonButton, RibbonColumn, RibbonGallery, RibbonGalleryCell, RibbonGroup,
        RibbonItem, RibbonTab, RibbonTabVec,
    };

    let col = |labels: [&str; 3]| {
        RibbonItem::Column(labels.into_iter().fold(RibbonColumn::new(), |c, l| {
            c.with_item(RibbonItem::SmallButton(RibbonButton::new("content_cut".into(), l.into())))
        }))
    };
    let cells: Vec<RibbonGalleryCell> = (0..8)
        .map(|i| RibbonGalleryCell::new(Dom::create_text_do_not_use_without_block_level_wrapper("AaBbCcDc"), format!("Style {i}").into()))
        .collect();

    let tab = RibbonTab::new("HOME".into())
        .with_group(RibbonGroup::new("Clipboard".into()).with_item(col(["Cut", "Copy", "Format Painter"])))
        .with_group(RibbonGroup::new("Font".into()).with_item(col(["Grow", "Shrink", "Clear"])))
        .with_group(RibbonGroup::new("Paragraph".into()).with_item(col(["Bullets", "Numbering", "Sort"])))
        .with_group(
            RibbonGroup::new("Styles".into())
                .with_item(RibbonItem::Gallery(RibbonGallery::new(cells.into())))
                .with_fills_space(true),
        )
        .with_group(RibbonGroup::new("Editing".into()).with_item(col(["Find", "Replace", "Select"])));

    // Match the live example: a mock title bar plus NINE tabs (each tab
    // carries chrome callbacks, so each mints its own hit-test tag).
    let mut tabs = vec![tab];
    for label in ["INSERT", "DESIGN", "PAGE LAYOUT", "REFERENCES", "MAILINGS", "REVIEW",
                  "VIEW", "ADD-INS"] {
        tabs.push(RibbonTab::new(label.into()).with_group(
            RibbonGroup::new("Preview".into()).with_item(RibbonItem::LargeButton(
                RibbonButton::new("layers".into(), label.into()),
            )),
        ));
    }
    let title_bar = Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center; height: 30px;")
        .with_child(Dom::create_icon("save"))
        .with_child(Dom::create_icon("undo"))
        .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("Document1 - AzWriter"))
        .with_child(Dom::create_icon("close"));
    let dom = Dom::create_body()
        .with_child(title_bar)
        .with_child(
            Ribbon::new(RibbonTabVec::from_vec(tabs))
                .with_app_button(azul_layout::widgets::ribbon::RibbonAppButton::new("FILE".into()))
                .dom(),
        );
    let lw = layout_dom(dom, "", 1388.0, 260.0);
    let result = lw.layout_results.get(&CoreDomId::ROOT_ID).expect("root");

    let node_data = result.styled_dom.node_data.as_container();
    let find_class = |name: &str| -> Option<usize> {
        (0..node_data.len()).find(|i| {
            node_data[NodeId::new(*i)]
                .get_ids_and_classes()
                .as_ref()
                .iter()
                .any(|c| matches!(c, IdOrClass::Class(s) if s.as_str() == name))
        })
    };

    let more = find_class("__azul-native-ribbon-gallery-more").expect("More button");
    let cell0 = find_class("__azul-native-ribbon-gallery-cell").expect("a gallery cell");

    let layout_rect = lw.get_node_layout_rect(node_id(more)).expect("More layout rect");
    let hit_bounds = lw.get_node_hit_test_bounds(node_id(more)).expect("More hit bounds");
    let cell_rect = lw.get_node_layout_rect(node_id(cell0)).expect("cell layout rect");
    println!("More(node {more}) layout = {layout_rect:?}");
    println!("More(node {more}) hit    = {hit_bounds:?}");
    println!("cell0(node {cell0}) layout = {cell_rect:?}");

    assert!(
        (layout_rect.origin.x - hit_bounds.origin.x).abs() < 1.0,
        "hit-test bounds for the More button point at a different node: \
         layout {layout_rect:?} vs hit {hit_bounds:?} (cell0 is {cell_rect:?})"
    );

    // (a) TAG-NAMESPACE COLLISION: `tag.0` holds a node's sequential TagId
    //     for DOM-node areas but `(dom_id << 32) | node_index` for a text
    //     run's cursor area — identical numbers for DomId(0). The bounds
    //     lookup must match the tag TYPE too, or a node whose TagId equals
    //     some other node's INDEX silently gets that node's text rect (the
    //     live ribbon aimed clicks for node 199 at node 172's label).
    {
        use azul_core::hit_test::{TAG_TYPE_CURSOR, TAG_TYPE_DOM_NODE};
        use azul_layout::solver3::display_list::DisplayListItem;

        // Every tag id that a text-run (cursor) area carries.
        let mut cursor_tags: Vec<u64> = Vec::new();
        for item in &result.display_list.items {
            if let DisplayListItem::HitTestArea { tag, .. } = item {
                if tag.1 & TAG_TYPE_CURSOR != 0 {
                    cursor_tags.push(tag.0);
                }
            }
        }

        // Check EVERY hit-testable node: its bounds must be its own rect.
        let mut checked = 0usize;
        let mut collisions = 0usize;
        for mapping in result.styled_dom.tag_ids_to_node_ids.iter() {
            let Some(nid) = mapping.node_id.into_crate_internal() else {
                continue;
            };
            let tag = mapping.tag_id.inner;
            if cursor_tags.contains(&tag) {
                collisions += 1;
            }
            let (Some(layout), Some(hit)) = (
                lw.get_node_layout_rect(node_id(nid.index())),
                lw.get_node_hit_test_bounds(node_id(nid.index())),
            ) else {
                continue;
            };
            checked += 1;
            assert!(
                (layout.origin.x - hit.origin.x).abs() < 1.0
                    && (layout.origin.y - hit.origin.y).abs() < 1.0,
                "node {} (tag {tag}): hit bounds {hit:?} are not its layout rect {layout:?} \
                 — the lookup crossed the DOM-node/cursor tag namespaces",
                nid.index()
            );
        }
        println!("checked {checked} hit-testable nodes, {collisions} tag collisions present");
        assert!(checked > 20, "fixture got smaller: only {checked} nodes checked");
        assert!(
            collisions > 0,
            "this fixture no longer contains a DOM-node/cursor tag collision, so it \
             cannot catch the regression — extend it until one exists"
        );
        // Sanity: the More button still contributes exactly one DOM-node area.
        let more_tag = result
            .styled_dom
            .tag_ids_to_node_ids
            .iter()
            .find(|m| m.node_id.into_crate_internal() == Some(NodeId::new(more)))
            .expect("the More button is hit-testable")
            .tag_id
            .inner;
        let dom_node_areas = result
            .display_list
            .items
            .iter()
            .filter(|i| {
                matches!(i, DisplayListItem::HitTestArea { tag, .. }
                    if tag.0 == more_tag && tag.1 == TAG_TYPE_DOM_NODE)
            })
            .count();
        assert_eq!(dom_node_areas, 1);
    }

    // (b) Does the E2E selector path over-match? `.…-gallery-more` must
    //     select exactly the More button, not every `.…-gallery-*` node.
    {
        use azul_core::style::matches_html_element;
        use azul_css::parser2::parse_css_path;

        let path = parse_css_path(".__azul-native-ribbon-gallery-more")
            .expect("the class selector must parse");
        let hierarchy = result.styled_dom.node_hierarchy.as_container();
        let cascade = result.styled_dom.cascade_info.as_container();
        let matched: Vec<usize> = (0..node_data.len())
            .filter(|i| {
                matches_html_element(
                    &path,
                    NodeId::new(*i),
                    &hierarchy,
                    &node_data,
                    &cascade,
                    None,
                )
            })
            .collect();
        println!("selector matched nodes: {matched:?} (More = {more})");
        assert_eq!(
            matched,
            vec![more],
            "`.__azul-native-ribbon-gallery-more` must match ONLY the More button"
        );
    }

    // (c) Does a click at the More button's own centre reach it?
    {
        use azul_layout::headless::CpuHitTester;
        let mut tester = CpuHitTester::new();
        tester.rebuild_from_layout(&lw.layout_results);
        let centre = azul_core::geom::LogicalPosition::new(
            hit_bounds.origin.x + hit_bounds.size.width / 2.0,
            hit_bounds.origin.y + hit_bounds.size.height / 2.0,
        );
        let hits = tester.hit_test(centre);
        println!("hits at More centre {centre:?}: {hits:?}");
        assert_eq!(
            hits.first().map(|(_, n)| n.index()),
            Some(more),
            "a click at the More button's centre must reach it first, got {hits:?}"
        );
    }
}

/// ENGINE GAP: a viewport-conditional INLINE property is never applied.
///
/// `CssPropertyWithConditions` can carry any `DynamicSelector` — viewport
/// width, media type, theme, OS — and `DynamicSelector::matches` implements
/// all of them. But the production resolver (`CssPropertyCache::get_property`,
/// plus the compact-cache fast paths) only ever tests PSEUDO-STATE conditions
/// (`matches_pseudo_state`). The context-aware
/// `get_property_with_context`, which does evaluate the rest, has no
/// production caller.
///
/// So a widget written with `@media`-style inline conditions — the ribbon's
/// touch layout — silently keeps its unconditional value at every viewport
/// size. Fixing it means threading a `DynamicSelectorContext` (viewport,
/// theme, OS) through property resolution and invalidating the compact cache
/// on viewport change; that is a cascade-level change, so this test documents
/// the gap rather than asserting today's behaviour.
#[test]
fn inline_viewport_conditions_are_applied() {
    use azul_core::dom::{CssPropertyWithConditions, CssPropertyWithConditionsVec};
    use azul_css::{
        dynamic_selector::{DynamicSelector, MinMaxRange},
        props::{layout::LayoutDisplay, property::CssProperty},
    };

    // display: none, and display: flex only at viewports <= 720px.
    let mobile_only = CssPropertyWithConditionsVec::from_vec(vec![
        CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::None)),
        CssPropertyWithConditions::with_conditions(
            CssProperty::const_display(LayoutDisplay::Flex),
            vec![DynamicSelector::ViewportWidth(MinMaxRange {
                min: f32::NAN,
                max: 720.0,
            })]
            .into(),
        ),
    ]);

    let mut panel = Dom::create_div().with_child(Dom::create_text_do_not_use_without_block_level_wrapper("touch"));
    panel.root.set_css_props(mobile_only);
    let dom = Dom::create_body().with_child(panel);

    // 390px viewport: the conditional `display: flex` must win.
    let lw = layout_dom(dom, "body { display: flex; }", 390.0, 600.0);
    let rect = lw.get_node_layout_rect(node_id(1));
    assert!(
        rect.is_some_and(|r| r.size.height > 0.0),
        "the mobile-only panel did not lay out at a 390px viewport: {rect:?}"
    );
}

/// The author `* { margin: 0 }` reset must beat the UA body margin — UA
/// origin loses to author origin regardless of specificity. `apply_ua_css`
/// skipped only properties already set per-node (css_props / cascaded /
/// inline) and never consulted the GLOBAL `*` bucket, so every reftest page
/// with the classic reset rendered shifted by the UA's 8px body margin
/// against Chrome (ribbonbug-flex-root-auto-height-001).
#[test]
fn global_star_reset_beats_the_ua_body_margin() {
    let xml = r#"<html><head><style type="text/css">
        * { margin: 0; padding: 0; }
        body { width: 800px; }
        .toolbar { height: 120px; }
    </style></head>
    <body><div class="toolbar"></div></body></html>"#;

    let styled = azul_layout::xml::parse_xml_to_styled_dom(xml).expect("parses");
    let mut layout_window = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(1920.0, 1080.0);
    let renderer_resources = RendererResources::default();
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let mut debug_messages = None;
    layout_window
        .layout_and_generate_display_list(
            styled,
            &window_state,
            &renderer_resources,
            &system_callbacks,
            &mut debug_messages,
        )
        .unwrap();

    // Find the toolbar (the only 120px-high box) and assert it sits at the
    // true origin - any UA body margin surviving the reset shifts it.
    let toolbar = (0..8_usize)
        .filter_map(|i| layout_window.get_node_layout_rect(node_id(i)))
        .find(|r| (r.size.height - 120.0).abs() < 0.5)
        .expect("toolbar rect");
    assert_eq!(
        (toolbar.origin.x, toolbar.origin.y),
        (0.0, 0.0),
        "`* {{ margin: 0 }}` must reset the UA body margin: toolbar at {:?}",
        toolbar.origin
    );
}

/// css-overflow-3 §3.1: `overflow: hidden` establishes a SCROLL CONTAINER —
/// programmatically scrollable (it gets a scroll id / scroll state), while
/// user-triggered scrolling stays disabled (it must NOT become a wheel
/// target in the hit-tester). `visible` boxes get neither.
#[test]
fn overflow_hidden_is_a_programmatic_scroll_container_but_not_a_wheel_target() {
    const CSS: &str = r#"
        body { display: flex; flex-direction: column; }
        .hidden-clip { width: 200px; height: 50px; overflow: hidden; }
        .plain { width: 200px; height: 50px; }
        .tall { width: 100px; height: 400px; }
    "#;
    let dom = Dom::create_body()
        .with_child(
            Dom::create_div()
                .with_ids_and_classes(class("hidden-clip"))
                .with_child(Dom::create_div().with_ids_and_classes(class("tall"))),
        )
        .with_child(
            Dom::create_div()
                .with_ids_and_classes(class("plain"))
                .with_child(Dom::create_div().with_ids_and_classes(class("tall"))),
        );
    let lw = layout_dom(dom, CSS, 800.0, 600.0);

    // 0 body, 1 hidden-clip, 2 tall, 3 plain, 4 tall
    let lr = lw.layout_results.get(&DomId { inner: 0 }).expect("root layout");
    let scroll_nodes: Vec<_> = lr.scroll_id_to_node_id.values().copied().collect();
    assert!(
        scroll_nodes.contains(&node_id(1).node.into_crate_internal().unwrap()),
        "overflow:hidden must register scroll state (programmatic scrolling): {scroll_nodes:?}"
    );
    assert!(
        !scroll_nodes.contains(&node_id(3).node.into_crate_internal().unwrap()),
        "a plain (overflow:visible) box must not: {scroll_nodes:?}"
    );

    // The wheel-target hit-tester must SKIP the hidden container.
    let mut tester = azul_layout::headless::CpuHitTester::new();
    tester.rebuild_from_layout_with_gpu(&lw.layout_results, None);
    assert!(
        !tester
            .debug_scroll_container_nodes()
            .contains(&node_id(1).node.into_crate_internal().unwrap()),
        "overflow:hidden must not be a user-wheel target"
    );
}

/// css-overflow-3: `overflow-inline` / `overflow-block` resolve onto the
/// physical axes through the writing mode. Declaring them used to parse and
/// then do nothing at all (the getters had zero callers).
#[test]
fn logical_overflow_properties_map_onto_physical_axes() {
    const CSS: &str = r#"
        body { display: flex; }
        .h { width: 100px; height: 100px; overflow-inline: scroll; }
    "#;
    let dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_ids_and_classes(class("h"))
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("wide wide wide wide wide")),
    );
    let lw = layout_dom(dom, CSS, 800.0, 600.0);
    let lr = lw.layout_results.get(&DomId { inner: 0 }).expect("layout");
    let nid = node_id(1).node.into_crate_internal().unwrap();
    let st = lr.styled_dom.styled_nodes.as_container()[nid].styled_node_state;
    let ox = azul_layout::solver3::getters::get_overflow_x(&lr.styled_dom, nid, &st);
    assert!(
        format!("{ox:?}").contains("Scroll"),
        "horizontal-tb: overflow-inline must resolve to overflow-x, got {ox:?}"
    );
    // And the box is registered as a user-scrollable container.
    assert!(
        lr.scroll_id_to_node_id.values().any(|n| *n == nid),
        "the overflow-inline: scroll box must register scroll state"
    );
}

/// CSS Inline 3 §6: text-box-edge selects the metric text-box-trim cuts to.
/// `cap alphabetic` must trim MORE than the default text edges: the over
/// side additionally removes (ascent - cap-height), the under side the full
/// descent. With font-size 20 / line-height 30 and the strut approximations
/// (ascent .8em, cap .7em, descent .2em): text-edge trim-both removes
/// 2 x 5px of half-leading; cap/alphabetic removes 10 + (16-14) + 4 = 16px.
#[test]
fn text_box_edge_cap_alphabetic_trims_to_the_metrics() {
    const CSS_TEXT: &str = r#"
        body { display: flex; flex-direction: column; }
        .t { font-size: 20px; line-height: 30px; text-box: trim-both text; }
    "#;
    const CSS_CAP: &str = r#"
        body { display: flex; flex-direction: column; }
        .t { font-size: 20px; line-height: 30px; text-box: trim-both cap alphabetic; }
    "#;
    let build = || {
        Dom::create_body().with_child(
            Dom::create_div()
                .with_ids_and_classes(class("t"))
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("Hello")),
        )
    };
    let h_text = layout_dom(build(), CSS_TEXT, 400.0, 300.0)
        .get_node_layout_rect(node_id(1))
        .expect("text-edge box")
        .size
        .height;
    let h_cap = layout_dom(build(), CSS_CAP, 400.0, 300.0)
        .get_node_layout_rect(node_id(1))
        .expect("cap-edge box")
        .size
        .height;
    assert!(
        h_cap < h_text - 4.0,
        "cap/alphabetic must trim deeper than the text edges: text={h_text} cap={h_cap}"
    );
    assert!(
        (h_text - h_cap - 6.0).abs() < 1.0,
        "expected ~6px extra trim ((16-14) over + 4 under): text={h_text} cap={h_cap}"
    );
}

/// css-writing-modes-4 §7.2: margin/padding percentages resolve against the
/// containing block's INLINE size — the physical HEIGHT in vertical writing
/// modes. The resolver hard-coded width, so `margin-top: 10%` in a
/// vertical-rl container resolved against 200px instead of 400px.
#[test]
fn margin_percentages_use_the_inline_size_in_vertical_writing_modes() {
    const CSS_H: &str = r#"
        * { margin: 0; padding: 0; }
        .cb { display: block; width: 200px; height: 400px; }
        .m { display: block; margin-top: 10%; width: 50px; height: 50px; }
    "#;
    const CSS_V: &str = r#"
        * { margin: 0; padding: 0; }
        .cb { display: block; width: 200px; height: 400px; writing-mode: vertical-rl; }
        .m { display: block; margin-top: 10%; width: 50px; height: 50px; writing-mode: vertical-rl; }
    "#;
    let build = || {
        Dom::create_body().with_child(
            Dom::create_div()
                .with_ids_and_classes(class("cb"))
                .with_child(Dom::create_div().with_ids_and_classes(class("m"))),
        )
    };
    let y_h = layout_dom(build(), CSS_H, 800.0, 600.0)
        .get_node_layout_rect(node_id(2))
        .expect("h child")
        .origin
        .y;
    let y_v = layout_dom(build(), CSS_V, 800.0, 600.0)
        .get_node_layout_rect(node_id(2))
        .expect("v child")
        .origin
        .y;
    assert!(
        (y_h - 20.0).abs() < 0.6,
        "horizontal-tb: 10% of width 200 = 20, got {y_h}"
    );
    assert!(
        (y_v - 40.0).abs() < 0.6,
        "vertical-rl: 10% of INLINE size (height 400) = 40, got {y_v}"
    );
}

/// The compact cache stored line-height as normalized x1000 in an i16, so
/// ANY absolute line-height above 32.76px overflowed to the sentinel and
/// decoded as `normal` — `line-height: 40px` was silently dropped. The
/// split-scale encoding keeps absolute values to ±3276.7px.
#[test]
fn absolute_line_heights_above_32px_survive_the_compact_cache() {
    const CSS: &str = r#"
        * { margin: 0; padding: 0; }
        .t { font-size: 16px; line-height: 48px; width: 300px; }
    "#;
    let dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_ids_and_classes(class("t"))
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("one line")),
    );
    let lw = layout_dom(dom, CSS, 800.0, 600.0);
    let h = lw
        .get_node_layout_rect(node_id(1))
        .expect("box")
        .size
        .height;
    assert!(
        (h - 48.0).abs() < 1.0,
        "a single line at line-height: 48px must be 48px tall, got {h}"
    );
}

/// The fold invariant, exercised with a REAL kerning-heavy system font
/// (whatever `sans-serif` resolves to on this machine): a shrink-to-fit
/// flex label sized to its own max-content must never wrap. The ribbonbug
/// reftest caught "Decrease Indent" wrapping under Noto Sans while the
/// KoHo/mock-font pins stayed green.
#[test]
fn shrink_to_fit_labels_do_not_wrap_with_the_system_sans_font() {
    const CSS: &str = r#"
        * { margin: 0; padding: 0; }
        body { display: flex; flex-direction: row; font-family: sans-serif; }
        .btn { display: flex; flex-direction: row; align-items: center;
               background: #dde8f5; padding: 2px 6px; margin: 4px;
               font-size: 16px; color: #222222; }
    "#;
    let dom = Dom::create_body()
        .with_child(
            Dom::create_div()
                .with_ids_and_classes(class("btn"))
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("Format Painter")),
        )
        .with_child(
            Dom::create_div()
                .with_ids_and_classes(class("btn"))
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("Decrease Indent")),
        )
        .with_child(
            Dom::create_div()
                .with_ids_and_classes(class("btn"))
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("No Spacing")),
        );
    let lw = layout_dom(dom, CSS, 800.0, 400.0);
    // 0 body, 1 btn, 2 text, 3 btn, 4 text, 5 btn, 6 text
    for (btn, label) in [(1_usize, "Format Painter"), (3, "Decrease Indent"), (5, "No Spacing")] {
        let r = lw.get_node_layout_rect(node_id(btn)).expect("btn rect");
        assert!(
            r.size.height < 30.0,
            "'{label}' wrapped (button {btn} is {}px tall)",
            r.size.height
        );
    }
}

/// `border-collapse: collapse` must zero out border-spacing between cells
/// (CSS 2.2 section 17.6.2: the collapsing model has no border-spacing).
/// table-basic-001 renders visible gaps between header cells because the
/// separate-borders path runs despite `border-collapse: collapse`.
#[test]
fn border_collapse_collapse_suppresses_border_spacing() {
    const CSS: &str = r#"
        * { margin: 0; padding: 0; }
        .tbl { display: table; border-collapse: collapse; border-spacing: 10px; }
        .row { display: table-row; }
        .c { display: table-cell; width: 50px; height: 20px; }
    "#;
    let dom = Dom::create_body().with_child(
        Dom::create_div().with_ids_and_classes(class("tbl")).with_child(
            Dom::create_div()
                .with_ids_and_classes(class("row"))
                .with_child(Dom::create_div().with_ids_and_classes(class("c")))
                .with_child(Dom::create_div().with_ids_and_classes(class("c"))),
        ),
    );
    let lw = layout_dom(dom, CSS, 800.0, 600.0);
    let c1 = lw.get_node_layout_rect(node_id(3)).expect("cell 1");
    let c2 = lw.get_node_layout_rect(node_id(4)).expect("cell 2");
    let gap = c2.origin.x - (c1.origin.x + c1.size.width);
    assert!(
        gap.abs() < 0.5,
        "collapsed table must have no border-spacing between cells, got a {gap}px gap"
    );
    assert!(
        (c1.origin.x - 0.0).abs() < 0.5,
        "collapsed table must not inset the first cell by border-spacing, cell 1 starts at {}",
        c1.origin.x
    );
}

/// Table-cell padding must surround the cell text symmetrically: with
/// `padding: 8px` the text fragment starts 8px below the cell top (chrome
/// centers it naturally; azul painted the text hugging the cell bottom in
/// table-basic-001).
#[test]
fn table_cell_padding_offsets_text_from_the_cell_top() {
    const CSS: &str = r#"
        * { margin: 0; padding: 0; }
        body { font-size: 14px; }
        .tbl { display: table; border-collapse: collapse; }
        .row { display: table-row; }
        .c { display: table-cell; padding: 8px; }
    "#;
    let dom = Dom::create_body().with_child(
        Dom::create_div().with_ids_and_classes(class("tbl")).with_child(
            Dom::create_div().with_ids_and_classes(class("row")).with_child(
                Dom::create_div()
                    .with_ids_and_classes(class("c"))
                    .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("Red 1")),
            ),
        ),
    );
    let lw = layout_dom(dom, CSS, 800.0, 600.0);
    let cell = lw.get_node_layout_rect(node_id(3)).expect("cell");
    let text = lw.get_node_layout_rect(node_id(4)).expect("text");
    let top_inset = text.origin.y - cell.origin.y;
    assert!(
        (top_inset - 8.0).abs() < 1.5,
        "text must start ~8px below the cell top (padding), got {top_inset}px (cell h {})",
        cell.size.height
    );
}

/// Collapsed-border painting must cover header rows too: the resolved
/// 2px black strip between two `<th>` cells (and the black strip on the
/// header/body boundary) must exist in the display list. table-basic-001
/// renders the tbody edges but drops every edge touching the thead row.
#[test]
fn collapsed_borders_paint_across_header_rows() {
    use azul_layout::solver3::display_list::DisplayListItem;
    const CSS: &str = r#"
        * { margin: 0; padding: 0; }
        .tbl { display: table; border-collapse: collapse; }
        .hgrp { display: table-header-group; }
        .bgrp { display: table-row-group; }
        .row { display: table-row; }
        .th { display: table-cell; width: 50px; height: 20px; border: 2px solid #000000; background: #333333; }
        .td { display: table-cell; width: 50px; height: 20px; border: 1px solid #999999; background: #ffcccc; }
    "#;
    let dom = Dom::create_body().with_child(
        Dom::create_div().with_ids_and_classes(class("tbl"))
            .with_child(
                Dom::create_div().with_ids_and_classes(class("hgrp")).with_child(
                    Dom::create_div().with_ids_and_classes(class("row"))
                        .with_child(Dom::create_div().with_ids_and_classes(class("th")))
                        .with_child(Dom::create_div().with_ids_and_classes(class("th"))),
                ),
            )
            .with_child(
                Dom::create_div().with_ids_and_classes(class("bgrp")).with_child(
                    Dom::create_div().with_ids_and_classes(class("row"))
                        .with_child(Dom::create_div().with_ids_and_classes(class("td")))
                        .with_child(Dom::create_div().with_ids_and_classes(class("td"))),
                ),
            ),
    );
    let lw = layout_dom(dom, CSS, 800.0, 600.0);
    // Boundary between the two header cells, from the real layout geometry
    // (the test harness sizes cells to content, not to the declared width).
    let th1 = lw.get_node_layout_rect(node_id(4)).expect("first th");
    let th_boundary = th1.origin.x + th1.size.width;
    let th_bottom = th1.origin.y + th1.size.height;
    let result = lw.get_layout_result(&DomId::ROOT_ID).expect("layout result");
    let mut black_vertical_between_ths = false;
    let mut black_horizontal_under_header = false;
    // Paint-order guard: cell BACKGROUNDS must never be emitted after the
    // resolved border strips (a later background re-paint covers them).
    let mut last_cell_bg_idx = None;
    let mut first_strip_idx = None;
    for (idx, item) in result.display_list.items.iter().enumerate() {
        if let DisplayListItem::Rect { bounds, color, .. } = item {
            let is_cell_bg = (color.r == 51 && color.g == 51 && color.b == 51)
                || (color.r == 255 && color.g == 204 && color.b == 204);
            let thin = bounds.size().width < 3.0 || bounds.size().height < 3.0;
            if is_cell_bg && !thin {
                last_cell_bg_idx = Some(idx);
            }
            if thin && color.a > 0 {
                first_strip_idx.get_or_insert(idx);
            }
        }
    }
    if let (Some(bg), Some(strip)) = (last_cell_bg_idx, first_strip_idx) {
        assert!(
            bg < strip,
            "cell background (item {bg}) painted AFTER a collapsed border strip (item {strip}) — it covers the border"
        );
    }
    for item in &result.display_list.items {
        if let DisplayListItem::Rect { bounds, color, .. } = item {
            let is_black = color.r == 0 && color.g == 0 && color.b == 0 && color.a > 0;
            if !is_black {
                continue;
            }
            let (w, h) = (bounds.size().width, bounds.size().height);
            let x = bounds.origin().x;
            let y = bounds.origin().y;
            // vertical 2px strip centered on the th/th boundary
            if (w - 2.0).abs() < 0.6 && h > 10.0 && (x + w * 0.5 - th_boundary).abs() < 1.5 {
                black_vertical_between_ths = true;
            }
            // horizontal 2px strip on the header/body boundary
            if (h - 2.0).abs() < 0.6 && w > 3.0 && (y + h * 0.5 - th_bottom).abs() < 2.5 {
                black_horizontal_under_header = true;
            }
        }
    }
    assert!(
        black_vertical_between_ths,
        "missing the resolved 2px black strip between the two th cells"
    );
    assert!(
        black_horizontal_under_header,
        "missing the resolved 2px black strip on the header/body row boundary"
    );
}

/// Ground-truth probe: real <table>/<thead>/<th>/<td> nodes (UA stylesheet
/// applies), replicating table-basic-001's header cell. Chrome centers the
/// text (UA vertical-align: middle on cells); azul painted it 9px lower.
#[test]
fn real_table_cells_center_their_text_vertically() {
    const CSS: &str = r#"
        * { margin: 0; padding: 0; }
        body { font-size: 14px; }
        table { border-collapse: collapse; width: 400px; }
        th { background: #333333; color: #ffffff; padding: 10px; border: 2px solid #000000; text-align: left; }
    "#;
    let mut dom = Dom::create_body().with_child(
        Dom::create_table_no_a11y().with_child(
            Dom::create_thead().with_child(
                Dom::create_tr()
                    .with_child(Dom::create_th().with_child(Dom::create_text_do_not_use_without_block_level_wrapper("Header A")))
                    .with_child(Dom::create_th().with_child(Dom::create_text_do_not_use_without_block_level_wrapper("Header B"))),
            ),
        ),
    );
    let (css, _) = azul_css::parser2::new_from_str(CSS);
    let styled_dom = StyledDom::create(&mut dom, css);
    let mut layout_window = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(800.0, 600.0);
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
    let mut y_offset: Option<f32> = None;
    for m in debug_messages.as_deref().unwrap_or_default() {
        let msg = m.message.as_str();
        if msg.contains("[position_table_cells]") && msg.contains("vertical-align") {
            eprintln!("PROBE {msg}");
            if let Some(pos) = msg.find("y_offset=") {
                y_offset = msg[pos + 9..].trim_end().parse::<f32>().ok().or(y_offset);
            }
        }
    }
    let y = y_offset.expect("no vertical-align debug line for any cell");
    assert!(
        y.abs() < 2.5,
        "header cell text must sit at the padding edge (row height equals \
         content height, so EVERY alignment gives ~0), got y_offset={y}"
    );

    // End-to-end glyph check: the white header glyphs must start ~2px
    // (border) + 10px (padding) + a small cap-vs-ascent gap below the cell
    // top, like Chrome. The regression painted them ~9px lower.
    use azul_layout::solver3::display_list::DisplayListItem;
    let th_rect = lw_rect_of(&layout_window, 4);
    let result = layout_window
        .get_layout_result(&DomId::ROOT_ID)
        .expect("layout result");
    let mut glyph_min_y: Option<f32> = None;
    for item in &result.display_list.items {
        if let DisplayListItem::Text { glyphs, color, .. } = item {
            if color.r == 255 && color.g == 255 && color.b == 255 {
                for g in glyphs {
                    let y = g.point.y;
                    glyph_min_y = Some(glyph_min_y.map_or(y, |m: f32| m.min(y)));
                }
            }
        }
    }
    let baseline_y = glyph_min_y.expect("no white glyphs in the display list");
    // baseline sits at border(2) + padding(10) + ascent(~15.2 at 14px)
    let expected = th_rect.origin.y + 2.0 + 10.0 + 15.2;
    assert!(
        (baseline_y - expected).abs() < 3.0,
        "header glyph baseline must sit at the padding-box top plus the \
         ascent ({expected:.1}), got {baseline_y:.1} (cell top {:.1})",
        th_rect.origin.y
    );
}

/// Layout rect of node `n`, panicking with context.
fn lw_rect_of(lw: &LayoutWindow, n: usize) -> azul_core::geom::LogicalRect {
    lw.get_node_layout_rect(node_id(n)).expect("node rect")
}

/// Advance parity probe: a run of N identical glyphs must measure N times
/// the linear (design-space) advance, like Chrome/HarfBuzz. Azul renders
/// long lines ~0.19px/glyph wider than Chrome (cascade-* reftests), which
/// points at per-glyph advance quantization somewhere in shaping.
#[test]
fn glyph_advances_stay_linear_and_unquantized() {
    const CSS: &str = r#"
        * { margin: 0; padding: 0; }
        body { font-family: "Noto Sans"; font-size: 14px; }
        .m { display: inline-block; }
    "#;
    // Noto Sans design advances at upem 1000: 'l' = 268? measured via FT:
    // the exact per-glyph value doesn't matter — LINEARITY does: width of
    // 40 glyphs must be exactly 4x the width of 10 glyphs (no per-glyph
    // rounding), and both must be within 1px of the FT linear sum ratio.
    let w10 = {
        let dom = Dom::create_body().with_child(
            Dom::create_div().with_ids_and_classes(class("m"))
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("llllllllll")),
        );
        layout_dom(dom, CSS, 800.0, 600.0)
            .get_node_layout_rect(node_id(1)).expect("10-run").size.width
    };
    let (w40, expected) = {
        let dom = Dom::create_body().with_child(
            Dom::create_div().with_ids_and_classes(class("m"))
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("llllllllllllllllllllllllllllllllllllllll")),
        );
        let lw = layout_dom(dom, CSS, 800.0, 600.0);
        let w = lw.get_node_layout_rect(node_id(1)).expect("40-run").size.width;
        // Hermetic expectation: take the font THE LAYOUT ACTUALLY USED,
        // straight from its own display list, and read the linear design
        // advance from that font's tables. Re-resolving "Noto Sans" through
        // a second query can (and did) pick a DIFFERENT Noto cut than the
        // engine's resolution — the law is "layout == its own font's linear
        // advance", so the font must come from the layout itself.
        use azul_layout::solver3::display_list::DisplayListItem;
        let result = lw.layout_results.get(&DomId::ROOT_ID).expect("layout result");
        let font_hash = result
            .display_list
            .items
            .iter()
            .find_map(|i| match i {
                DisplayListItem::Text { font_hash, .. } => Some(font_hash.font_hash),
                _ => None,
            })
            .expect("a text item in the display list");
        let font_ref = lw
            .font_manager
            .get_font_by_hash(font_hash)
            .expect("the display list's font hash must resolve in the font manager");
        let font = azul_layout::font_ref_to_parsed_font(&font_ref);
        let gid = font.lookup_glyph_index('l' as u32).expect("'l' maps to a glyph");
        let expected = font.get_horizontal_advance(gid) as f32
            / font.font_metrics.units_per_em as f32
            * 14.0;
        (w, expected)
    };
    let per_glyph_10 = w10 / 10.0;
    let per_glyph_40 = w40 / 40.0;
    assert!(
        (per_glyph_10 - per_glyph_40).abs() < 0.01,
        "per-glyph advance must not depend on run length (quantization!): \
         10-run {per_glyph_10:.4}px/glyph vs 40-run {per_glyph_40:.4}px/glyph"
    );
    // `expected` computed above from the layout's own font (see the 40-run
    // block): the anti-quantization law, checked against that font's tables.
    assert!(
        (per_glyph_40 - expected).abs() < 0.05,
        "advance must be the resolved font's linear design advance \
         ({expected:.4}px for 'l' at 14px), got {per_glyph_40:.4}px - \
         hinted-quantized advances leak into layout"
    );
}

/// grid-template-areas placement: the classic header/sidebar/main/aside/
/// footer layout. Distilled from grid-template-areas-001, where azul
/// scatters the items instead of honoring their named areas.
#[test]
fn grid_template_areas_place_items_in_their_named_cells() {
    const CSS: &str = r#"
        * { margin: 0; padding: 0; }
        .container {
            display: grid;
            width: 600px;
            height: 400px;
            grid-template-columns: 100px 1fr 100px;
            grid-template-rows: 50px 1fr 30px;
            grid-template-areas:
                "header header header"
                "sidebar main aside"
                "footer footer footer";
        }
        .header { grid-area: header; }
        .sidebar { grid-area: sidebar; }
        .main { grid-area: main; }
        .aside { grid-area: aside; }
        .footer { grid-area: footer; }
    "#;
    let dom = Dom::create_body().with_child(
        Dom::create_div().with_ids_and_classes(class("container"))
            .with_child(Dom::create_div().with_ids_and_classes(class("header")))
            .with_child(Dom::create_div().with_ids_and_classes(class("sidebar")))
            .with_child(Dom::create_div().with_ids_and_classes(class("main")))
            .with_child(Dom::create_div().with_ids_and_classes(class("aside")))
            .with_child(Dom::create_div().with_ids_and_classes(class("footer"))),
    );
    let lw = layout_dom(dom, CSS, 800.0, 600.0);
    let r = |n: usize| lw.get_node_layout_rect(node_id(n)).expect("rect");
    let (header, sidebar, main, aside, footer) = (r(2), r(3), r(4), r(5), r(6));

    assert!(
        (header.size.width - 600.0).abs() < 1.0 && (header.size.height - 50.0).abs() < 1.0
            && header.origin.y.abs() < 1.0,
        "header must span all columns in row 1: got {header:?}"
    );
    assert!(
        sidebar.origin.x.abs() < 1.0 && (sidebar.origin.y - 50.0).abs() < 1.0
            && (sidebar.size.width - 100.0).abs() < 1.0 && (sidebar.size.height - 320.0).abs() < 1.0,
        "sidebar must fill column 1 of row 2: got {sidebar:?}"
    );
    assert!(
        (main.origin.x - 100.0).abs() < 1.0 && (main.size.width - 400.0).abs() < 1.0
            && (main.size.height - 320.0).abs() < 1.0,
        "main must fill the 1fr center: got {main:?}"
    );
    assert!(
        (aside.origin.x - 500.0).abs() < 1.0 && (aside.size.width - 100.0).abs() < 1.0,
        "aside must fill column 3 of row 2: got {aside:?}"
    );
    assert!(
        (footer.origin.y - 370.0).abs() < 1.0 && (footer.size.width - 600.0).abs() < 1.0
            && (footer.size.height - 30.0).abs() < 1.0,
        "footer must span all columns in row 3: got {footer:?}"
    );
}

/// minmax()/fr track sizing: three minmax(min, Xfr) columns in an 800px
/// grid resolve to the fr shares (200/400/200), and auto-placed items
/// STRETCH to fill their cells (grid default). Distilled from
/// grid-minmax-fr-001, where azul renders items at content size in
/// wrong positions.
#[test]
fn grid_minmax_fr_tracks_resolve_and_items_stretch() {
    const CSS: &str = r#"
        * { margin: 0; padding: 0; }
        .g {
            display: grid;
            width: 800px;
            height: 300px;
            grid-template-columns: minmax(150px, 1fr) minmax(200px, 2fr) minmax(100px, 1fr);
            grid-template-rows: minmax(100px, 1fr);
        }
        .i { }
    "#;
    let dom = Dom::create_body().with_child(
        Dom::create_div().with_ids_and_classes(class("g"))
            .with_child(Dom::create_div().with_ids_and_classes(class("i")))
            .with_child(Dom::create_div().with_ids_and_classes(class("i")))
            .with_child(Dom::create_div().with_ids_and_classes(class("i")))
    );
    let lw = layout_dom(dom, CSS, 1000.0, 700.0);
    let r = |n: usize| lw.get_node_layout_rect(node_id(n)).expect("rect");
    let (a, b, c) = (r(2), r(3), r(4));
    assert!(
        (a.size.width - 200.0).abs() < 1.0 && a.origin.x.abs() < 1.0,
        "col 1 must be 200px (1fr of 800 with 150px floor), item stretched: got {a:?}"
    );
    assert!(
        (b.size.width - 400.0).abs() < 1.0 && (b.origin.x - 200.0).abs() < 1.0,
        "col 2 must be 400px (2fr): got {b:?}"
    );
    assert!(
        (c.size.width - 200.0).abs() < 1.0 && (c.origin.x - 600.0).abs() < 1.0,
        "col 3 must be 200px (1fr with 100px floor): got {c:?}"
    );
    assert!(
        (a.size.height - 300.0).abs() < 1.0,
        "single minmax(100px,1fr) row must fill the 300px container, item stretched: got {a:?}"
    );
}

/// Same grid as grid_minmax_fr_tracks_resolve_and_items_stretch, but built
/// through the XML pipeline the reftests use (whitespace text nodes between
/// the items!). In the rendered reftest the items land at every SECOND
/// auto-placement cell (item N in cell 2N), so something in the XML-built
/// DOM occupies the odd cells.
#[test]
fn grid_items_from_xml_markup_fill_consecutive_cells() {
    const XML: &str = r#"<html><head><style type="text/css">
        * { margin: 0; padding: 0; }
        .g {
            display: grid;
            width: 800px;
            height: 300px;
            grid-template-columns: minmax(150px, 1fr) minmax(200px, 2fr) minmax(100px, 1fr);
            grid-template-rows: minmax(100px, 1fr);
        }
    </style></head>
    <body>
        <div class="g">
            <div class="i"></div>
            <div class="i"></div>
            <div class="i"></div>
        </div>
    </body></html>"#;
    use azul_layout::xml::domxml_from_str;
    use azul_core::xml::ComponentMap;
    let component_map = ComponentMap::default();
    let dom_xml = domxml_from_str(XML, &component_map);
    // The XML path styles internally (the page's <style> is applied).
    let styled_dom = dom_xml.parsed_dom;
    let mut layout_window = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(1000.0, 700.0);
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
    // Find the three item rects: nodes are parser-defined, so scan for
    // 3 sibling rects of equal height inside an 800px-wide parent.
    let result = layout_window.get_layout_result(&DomId::ROOT_ID).expect("result");
    let mut rects: Vec<(usize, azul_core::geom::LogicalRect)> = Vec::new();
    for n in 0..result.styled_dom.node_data.len() {
        if let Some(r) = layout_window.get_node_layout_rect(node_id(n)) {
            rects.push((n, r));
        }
    }
    for (n, r) in &rects {
        eprintln!("XMLGRID node {n}: {:?} @ {:?}", r.size, r.origin);
    }
    let items: Vec<_> = rects
        .iter()
        .filter(|(_, r)| (r.size.height - 300.0).abs() < 1.0 && r.size.width < 500.0)
        .collect();
    assert!(
        items.len() >= 3,
        "expected 3 grid items 300px tall, found {}",
        items.len()
    );
    let xs: Vec<f32> = items.iter().map(|(_, r)| r.origin.x).collect();
    assert!(
        xs.windows(2).all(|w| w[1] > w[0]) && (xs[0]).abs() < 1.0,
        "items must fill consecutive cells starting at x=0: xs={xs:?}"
    );
}

/// CSS 2.2 section 8.3.1: a last in-flow child's bottom margin adjoins its
/// parent's bottom margin when the parent has auto height and no bottom
/// padding/border - even when the CHILD has its own padding (that only
/// isolates the child's descendants). Distilled from
/// block-margin-collapse-complex-001: parent margin-bottom 40 with a padded
/// child of margin-bottom 50 must leave a 50px gap to the next sibling
/// (max(40,50,next top 30)), not 40.
#[test]
fn padded_last_child_bottom_margin_collapses_into_the_parents() {
    const CSS: &str = r#"
        * { margin: 0; padding: 0; }
        .wrap { width: 400px; background: #ffffff; }
        .parent { margin: 40px 0; }
        .child { margin: 50px 0; padding: 15px; height: 40px; }
        .after { margin-top: 30px; height: 20px; }
    "#;
    let dom = Dom::create_body().with_child(
        Dom::create_div().with_ids_and_classes(class("wrap"))
            .with_child(
                Dom::create_div().with_ids_and_classes(class("parent"))
                    .with_child(Dom::create_div().with_ids_and_classes(class("child"))),
            )
            .with_child(Dom::create_div().with_ids_and_classes(class("after"))),
    );
    let lw = layout_dom(dom, CSS, 800.0, 600.0);
    let child = lw.get_node_layout_rect(node_id(3)).expect("child");
    let after = lw.get_node_layout_rect(node_id(4)).expect("after");
    let child_bottom = child.origin.y + child.size.height;
    let gap = after.origin.y - child_bottom;
    assert!(
        (gap - 50.0).abs() < 1.0,
        "gap below the padded child must be max(child mb 50, parent mb 40, next mt 30) = 50, got {gap}"
    );
}

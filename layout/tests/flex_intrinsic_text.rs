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
            Dom::create_text(label).with_ids_and_classes(class("lbl")),
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
                RibbonGalleryCell::new(Dom::create_text("AaBbCcDc"), "Normal".into()),
                RibbonGalleryCell::new(Dom::create_text("AaBbCcDc"), "No Spacing".into()),
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

    // Locate labels by text content; their parent is the Button container.
    let node_data = result.styled_dom.node_data.as_container();
    let hierarchy = result.styled_dom.node_hierarchy.as_container();
    let find_label = |needle: &str| -> (usize, usize) {
        for i in 0..node_data.len() {
            if let NodeType::Text(t) = node_data[NodeId::new(i)].get_node_type() {
                if t.as_ref().as_str() == needle {
                    let parent = hierarchy[NodeId::new(i)]
                        .parent_id()
                        .expect("label has a parent");
                    return (i, parent.index());
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
        .map(|i| RibbonGalleryCell::new(Dom::create_text("AaBbCcDc"), format!("Style {i}").into()))
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
                    let parent = hierarchy[NodeId::new(i)]
                        .parent_id()
                        .expect("label has a parent");
                    return (i, parent.index());
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
        .with_child(Dom::create_text("X").with_ids_and_classes(class("glyph")))
        .with_child(Dom::create_text("Format Painter").with_ids_and_classes(class("lbl")));
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
        .with_child(Dom::create_text("Format Painter").with_ids_and_classes(class("lbl")));
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

    // 0 body, 1 btn, 2 glyph(icon->text), 3 lbl
    let btn_rect = layout_window.get_node_layout_rect(node_id(1)).expect("btn rect");
    let lbl_rect = layout_window.get_node_layout_rect(node_id(3)).expect("lbl rect");
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
    use azul_core::dom::{DomId as CoreDomId, NodeType};
    use azul_layout::headless::CpuHitTester;
    use azul_layout::widgets::ribbon::{
        Ribbon, RibbonGallery, RibbonGalleryCell, RibbonGroup, RibbonItem, RibbonTab,
        RibbonTabVec,
    };

    let cells: Vec<RibbonGalleryCell> = (0..8)
        .map(|i| RibbonGalleryCell::new(Dom::create_text("AaBbCcDc"), format!("Style {i}").into()))
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
        .map(|i| RibbonGalleryCell::new(Dom::create_text("AaBbCcDc"), format!("Style {i}").into()))
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
        .with_child(Dom::create_text("Document1 - Word"))
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
#[ignore = "engine gap: inline viewport conditions are not evaluated by the production cascade"]
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

    let mut panel = Dom::create_div().with_child(Dom::create_text("touch"));
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

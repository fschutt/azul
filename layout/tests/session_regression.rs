// Regression tests for bugs fixed in sessions 1-5 (commits 91c1ceba..064eac20).
//
// Each test documents WHAT went wrong, WHY, and verifies the fix.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId, NodeType},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_css::css::Css;
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

// =========================================================================
// Helpers
// =========================================================================

fn nid(idx: u32) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(idx as usize))),
    }
}

fn do_layout(dom: Dom, css_str: &str, w: f32, h: f32) -> LayoutWindow {
    let css = if css_str.is_empty() {
        Css::empty()
    } else {
        Css::from_string(css_str.into())
    };
    let mut dom = dom;
    let styled_dom = StyledDom::create(&mut dom, css);
    let font_cache = FcFontCache::build();
    let mut lw = LayoutWindow::new(font_cache).unwrap();
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(w, h);
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    let mut dbg = Some(Vec::new());
    lw.layout_and_generate_display_list(styled_dom, &ws, &rr, &sc, &mut dbg)
        .unwrap();
    lw
}

fn do_cascade(dom: Dom, css_str: &str) -> StyledDom {
    let css = if css_str.is_empty() {
        Css::empty()
    } else {
        Css::from_string(css_str.into())
    };
    let mut dom = dom;
    StyledDom::create(&mut dom, css)
}

fn cls(name: &str) -> Vec<IdOrClass> {
    vec![IdOrClass::Class(name.into())]
}

// =========================================================================
// Bug 16 — Vec::insert corruption (064eac20)
// BTreeMap→Vec refactoring left Vec::insert (shift-insert) where
// indexed assignment was needed, corrupting sibling positions.
// =========================================================================

#[test]
fn test_absolute_sibling_positions_not_corrupted() {
    let container = Dom::create_div()
        .with_ids_and_classes(cls("container").into())
        .with_child(Dom::create_div().with_ids_and_classes(cls("s1").into()))
        .with_child(Dom::create_div().with_ids_and_classes(cls("abs").into()))
        .with_child(Dom::create_div().with_ids_and_classes(cls("s2").into()));
    let dom = Dom::create_body().with_child(container);

    let css = r#"
        * { margin: 0; padding: 0; }
        body { width: 400px; height: 300px; }
        .container { position: relative; width: 400px; padding: 10px; }
        .s1 { width: 100px; height: 50px; margin: 10px; }
        .abs { position: absolute; top: 0; right: 0; width: 40px; height: 40px; }
        .s2 { width: 100px; height: 50px; margin: 10px; }
    "#;

    let lw = do_layout(dom, css, 400.0, 300.0);

    // s2 should be below s1, not at y=0 due to corruption
    // nodes: 0=body, 1=container, 2=s1, 3=abs, 4=s2
    let r = lw.get_node_layout_rect(nid(4)).expect("s2");
    assert!(
        r.origin.y > 50.0,
        "s2 y={:.0} should be >50 (below s1). Vec::insert corruption?",
        r.origin.y
    );
}

// =========================================================================
// Bug 15a — white-space:nowrap not enforced (fc20a0cb)
// break_one_line() ignored white_space_mode for Nowrap/Pre.
// =========================================================================

#[test]
fn test_whitespace_nowrap_single_line() {
    let dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_ids_and_classes(cls("box").into())
            .with_child(Dom::create_text(
                "This is a very long text that would normally wrap to multiple lines in a narrow box",
            )),
    );

    let css = r#"
        * { margin: 0; padding: 0; }
        body { width: 800px; height: 600px; font-family: sans-serif; font-size: 14px; }
        .box { width: 100px; white-space: nowrap; overflow: hidden; }
    "#;

    let lw = do_layout(dom, css, 800.0, 600.0);
    let r = lw.get_node_layout_rect(nid(1)).expect("box");
    assert!(
        r.size.height < 30.0,
        "nowrap box height={:.0} should be single-line (<30). nowrap not enforced?",
        r.size.height
    );
}

// =========================================================================
// Bug 15b — inline-block intrinsic sizes double-counted (fc20a0cb)
// margin+padding+border added at InlineBlock level AND by callers.
// =========================================================================

#[test]
fn test_inline_block_auto_width_not_inflated() {
    let dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_ids_and_classes(cls("wrap").into())
            .with_child(
                Dom::create_node(NodeType::Span)
                    .with_ids_and_classes(cls("ib").into())
                    .with_child(Dom::create_text("AB")),
            ),
    );

    let css = r#"
        * { margin: 0; padding: 0; }
        body { width: 800px; height: 600px; font-family: sans-serif; font-size: 16px; }
        .wrap { width: 800px; }
        .ib { display: inline-block; padding: 10px; }
    "#;

    let lw = do_layout(dom, css, 800.0, 600.0);
    // nodes: 0=body, 1=wrap, 2=ib
    let r = lw.get_node_layout_rect(nid(2)).expect("ib");
    assert!(
        r.size.width < 80.0 && r.size.width > 20.0,
        "inline-block width={:.0} should be 30-70 (shrink-to-fit). Double-counted?",
        r.size.width
    );
}

// =========================================================================
// Bug 15c — table intrinsic sizing stub (fc20a0cb)
// calculate_table_intrinsic_sizes() returned all zeros.
// =========================================================================

#[test]
fn test_table_auto_width_nonzero() {
    let dom = Dom::create_body().with_child(
        Dom::create_node(NodeType::Table).with_child(
            Dom::create_node(NodeType::Tr)
                .with_child(Dom::create_node(NodeType::Td).with_child(Dom::create_text("Hi")))
                .with_child(Dom::create_node(NodeType::Td).with_child(Dom::create_text("There"))),
        ),
    );

    let css = r#"
        * { margin: 0; padding: 0; }
        body { width: 800px; height: 600px; font-family: sans-serif; font-size: 14px; }
    "#;

    let lw = do_layout(dom, css, 800.0, 600.0);
    let r = lw.get_node_layout_rect(nid(1)).expect("table");
    assert!(r.size.width > 10.0, "table width={:.0} must be >10", r.size.width);
    assert!(r.size.height > 5.0, "table height={:.0} must be >5", r.size.height);
}

// =========================================================================
// Bug 14a — * selector applied to text nodes (43535c21)
// * should only match elements, not text nodes.
// =========================================================================

#[test]
fn test_star_selector_skips_text_nodes() {
    let dom = Dom::create_body().with_child(
        Dom::create_node(NodeType::P).with_child(Dom::create_text("Hello")),
    );

    let css = "* { color: #666666; } p { color: #ff0000; }";
    let s = do_cascade(dom, css);

    // nodes: 0=body, 1=p, 2=text
    let cc = s.css_property_cache.ptr.compact_cache.as_ref().unwrap();
    let p_color = cc.tier2b_text[1].text_color;
    let text_color = cc.tier2b_text[2].text_color;

    // Text node should inherit from p (red), not from * (gray)
    assert_eq!(
        p_color, text_color,
        "text color 0x{:08x} should equal p color 0x{:08x} (inherit, not * override)",
        text_color, p_color
    );
}

// =========================================================================
// Bug 14c — font-size em resolved against default, not parent (43535c21)
// Compact cache inherited raw em tokens instead of computed px.
// =========================================================================

#[test]
fn test_font_size_em_resolves_against_parent() {
    // Parent: font-size 20px, child: 1.5em → should resolve to 30px
    // A line of text at 30px should be taller than at 20px
    let dom = Dom::create_body()
        .with_child(
            Dom::create_div()
                .with_ids_and_classes(cls("p20").into())
                .with_child(Dom::create_text("X")),
        )
        .with_child(
            Dom::create_div()
                .with_ids_and_classes(cls("p20").into())
                .with_child(
                    Dom::create_div()
                        .with_ids_and_classes(cls("em15").into())
                        .with_child(Dom::create_text("X")),
                ),
        );

    let css = r#"
        * { margin: 0; padding: 0; }
        body { width: 800px; height: 600px; font-family: sans-serif; }
        .p20 { font-size: 20px; }
        .em15 { font-size: 1.5em; }
    "#;

    let lw = do_layout(dom, css, 800.0, 600.0);
    // nodes: 0=body, 1=.p20(first), 2=text, 3=.p20(second), 4=.em15, 5=text
    let r_20px = lw.get_node_layout_rect(nid(1)).expect("20px div");
    let r_30px = lw.get_node_layout_rect(nid(4)).expect("1.5em div");

    // 1.5em of 20px = 30px, so the 30px text should be taller
    assert!(
        r_30px.size.height > r_20px.size.height,
        "1.5em div height ({:.0}) should be > 20px div height ({:.0}). em not resolved against parent?",
        r_30px.size.height,
        r_20px.size.height
    );
}

// =========================================================================
// Bug 13 — margin collapsing ignored in intrinsic height (2fe02b4d)
// Intrinsic height summed full margins instead of collapsing siblings.
// =========================================================================

#[test]
fn test_auto_height_collapses_sibling_margins() {
    let parent = Dom::create_div()
        .with_ids_and_classes(cls("p").into())
        .with_child(Dom::create_div().with_ids_and_classes(cls("c").into()))
        .with_child(Dom::create_div().with_ids_and_classes(cls("c").into()))
        .with_child(Dom::create_div().with_ids_and_classes(cls("c").into()));
    let dom = Dom::create_body().with_child(parent);

    let css = r#"
        * { margin: 0; padding: 0; }
        body { width: 800px; height: 600px; }
        .p { width: 400px; padding: 10px; }
        .c { height: 50px; margin: 20px; }
    "#;

    let lw = do_layout(dom, css, 800.0, 600.0);
    let r = lw.get_node_layout_rect(nid(1)).expect("parent");
    // With collapsing: 20+50+20+50+20+50+20 = 230 content + 20 padding = 250
    // Without collapsing: 3*(20+50+20) = 270 content + 20 padding = 290
    assert!(
        r.size.height < 280.0,
        "parent height={:.0} should be <280 (margins collapsed). Summing?",
        r.size.height
    );
}

// =========================================================================
// Bug 10 — display:none not filtered in reconciler (beec64bf)
// collect_children_dom_ids lacked the filter.
// =========================================================================

#[test]
fn test_display_none_excluded_from_layout() {
    let dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_ids_and_classes(cls("w").into())
            .with_child(Dom::create_div().with_ids_and_classes(cls("v").into()))
            .with_child(Dom::create_div().with_ids_and_classes(cls("h").into()))
            .with_child(Dom::create_div().with_ids_and_classes(cls("a").into())),
    );

    let css = r#"
        * { margin: 0; padding: 0; }
        body { width: 800px; height: 600px; }
        .w { width: 400px; }
        .v { height: 50px; }
        .h { height: 100px; display: none; }
        .a { height: 50px; }
    "#;

    let lw = do_layout(dom, css, 800.0, 600.0);
    let r = lw.get_node_layout_rect(nid(1)).expect("wrap");
    assert!(
        r.size.height < 150.0,
        "wrap height={:.0} should be ~100 (display:none excluded). Got ~200?",
        r.size.height
    );
}

// =========================================================================
// Bug 12 — descendant selector checked only one parent (569b2842)
// =========================================================================

#[test]
fn test_descendant_selector_matches_deeply_nested() {
    let dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_ids_and_classes(cls("outer").into())
            .with_child(
                Dom::create_div().with_child(
                    Dom::create_node(NodeType::P).with_child(Dom::create_text("deep")),
                ),
            ),
    );

    let css = ".outer p { color: #ff0000; }";
    let s = do_cascade(dom, css);

    // nodes: 0=body, 1=.outer, 2=div, 3=p, 4=text
    let cc = s.css_property_cache.ptr.compact_cache.as_ref().unwrap();
    let p_color = cc.tier2b_text[3].text_color;
    let r = (p_color >> 24) & 0xFF;
    assert!(
        r > 200,
        "p should be red from '.outer p' (descendant). color=0x{:08x}",
        p_color
    );
}

// =========================================================================
// Bug 5 — table cell IFC ownership (07513f82)
// Both table layout and BFC pass created IFC results for same content.
// =========================================================================

#[test]
fn test_table_cell_has_content() {
    let dom = Dom::create_body().with_child(
        Dom::create_node(NodeType::Table).with_child(
            Dom::create_node(NodeType::Tr).with_child(
                Dom::create_node(NodeType::Td).with_child(Dom::create_text("Cell")),
            ),
        ),
    );

    let css = r#"
        * { margin: 0; padding: 0; }
        body { width: 800px; height: 600px; font-family: sans-serif; font-size: 14px; }
    "#;

    let lw = do_layout(dom, css, 800.0, 600.0);
    // nodes: 0=body, 1=table, 2=tr, 3=td
    let r = lw.get_node_layout_rect(nid(3)).expect("td");
    assert!(r.size.height > 5.0, "cell height={:.0} must be >5", r.size.height);
    assert!(r.size.width > 5.0, "cell width={:.0} must be >5", r.size.width);
}

// =========================================================================
// Bug 9 — table cell positions double-offset by row (8c183b9e)
// =========================================================================

#[test]
fn test_table_not_oversized_from_double_offset() {
    let dom = Dom::create_body().with_child(
        Dom::create_node(NodeType::Table)
            .with_child(
                Dom::create_node(NodeType::Tr)
                    .with_child(Dom::create_node(NodeType::Td).with_child(Dom::create_text("R1"))),
            )
            .with_child(
                Dom::create_node(NodeType::Tr)
                    .with_child(Dom::create_node(NodeType::Td).with_child(Dom::create_text("R2"))),
            ),
    );

    let css = r#"
        * { margin: 0; padding: 0; }
        body { width: 800px; height: 600px; font-family: sans-serif; font-size: 14px; }
    "#;

    let lw = do_layout(dom, css, 800.0, 600.0);
    let r = lw.get_node_layout_rect(nid(1)).expect("table");
    assert!(
        r.size.height < 100.0,
        "table height={:.0} should be <100 for 2 rows. Double-offset?",
        r.size.height
    );
}

// =========================================================================
// Bug 7 — inline-block with only text got zero size (424e2dc1)
// InlineBlock only checked layout tree children, missing text in DOM.
// =========================================================================

#[test]
fn test_inline_block_text_only_has_size() {
    let dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_ids_and_classes(cls("w").into())
            .with_child(
                Dom::create_node(NodeType::Span)
                    .with_ids_and_classes(cls("ib").into())
                    .with_child(Dom::create_text("Hello World")),
            ),
    );

    let css = r#"
        * { margin: 0; padding: 0; }
        body { width: 800px; height: 600px; font-family: sans-serif; font-size: 16px; }
        .w { width: 800px; }
        .ib { display: inline-block; }
    "#;

    let lw = do_layout(dom, css, 800.0, 600.0);
    let r = lw.get_node_layout_rect(nid(2)).expect("ib");
    assert!(r.size.width > 10.0, "ib width={:.0} must be >10", r.size.width);
    assert!(r.size.height > 5.0, "ib height={:.0} must be >5", r.size.height);
}

// =========================================================================
// Bug 11 — * selector properties not in slow path (11ca0c64)
// =========================================================================

#[test]
fn test_global_star_font_weight_applied() {
    let dom = Dom::create_body().with_child(
        Dom::create_div().with_child(Dom::create_text("test")),
    );

    let css = "* { font-weight: bold; }";
    let s = do_cascade(dom, css);

    // nodes: 0=body, 1=div
    let cc = s.css_property_cache.ptr.compact_cache.as_ref().unwrap();
    let div_fw = (cc.tier1_enums[1] >> azul_css::compact_cache::FONT_WEIGHT_SHIFT)
        & azul_css::compact_cache::FONT_WEIGHT_MASK;
    assert!(div_fw != 0, "div should be bold from *, got encoded {}", div_fw);
}

// =========================================================================
// Bug 1 — whitespace text in table structure (91c1ceba)
// Reconciler didn't filter whitespace text nodes, causing false
// mixed-content detection and incorrect anonymous block wrapping.
// =========================================================================

#[test]
fn test_table_layout_with_whitespace_nodes() {
    let dom = Dom::create_body().with_child(
        Dom::create_node(NodeType::Table).with_child(
            Dom::create_node(NodeType::Tr)
                .with_child(Dom::create_node(NodeType::Td).with_child(Dom::create_text("A")))
                .with_child(Dom::create_node(NodeType::Td).with_child(Dom::create_text("B"))),
        ),
    );

    let css = r#"
        * { margin: 0; padding: 0; }
        body { width: 800px; height: 600px; font-family: sans-serif; font-size: 14px; }
    "#;

    let lw = do_layout(dom, css, 800.0, 600.0);
    let r = lw.get_node_layout_rect(nid(1)).expect("table");
    assert!(
        r.size.width > 5.0 && r.size.height > 5.0,
        "table size ({:.0}x{:.0}) should be non-zero",
        r.size.width,
        r.size.height
    );
}

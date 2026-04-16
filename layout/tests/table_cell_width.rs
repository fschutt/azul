//! Test: table cells should get non-zero width even without explicit CSS.
//! Reproduces the "login" vertical text bug from the browser demo.

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

fn create_layout_window() -> LayoutWindow {
    let font_cache = FcFontCache::build();
    LayoutWindow::new(font_cache).unwrap()
}

fn create_window_state(width: f32, height: f32) -> FullWindowState {
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(width, height);
    window_state
}

fn layout_dom(dom: Dom, css_str: &str, width: f32, height: f32) -> LayoutWindow {
    let (css, _) = azul_css::parser2::new_from_str(css_str);
    let mut dom = dom;
    let styled_dom = StyledDom::create(&mut dom, css);

    let mut layout_window = create_layout_window();
    let window_state = create_window_state(width, height);
    let renderer_resources = RendererResources::default();
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let mut debug_messages = Some(Vec::new());

    layout_window
        .layout_and_generate_display_list(
            &styled_dom,
            &window_state,
            &renderer_resources,
            &system_callbacks,
            &mut debug_messages,
        )
        .unwrap();

    layout_window
}

fn get_root_id() -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::ZERO)),
    }
}

fn node_id(index: usize) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(index))),
    }
}

/// HN-like: table with width:85% containing row with 3 cells.
/// The cells should expand to fill the table width.
#[test]
fn test_table_percentage_width_cells_expand() {
    let dom = Dom::create_node(NodeType::Table)
        .with_ids_and_classes(vec![IdOrClass::Id("main".into())].into())
        .with_child(
            Dom::create_node(NodeType::Tr)
                .with_child(
                    Dom::create_node(NodeType::Td)
                        .with_child(Dom::create_text("Left"))
                )
                .with_child(
                    Dom::create_node(NodeType::Td)
                        .with_child(Dom::create_text("Middle content here"))
                )
                .with_child(
                    Dom::create_node(NodeType::Td)
                        .with_child(Dom::create_text("Right"))
                )
        );

    let css = r#"
        #main { width: 85%; }
    "#;

    let layout_window = layout_dom(dom, css, 800.0, 600.0);

    eprintln!("\n=== Table with width:85% (800px viewport -> 680px table) ===");
    for i in 0..15 {
        let id = node_id(i);
        if let Some(rect) = layout_window.get_node_layout_rect(id) {
            eprintln!(
                "  node[{}]: x={:.1} y={:.1} w={:.1} h={:.1}",
                i, rect.origin.x, rect.origin.y, rect.size.width, rect.size.height
            );
        }
    }

    // Table should be ~680px (85% of 800)
    // All 3 cells combined should fill the table width
    let mut total_cell_width = 0.0f32;
    let mut cell_count = 0;
    for i in 0..15 {
        if let Some(rect) = layout_window.get_node_layout_rect(node_id(i)) {
            // Cells should be > 50px each (not shrink-wrapped to text)
            if rect.size.width > 50.0 && rect.size.width < 680.0 {
                total_cell_width += rect.size.width;
                cell_count += 1;
                eprintln!("  -> cell candidate node[{}]: w={:.1}", i, rect.size.width);
            }
        }
    }
    eprintln!("  Total cell width: {:.1}, cell_count: {}", total_cell_width, cell_count);

    // The cells should approximately fill the table
    assert!(total_cell_width > 600.0,
        "Cells should fill most of the 680px table, total cell width = {}", total_cell_width);
}

/// Table WITHOUT explicit width — auto width should still expand to fill containing block.
/// This is the case that matters for HN: <table> without width should be auto (shrink-to-fit).
#[test]
fn test_table_auto_width_fills_parent() {
    let dom = Dom::create_node(NodeType::Table)
        .with_child(
            Dom::create_node(NodeType::Tr)
                .with_child(
                    Dom::create_node(NodeType::Td)
                        .with_child(Dom::create_text("Left"))
                )
                .with_child(
                    Dom::create_node(NodeType::Td)
                        .with_child(Dom::create_text("Right"))
                )
        );

    // No CSS at all — pure UA defaults
    let layout_window = layout_dom(dom, "", 800.0, 600.0);

    eprintln!("\n=== Table with auto width (no CSS, 800px viewport) ===");
    for i in 0..10 {
        let id = node_id(i);
        if let Some(rect) = layout_window.get_node_layout_rect(id) {
            eprintln!(
                "  node[{}]: x={:.1} y={:.1} w={:.1} h={:.1}",
                i, rect.origin.x, rect.origin.y, rect.size.width, rect.size.height
            );
        }
    }
}

/// Minimal reproduction: <table><tr><td>A</td><td>B</td></tr></table>
/// Both cells should get non-zero width.
#[test]
fn test_table_two_cells_have_nonzero_width() {
    // Build DOM using NodeType directly (like the XML parser does)
    let dom = Dom::create_node(NodeType::Table)
        .with_child(
            Dom::create_node(NodeType::Tr)
                .with_child(
                    Dom::create_node(NodeType::Td)
                        .with_child(Dom::create_text("CellA"))
                )
                .with_child(
                    Dom::create_node(NodeType::Td)
                        .with_child(Dom::create_text("CellB"))
                )
        );

    // No author CSS — only UA defaults
    let layout_window = layout_dom(dom, "", 800.0, 600.0);

    // Print all node rects for debugging
    for i in 0..20 {
        let id = node_id(i);
        if let Some(rect) = layout_window.get_node_layout_rect(id) {
            eprintln!(
                "  node[{}]: x={:.1} y={:.1} w={:.1} h={:.1}",
                i, rect.origin.x, rect.origin.y, rect.size.width, rect.size.height
            );
        }
    }

    // node[0] = body (implicit root)
    // node[1] = table
    // node[2] = tr
    // node[3] = td (CellA)
    // node[4] = text "CellA"
    // node[5] = td (CellB)
    // node[6] = text "CellB"

    // Find the table — should have non-zero width
    let table_rect = layout_window.get_node_layout_rect(node_id(1))
        .expect("table rect");
    eprintln!("\nTable: w={:.1} h={:.1}", table_rect.size.width, table_rect.size.height);
    assert!(table_rect.size.width > 10.0,
        "Table should have non-zero width, got {}", table_rect.size.width);

    // Find td cells — both should have non-zero width
    let td_a = layout_window.get_node_layout_rect(node_id(3))
        .expect("td CellA rect");
    let td_b = layout_window.get_node_layout_rect(node_id(5))
        .expect("td CellB rect");
    eprintln!("TD(CellA): w={:.1} h={:.1}", td_a.size.width, td_a.size.height);
    eprintln!("TD(CellB): w={:.1} h={:.1}", td_b.size.width, td_b.size.height);

    assert!(td_a.size.width > 5.0,
        "TD CellA should have non-zero width, got {}", td_a.size.width);
    assert!(td_b.size.width > 5.0,
        "TD CellB should have non-zero width, got {}", td_b.size.width);
}

/// Simpler test: single table cell with text should have non-zero width.
#[test]
fn test_single_table_cell_nonzero_width() {
    let dom = Dom::create_node(NodeType::Table)
        .with_child(
            Dom::create_node(NodeType::Tr)
                .with_child(
                    Dom::create_node(NodeType::Td)
                        .with_child(Dom::create_text("Hello"))
                )
        );

    let layout_window = layout_dom(dom, "", 800.0, 600.0);

    for i in 0..10 {
        let id = node_id(i);
        if let Some(rect) = layout_window.get_node_layout_rect(id) {
            eprintln!(
                "  node[{}]: x={:.1} y={:.1} w={:.1} h={:.1}",
                i, rect.origin.x, rect.origin.y, rect.size.width, rect.size.height
            );
        }
    }

    let td_rect = layout_window.get_node_layout_rect(node_id(3))
        .expect("td rect");
    eprintln!("\nTD: w={:.1} h={:.1}", td_rect.size.width, td_rect.size.height);
    assert!(td_rect.size.width > 5.0,
        "Single TD should have non-zero width, got {}", td_rect.size.width);
}

/// Even simpler: just a div with display:table > div with display:table-row > div with display:table-cell
/// No text at all — pure block sizing.
#[test]
fn test_table_cell_with_explicit_css_gets_width() {
    let dom = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Class("table".into())].into())
        .with_child(
            Dom::create_div()
                .with_ids_and_classes(vec![IdOrClass::Class("row".into())].into())
                .with_child(
                    Dom::create_div()
                        .with_ids_and_classes(vec![IdOrClass::Class("cell-a".into())].into())
                        .with_child(Dom::create_div()
                            .with_ids_and_classes(vec![IdOrClass::Class("inner".into())].into())
                        )
                )
                .with_child(
                    Dom::create_div()
                        .with_ids_and_classes(vec![IdOrClass::Class("cell-b".into())].into())
                        .with_child(Dom::create_div()
                            .with_ids_and_classes(vec![IdOrClass::Class("inner".into())].into())
                        )
                )
        );

    let css = r#"
        .table { display: table; width: 400px; }
        .row { display: table-row; }
        .cell-a { display: table-cell; }
        .cell-b { display: table-cell; }
        .inner { width: 50px; height: 20px; }
    "#;

    let layout_window = layout_dom(dom, css, 800.0, 600.0);

    for i in 0..12 {
        let id = node_id(i);
        if let Some(rect) = layout_window.get_node_layout_rect(id) {
            eprintln!(
                "  node[{}]: x={:.1} y={:.1} w={:.1} h={:.1}",
                i, rect.origin.x, rect.origin.y, rect.size.width, rect.size.height
            );
        }
    }

    // node[2] appears to be cell-a (w=200), node[4] is cell-b (w=200)
    // The table layout distributes 400px / 2 = 200px per cell correctly.
    // But let's verify by checking all widths
    let mut cell_widths = Vec::new();
    for i in 0..12 {
        if let Some(rect) = layout_window.get_node_layout_rect(node_id(i)) {
            if (rect.size.width - 200.0).abs() < 1.0 {
                cell_widths.push((i, rect.size.width));
            }
        }
    }
    eprintln!("\nNodes with ~200px width (expected cells): {:?}", cell_widths);
    assert!(cell_widths.len() >= 2,
        "Should have at least 2 nodes with 200px width (the cells), got {:?}", cell_widths);

    // The table itself should be 400px
    let table_rect = layout_window.get_node_layout_rect(node_id(1)).expect("table rect");
    assert!((table_rect.size.width - 400.0).abs() < 1.0,
        "Table should be 400px, got {}", table_rect.size.width);
}

/// HN header reproduction: 3-cell row where cell 1 has long text, cell 2 has
/// medium text, cell 3 has short text ("login"). Cell 3 must NOT get 0 width.
#[test]
fn test_hn_header_three_cells_all_nonzero() {
    // Mimics: <table><tr>
    //   <td><span><a>Hacker News</a></span></td>
    //   <td><span>new | past | comments | ask | show</span></td>
    //   <td><span><a>login</a></span></td>
    // </tr></table>
    let dom = Dom::create_node(NodeType::Table)
        .with_child(
            Dom::create_node(NodeType::Tr)
                .with_child(
                    Dom::create_node(NodeType::Td)
                        .with_child(
                            Dom::create_node(NodeType::Span)
                                .with_child(
                                    Dom::create_node(NodeType::A)
                                        .with_child(Dom::create_text("Hacker News"))
                                )
                        )
                )
                .with_child(
                    Dom::create_node(NodeType::Td)
                        .with_child(
                            Dom::create_node(NodeType::Span)
                                .with_child(Dom::create_text("new | past | comments | ask | show"))
                        )
                )
                .with_child(
                    Dom::create_node(NodeType::Td)
                        .with_child(
                            Dom::create_node(NodeType::Span)
                                .with_child(
                                    Dom::create_node(NodeType::A)
                                        .with_child(Dom::create_text("login"))
                                )
                        )
                )
        );

    // No CSS — pure UA defaults
    let layout_window = layout_dom(dom, "", 800.0, 600.0);

    eprintln!("\n=== HN-like 3-cell header (no CSS) ===");
    for i in 0..25 {
        let id = node_id(i);
        if let Some(rect) = layout_window.get_node_layout_rect(id) {
            eprintln!(
                "  node[{}]: x={:.1} y={:.1} w={:.1} h={:.1}",
                i, rect.origin.x, rect.origin.y, rect.size.width, rect.size.height
            );
        }
    }

    // Find the "login" text width — it should be at least ~30px (5 chars)
    // With 3 cells, even the narrowest should be > 20px for "login"
    let mut min_cell_width = f32::MAX;
    let mut min_cell_idx = 0;
    for i in 0..25 {
        if let Some(rect) = layout_window.get_node_layout_rect(node_id(i)) {
            // Cells should be > 0 width and < total table width
            if rect.size.width > 0.0 && rect.size.width < 400.0 && rect.size.height > 10.0 {
                if rect.size.width < min_cell_width {
                    min_cell_width = rect.size.width;
                    min_cell_idx = i;
                }
            }
        }
    }
    eprintln!("\n  Narrowest cell-like node: node[{}] w={:.1}", min_cell_idx, min_cell_width);

    // Also test: simple cells with inline <a> children directly
    eprintln!("\n=== Simple: <td><a>login</a></td> vs <td>login</td> ===");
    let dom2 = Dom::create_node(NodeType::Table)
        .with_child(
            Dom::create_node(NodeType::Tr)
                .with_child(
                    Dom::create_node(NodeType::Td)
                        .with_child(Dom::create_text("direct text"))
                )
                .with_child(
                    Dom::create_node(NodeType::Td)
                        .with_child(
                            Dom::create_node(NodeType::A)
                                .with_child(Dom::create_text("in anchor"))
                        )
                )
                .with_child(
                    Dom::create_node(NodeType::Td)
                        .with_child(
                            Dom::create_node(NodeType::Span)
                                .with_child(Dom::create_text("in span"))
                        )
                )
        );
    let lw2 = layout_dom(dom2, "", 800.0, 600.0);
    for i in 0..20 {
        if let Some(rect) = lw2.get_node_layout_rect(node_id(i)) {
            eprintln!("  node[{}]: x={:.1} y={:.1} w={:.1} h={:.1}",
                i, rect.origin.x, rect.origin.y, rect.size.width, rect.size.height);
        }
    }

    // Now test nested inlines: <td><span><a>text</a></span></td>
    eprintln!("\n=== Nested: <td><span><a>text</a></span></td> ===");
    let dom3 = Dom::create_node(NodeType::Table)
        .with_child(
            Dom::create_node(NodeType::Tr)
                .with_child(
                    Dom::create_node(NodeType::Td)
                        .with_child(Dom::create_text("plain"))
                )
                .with_child(
                    Dom::create_node(NodeType::Td)
                        .with_child(
                            Dom::create_node(NodeType::Span)
                                .with_child(
                                    Dom::create_node(NodeType::A)
                                        .with_child(Dom::create_text("nested"))
                                )
                        )
                )
        );
    let lw3 = layout_dom(dom3, "", 800.0, 600.0);
    for i in 0..15 {
        if let Some(rect) = lw3.get_node_layout_rect(node_id(i)) {
            eprintln!("  node[{}]: x={:.1} y={:.1} w={:.1} h={:.1}",
                i, rect.origin.x, rect.origin.y, rect.size.width, rect.size.height);
        }
    }

    assert!(min_cell_width > 20.0,
        "Narrowest cell should be > 20px (for 'login' text), got {:.1}px at node[{}]",
        min_cell_width, min_cell_idx);
}

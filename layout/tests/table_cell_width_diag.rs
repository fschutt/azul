//! Diagnostic tests for the table cell width bug.
//!
//! Bug: `<td><span><a>text</a></span></td>` gets 0 min-content width
//! in a 3-cell table row, but works fine in a 2-cell table and for
//! `<td><a>text</a></td>` (single inline wrapper).
//!
//! These tests do NOT fix anything. They dump the layout tree structure,
//! cache state, and measure results to pinpoint the root cause.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, FormattingContext, IdOrClass, NodeId, NodeType},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    solver3::layout_tree::LayoutTree,
    window::LayoutWindow,
    window_state::FullWindowState,
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
            styled_dom,
            &window_state,
            &renderer_resources,
            &system_callbacks,
            &mut debug_messages,
        )
        .unwrap();

    layout_window
}

fn node_id(index: usize) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(index))),
    }
}

/// Helper: dump the full layout tree structure for a LayoutWindow.
/// Prints node index, DOM ID, formatting context, anonymous type,
/// parent, children, and used_size for every node.
fn dump_layout_tree(lw: &LayoutWindow, label: &str) {
    let tree = lw.layout_cache.tree.as_ref().expect("layout tree missing");
    let positions = &lw.layout_cache.calculated_positions;

    eprintln!("\n============================================================");
    eprintln!("=== LAYOUT TREE DUMP: {label} ===");
    eprintln!("Total nodes: {}", tree.nodes.len());
    eprintln!("============================================================");

    for i in 0..tree.nodes.len() {
        let hot = &tree.nodes[i];
        let cold = &tree.cold[i];
        let warm = &tree.warm[i];
        let children = tree.children(i);

        let dom_id_str = match hot.dom_node_id {
            Some(id) => format!("dom={}", id.index()),
            None => "ANON".to_string(),
        };

        let fc_str = format!("{:?}", hot.formatting_context);

        let anon_str = match cold.anonymous_type {
            Some(at) => format!(" anon={at:?}"),
            None => String::new(),
        };

        let parent_str = match hot.parent {
            Some(p) => format!("{p}"),
            None => "ROOT".to_string(),
        };

        let size_str = match hot.used_size {
            Some(s) => format!("w={:.1} h={:.1}", s.width, s.height),
            None => "NO SIZE".to_string(),
        };

        let pos_str = match positions.get(i) {
            Some(p) => format!("x={:.1} y={:.1}", p.x, p.y),
            None => "NO POS".to_string(),
        };

        let overflow_str = match &warm.overflow_content_size {
            Some(s) => format!(" overflow=({:.1},{:.1})", s.width, s.height),
            None => String::new(),
        };

        let display_str = format!("{:?}", warm.computed_style.display);

        eprintln!(
            "  [{i:>2}] {dom_id_str} fc={fc_str:<30} parent={parent_str:<5} children={children:?} {size_str} {pos_str} display={display_str}{anon_str}{overflow_str}",
        );
    }
    eprintln!();
}

/// Helper: print which DOM node IDs map to which layout indices.
fn dump_dom_to_layout(lw: &LayoutWindow, label: &str) {
    let tree = lw.layout_cache.tree.as_ref().expect("layout tree missing");
    eprintln!("--- DOM -> Layout mapping ({label}) ---");
    let mut entries: Vec<_> = tree.dom_to_layout.iter().collect();
    entries.sort_by_key(|(k, _)| k.index());
    for (dom_id, layout_indices) in entries {
        eprintln!("  dom[{}] -> layout{:?}", dom_id.index(), layout_indices);
    }
    eprintln!();
}

// ============================================================================
// TEST 1: Full layout tree dump for the failing 3-cell case
// ============================================================================
#[test]
fn diag_dump_3cell_layout_tree() {
    eprintln!("\n########## DIAGNOSTIC: 3-CELL NESTED INLINE TREE DUMP ##########");

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

    let lw = layout_dom(dom, "", 800.0, 600.0);
    dump_layout_tree(&lw, "3-cell: <td><span><a>text</a></span></td>");
    dump_dom_to_layout(&lw, "3-cell");

    // Print the final rects for all DOM nodes
    eprintln!("--- Final DOM node rects ---");
    for i in 0..20 {
        let id = node_id(i);
        if let Some(rect) = lw.get_node_layout_rect(id) {
            eprintln!(
                "  dom_node[{}]: x={:.1} y={:.1} w={:.1} h={:.1}",
                i, rect.origin.x, rect.origin.y, rect.size.width, rect.size.height
            );
        }
    }

    // Count how many layout tree nodes have 0 content width
    let tree = lw.layout_cache.tree.as_ref().unwrap();
    let mut zero_width_cells = Vec::new();
    for i in 0..tree.nodes.len() {
        if matches!(tree.nodes[i].formatting_context, FormattingContext::TableCell) {
            let w = tree.nodes[i].used_size.map(|s| s.width).unwrap_or(0.0);
            if w < 5.0 {
                zero_width_cells.push((i, w));
            }
        }
    }
    eprintln!("\nTable cells with near-zero width: {zero_width_cells:?}");
    eprintln!("EXPECTED: no table cells should have near-zero width");
}

// ============================================================================
// TEST 2: Compare 2-cell (works) vs 3-cell (fails)
// ============================================================================
#[test]
fn diag_compare_2cell_vs_3cell() {
    eprintln!("\n########## DIAGNOSTIC: 2-CELL vs 3-CELL COMPARISON ##########");

    // 2-cell case: both cells have <span><a>text</a></span>
    // This should work per the bug report
    let dom_2cell = Dom::create_node(NodeType::Table)
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
                                .with_child(
                                    Dom::create_node(NodeType::A)
                                        .with_child(Dom::create_text("login"))
                                )
                        )
                )
        );

    let lw2 = layout_dom(dom_2cell, "", 800.0, 600.0);
    dump_layout_tree(&lw2, "2-cell: both nested inline");

    eprintln!("--- 2-cell DOM node rects ---");
    for i in 0..15 {
        let id = node_id(i);
        if let Some(rect) = lw2.get_node_layout_rect(id) {
            eprintln!("  dom[{}]: w={:.1} h={:.1}", i, rect.size.width, rect.size.height);
        }
    }

    // 3-cell case: cells 1 and 3 have <span><a>text</a></span>,
    // cell 2 has <span>direct text</span>
    let dom_3cell = Dom::create_node(NodeType::Table)
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
                                .with_child(Dom::create_text("new | past | comments"))
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

    let lw3 = layout_dom(dom_3cell, "", 800.0, 600.0);
    dump_layout_tree(&lw3, "3-cell: mixed (nested/direct/nested)");

    eprintln!("--- 3-cell DOM node rects ---");
    for i in 0..20 {
        let id = node_id(i);
        if let Some(rect) = lw3.get_node_layout_rect(id) {
            eprintln!("  dom[{}]: w={:.1} h={:.1}", i, rect.size.width, rect.size.height);
        }
    }

    // Compare cell widths
    let tree2 = lw2.layout_cache.tree.as_ref().unwrap();
    let tree3 = lw3.layout_cache.tree.as_ref().unwrap();

    eprintln!("\n--- Cell width comparison ---");
    for i in 0..tree2.nodes.len() {
        if matches!(tree2.nodes[i].formatting_context, FormattingContext::TableCell) {
            let w = tree2.nodes[i].used_size.map(|s| s.width).unwrap_or(-1.0);
            eprintln!("  2-cell layout[{}]: w={:.1} (dom={:?})", i, w,
                tree2.nodes[i].dom_node_id.map(|n| n.index()));
        }
    }
    for i in 0..tree3.nodes.len() {
        if matches!(tree3.nodes[i].formatting_context, FormattingContext::TableCell) {
            let w = tree3.nodes[i].used_size.map(|s| s.width).unwrap_or(-1.0);
            eprintln!("  3-cell layout[{}]: w={:.1} (dom={:?})", i, w,
                tree3.nodes[i].dom_node_id.map(|n| n.index()));
        }
    }
}

// ============================================================================
// TEST 3: Does removing cell 2 (direct-text cell) fix the bug?
// This tests if the issue is specifically about mixing direct-text and
// nested-inline cells in the same row.
// ============================================================================
#[test]
fn diag_3cell_all_nested_inline() {
    eprintln!("\n########## DIAGNOSTIC: 3-CELL ALL NESTED INLINE ##########");

    // 3 cells, ALL with <span><a>text</a></span> (no direct text)
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
                                .with_child(
                                    Dom::create_node(NodeType::A)
                                        .with_child(Dom::create_text("new | past | comments"))
                                )
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

    let lw = layout_dom(dom, "", 800.0, 600.0);
    dump_layout_tree(&lw, "3-cell: ALL nested inline");

    let tree = lw.layout_cache.tree.as_ref().unwrap();
    eprintln!("--- Cell widths (all nested inline) ---");
    let mut cell_widths = Vec::new();
    for i in 0..tree.nodes.len() {
        if matches!(tree.nodes[i].formatting_context, FormattingContext::TableCell) {
            let w = tree.nodes[i].used_size.map(|s| s.width).unwrap_or(-1.0);
            cell_widths.push((i, w));
            eprintln!("  cell layout[{i}]: w={w:.1}");
        }
    }

    let zero_cells: Vec<_> = cell_widths.iter().filter(|(_, w)| *w < 5.0).collect();
    if zero_cells.is_empty() {
        eprintln!("RESULT: All 3 nested-inline cells have proper width!");
        eprintln!("  -> Bug is specific to MIXING direct-text and nested-inline cells");
    } else {
        eprintln!("RESULT: {} cells still have zero width: {:?}", zero_cells.len(), zero_cells);
        eprintln!("  -> Bug is NOT about mixing, it's about nested inlines in general with 3 cells");
    }
}

// ============================================================================
// TEST 4: Does cell ORDER matter? Put the direct-text cell first vs last.
// ============================================================================
#[test]
fn diag_cell_order_matters() {
    eprintln!("\n########## DIAGNOSTIC: CELL ORDER ##########");

    // Order A: direct-text FIRST, then two nested-inline
    let dom_a = Dom::create_node(NodeType::Table)
        .with_child(
            Dom::create_node(NodeType::Tr)
                .with_child(
                    Dom::create_node(NodeType::Td)
                        .with_child(
                            Dom::create_node(NodeType::Span)
                                .with_child(Dom::create_text("direct text first"))
                        )
                )
                .with_child(
                    Dom::create_node(NodeType::Td)
                        .with_child(
                            Dom::create_node(NodeType::Span)
                                .with_child(
                                    Dom::create_node(NodeType::A)
                                        .with_child(Dom::create_text("nested second"))
                                )
                        )
                )
                .with_child(
                    Dom::create_node(NodeType::Td)
                        .with_child(
                            Dom::create_node(NodeType::Span)
                                .with_child(
                                    Dom::create_node(NodeType::A)
                                        .with_child(Dom::create_text("nested third"))
                                )
                        )
                )
        );

    let lw_a = layout_dom(dom_a, "", 800.0, 600.0);
    let tree_a = lw_a.layout_cache.tree.as_ref().unwrap();
    eprintln!("--- Order A: [direct, nested, nested] ---");
    for i in 0..tree_a.nodes.len() {
        if matches!(tree_a.nodes[i].formatting_context, FormattingContext::TableCell) {
            let w = tree_a.nodes[i].used_size.map(|s| s.width).unwrap_or(-1.0);
            eprintln!("  cell layout[{}]: w={:.1} (dom={:?})", i, w,
                tree_a.nodes[i].dom_node_id.map(|n| n.index()));
        }
    }

    // Order B: direct-text LAST
    let dom_b = Dom::create_node(NodeType::Table)
        .with_child(
            Dom::create_node(NodeType::Tr)
                .with_child(
                    Dom::create_node(NodeType::Td)
                        .with_child(
                            Dom::create_node(NodeType::Span)
                                .with_child(
                                    Dom::create_node(NodeType::A)
                                        .with_child(Dom::create_text("nested first"))
                                )
                        )
                )
                .with_child(
                    Dom::create_node(NodeType::Td)
                        .with_child(
                            Dom::create_node(NodeType::Span)
                                .with_child(
                                    Dom::create_node(NodeType::A)
                                        .with_child(Dom::create_text("nested second"))
                                )
                        )
                )
                .with_child(
                    Dom::create_node(NodeType::Td)
                        .with_child(
                            Dom::create_node(NodeType::Span)
                                .with_child(Dom::create_text("direct text last"))
                        )
                )
        );

    let lw_b = layout_dom(dom_b, "", 800.0, 600.0);
    let tree_b = lw_b.layout_cache.tree.as_ref().unwrap();
    eprintln!("--- Order B: [nested, nested, direct] ---");
    for i in 0..tree_b.nodes.len() {
        if matches!(tree_b.nodes[i].formatting_context, FormattingContext::TableCell) {
            let w = tree_b.nodes[i].used_size.map(|s| s.width).unwrap_or(-1.0);
            eprintln!("  cell layout[{}]: w={:.1} (dom={:?})", i, w,
                tree_b.nodes[i].dom_node_id.map(|n| n.index()));
        }
    }

    // Order C: direct-text MIDDLE (the original failing case order)
    let dom_c = Dom::create_node(NodeType::Table)
        .with_child(
            Dom::create_node(NodeType::Tr)
                .with_child(
                    Dom::create_node(NodeType::Td)
                        .with_child(
                            Dom::create_node(NodeType::Span)
                                .with_child(
                                    Dom::create_node(NodeType::A)
                                        .with_child(Dom::create_text("nested first"))
                                )
                        )
                )
                .with_child(
                    Dom::create_node(NodeType::Td)
                        .with_child(
                            Dom::create_node(NodeType::Span)
                                .with_child(Dom::create_text("direct text middle"))
                        )
                )
                .with_child(
                    Dom::create_node(NodeType::Td)
                        .with_child(
                            Dom::create_node(NodeType::Span)
                                .with_child(
                                    Dom::create_node(NodeType::A)
                                        .with_child(Dom::create_text("nested last"))
                                )
                        )
                )
        );

    let lw_c = layout_dom(dom_c, "", 800.0, 600.0);
    let tree_c = lw_c.layout_cache.tree.as_ref().unwrap();
    eprintln!("--- Order C: [nested, direct, nested] (original bug) ---");
    for i in 0..tree_c.nodes.len() {
        if matches!(tree_c.nodes[i].formatting_context, FormattingContext::TableCell) {
            let w = tree_c.nodes[i].used_size.map(|s| s.width).unwrap_or(-1.0);
            eprintln!("  cell layout[{}]: w={:.1} (dom={:?})", i, w,
                tree_c.nodes[i].dom_node_id.map(|n| n.index()));
        }
    }
}

// ============================================================================
// TEST 5: Cache depth analysis - count how deep the tree goes below each
// cell and check if the cache clearing depth (2 levels) is sufficient.
// ============================================================================
#[test]
fn diag_cache_depth_analysis() {
    eprintln!("\n########## DIAGNOSTIC: CACHE DEPTH ANALYSIS ##########");
    eprintln!("The cache clearing in measure_cell_content_width() only clears");
    eprintln!("cell + children + grandchildren (2 levels below cell).");
    eprintln!("If the tree is deeper, stale cache entries survive.");

    // Case 1: <td>text</td> - depth 1 (text node is child of td)
    let dom1 = Dom::create_node(NodeType::Table)
        .with_child(
            Dom::create_node(NodeType::Tr)
                .with_child(
                    Dom::create_node(NodeType::Td)
                        .with_child(Dom::create_text("plain text"))
                )
                .with_child(
                    Dom::create_node(NodeType::Td)
                        .with_child(Dom::create_text("more plain"))
                )
                .with_child(
                    Dom::create_node(NodeType::Td)
                        .with_child(Dom::create_text("yet more"))
                )
        );
    let lw1 = layout_dom(dom1, "", 800.0, 600.0);
    let tree1 = lw1.layout_cache.tree.as_ref().unwrap();

    eprintln!("\n--- Case 1: <td>text</td> (3 cells) ---");
    print_subtree_depth(tree1, "3-cell plain text");

    // Case 2: <td><a>text</a></td> - depth 2 (a > text)
    let dom2 = Dom::create_node(NodeType::Table)
        .with_child(
            Dom::create_node(NodeType::Tr)
                .with_child(
                    Dom::create_node(NodeType::Td)
                        .with_child(
                            Dom::create_node(NodeType::A)
                                .with_child(Dom::create_text("link one"))
                        )
                )
                .with_child(
                    Dom::create_node(NodeType::Td)
                        .with_child(
                            Dom::create_node(NodeType::A)
                                .with_child(Dom::create_text("link two"))
                        )
                )
                .with_child(
                    Dom::create_node(NodeType::Td)
                        .with_child(
                            Dom::create_node(NodeType::A)
                                .with_child(Dom::create_text("link three"))
                        )
                )
        );
    let lw2 = layout_dom(dom2, "", 800.0, 600.0);
    let tree2 = lw2.layout_cache.tree.as_ref().unwrap();

    eprintln!("\n--- Case 2: <td><a>text</a></td> (3 cells) ---");
    print_subtree_depth(tree2, "3-cell single inline");

    // Case 3: <td><span><a>text</a></span></td> - depth 3 (span > a > text)
    // PLUS possibly anonymous IFC wrappers making it depth 4+
    let dom3 = Dom::create_node(NodeType::Table)
        .with_child(
            Dom::create_node(NodeType::Tr)
                .with_child(
                    Dom::create_node(NodeType::Td)
                        .with_child(
                            Dom::create_node(NodeType::Span)
                                .with_child(
                                    Dom::create_node(NodeType::A)
                                        .with_child(Dom::create_text("nested one"))
                                )
                        )
                )
                .with_child(
                    Dom::create_node(NodeType::Td)
                        .with_child(
                            Dom::create_node(NodeType::Span)
                                .with_child(
                                    Dom::create_node(NodeType::A)
                                        .with_child(Dom::create_text("nested two"))
                                )
                        )
                )
                .with_child(
                    Dom::create_node(NodeType::Td)
                        .with_child(
                            Dom::create_node(NodeType::Span)
                                .with_child(
                                    Dom::create_node(NodeType::A)
                                        .with_child(Dom::create_text("nested three"))
                                )
                        )
                )
        );
    let lw3 = layout_dom(dom3, "", 800.0, 600.0);
    let tree3 = lw3.layout_cache.tree.as_ref().unwrap();

    eprintln!("\n--- Case 3: <td><span><a>text</a></span></td> (3 cells) ---");
    print_subtree_depth(tree3, "3-cell nested inline");

    // Full tree dump for case 3 to see anonymous nodes
    dump_layout_tree(&lw3, "Case 3: nested inline 3-cell");
}

/// Recursively compute the depth of the subtree below each table cell.
fn compute_depth(tree: &LayoutTree, node: usize) -> usize {
    let children = tree.children(node);
    if children.is_empty() {
        return 0;
    }
    children.iter().map(|&c| 1 + compute_depth(tree, c)).max().unwrap_or(0)
}

/// Print subtree depth for all table cells in the tree.
fn print_subtree_depth(tree: &LayoutTree, label: &str) {
    eprintln!("  Subtree depths below table cells ({label}):");
    for i in 0..tree.nodes.len() {
        if matches!(tree.nodes[i].formatting_context, FormattingContext::TableCell) {
            let depth = compute_depth(tree, i);
            let w = tree.nodes[i].used_size.map(|s| s.width).unwrap_or(-1.0);
            let dom_id = tree.nodes[i].dom_node_id.map(|n| n.index());
            eprintln!(
                "    cell layout[{}] (dom={:?}): depth={}, w={:.1} {}",
                i, dom_id, depth, w,
                if w < 5.0 { " <<< ZERO WIDTH" } else { "" }
            );

            // Print the full child tree below this cell
            print_children_recursive(tree, i, 3);
        }
    }
}

/// Print children recursively with indentation.
fn print_children_recursive(tree: &LayoutTree, node: usize, indent: usize) {
    let children = tree.children(node);
    for &child in children {
        let hot = &tree.nodes[child];
        let cold = &tree.cold[child];
        let prefix = " ".repeat(indent);
        let dom_str = match hot.dom_node_id {
            Some(id) => format!("dom={}", id.index()),
            None => "ANON".to_string(),
        };
        let anon_str = match cold.anonymous_type {
            Some(at) => format!(" ({at:?})"),
            None => String::new(),
        };
        let w = hot.used_size.map(|s| s.width).unwrap_or(-1.0);
        eprintln!(
            "{}[{}] {} fc={:?}{} w={:.1}",
            prefix, child, dom_str, hot.formatting_context, anon_str, w
        );
        print_children_recursive(tree, child, indent + 2);
    }
}

// ============================================================================
// TEST 6: Does adding more text to nested-inline cells change anything?
// If it's a cache issue, longer text might still get 0 width.
// ============================================================================
#[test]
fn diag_longer_text_nested_inline() {
    eprintln!("\n########## DIAGNOSTIC: LONGER TEXT IN NESTED INLINE ##########");

    let dom = Dom::create_node(NodeType::Table)
        .with_child(
            Dom::create_node(NodeType::Tr)
                .with_child(
                    Dom::create_node(NodeType::Td)
                        .with_child(
                            Dom::create_node(NodeType::Span)
                                .with_child(
                                    Dom::create_node(NodeType::A)
                                        .with_child(Dom::create_text(
                                            "This is a very long text string that should definitely have significant width"
                                        ))
                                )
                        )
                )
                .with_child(
                    Dom::create_node(NodeType::Td)
                        .with_child(
                            Dom::create_node(NodeType::Span)
                                .with_child(Dom::create_text("short"))
                        )
                )
                .with_child(
                    Dom::create_node(NodeType::Td)
                        .with_child(
                            Dom::create_node(NodeType::Span)
                                .with_child(
                                    Dom::create_node(NodeType::A)
                                        .with_child(Dom::create_text(
                                            "Another very long text that should be wide"
                                        ))
                                )
                        )
                )
        );

    let lw = layout_dom(dom, "", 800.0, 600.0);
    let tree = lw.layout_cache.tree.as_ref().unwrap();

    eprintln!("--- Cell widths with long nested text ---");
    for i in 0..tree.nodes.len() {
        if matches!(tree.nodes[i].formatting_context, FormattingContext::TableCell) {
            let w = tree.nodes[i].used_size.map(|s| s.width).unwrap_or(-1.0);
            eprintln!("  cell layout[{}]: w={:.1} (dom={:?})", i, w,
                tree.nodes[i].dom_node_id.map(|n| n.index()));
        }
    }

    eprintln!("\nIf long-text cells STILL get 0 width, it confirms cache/measurement bug.");
    eprintln!("If they get proper width, the bug is text-length dependent (unlikely).");
}

// ============================================================================
// TEST 7: Nesting depth isolation - does depth 2 work but depth 3 fail?
// ============================================================================
#[test]
fn diag_nesting_depth_isolation() {
    eprintln!("\n########## DIAGNOSTIC: NESTING DEPTH ISOLATION ##########");

    // Depth 1: <td>text</td>
    let dom_d1 = Dom::create_node(NodeType::Table)
        .with_child(
            Dom::create_node(NodeType::Tr)
                .with_child(Dom::create_node(NodeType::Td).with_child(Dom::create_text("AAA")))
                .with_child(Dom::create_node(NodeType::Td).with_child(Dom::create_text("BBB")))
                .with_child(Dom::create_node(NodeType::Td).with_child(Dom::create_text("CCC")))
        );
    let lw1 = layout_dom(dom_d1, "", 800.0, 600.0);
    let t1 = lw1.layout_cache.tree.as_ref().unwrap();
    eprintln!("--- Depth 1: <td>text</td> ---");
    for i in 0..t1.nodes.len() {
        if matches!(t1.nodes[i].formatting_context, FormattingContext::TableCell) {
            let w = t1.nodes[i].used_size.map(|s| s.width).unwrap_or(-1.0);
            eprintln!("  cell[{i}]: w={w:.1}");
        }
    }

    // Depth 2: <td><a>text</a></td>
    let dom_d2 = Dom::create_node(NodeType::Table)
        .with_child(
            Dom::create_node(NodeType::Tr)
                .with_child(Dom::create_node(NodeType::Td).with_child(
                    Dom::create_node(NodeType::A).with_child(Dom::create_text("AAA"))))
                .with_child(Dom::create_node(NodeType::Td).with_child(
                    Dom::create_node(NodeType::A).with_child(Dom::create_text("BBB"))))
                .with_child(Dom::create_node(NodeType::Td).with_child(
                    Dom::create_node(NodeType::A).with_child(Dom::create_text("CCC"))))
        );
    let lw2 = layout_dom(dom_d2, "", 800.0, 600.0);
    let t2 = lw2.layout_cache.tree.as_ref().unwrap();
    eprintln!("--- Depth 2: <td><a>text</a></td> ---");
    for i in 0..t2.nodes.len() {
        if matches!(t2.nodes[i].formatting_context, FormattingContext::TableCell) {
            let w = t2.nodes[i].used_size.map(|s| s.width).unwrap_or(-1.0);
            eprintln!("  cell[{i}]: w={w:.1}");
        }
    }

    // Depth 3: <td><span><a>text</a></span></td>
    let dom_d3 = Dom::create_node(NodeType::Table)
        .with_child(
            Dom::create_node(NodeType::Tr)
                .with_child(Dom::create_node(NodeType::Td).with_child(
                    Dom::create_node(NodeType::Span).with_child(
                        Dom::create_node(NodeType::A).with_child(Dom::create_text("AAA")))))
                .with_child(Dom::create_node(NodeType::Td).with_child(
                    Dom::create_node(NodeType::Span).with_child(
                        Dom::create_node(NodeType::A).with_child(Dom::create_text("BBB")))))
                .with_child(Dom::create_node(NodeType::Td).with_child(
                    Dom::create_node(NodeType::Span).with_child(
                        Dom::create_node(NodeType::A).with_child(Dom::create_text("CCC")))))
        );
    let lw3 = layout_dom(dom_d3, "", 800.0, 600.0);
    let t3 = lw3.layout_cache.tree.as_ref().unwrap();
    eprintln!("--- Depth 3: <td><span><a>text</a></span></td> ---");
    for i in 0..t3.nodes.len() {
        if matches!(t3.nodes[i].formatting_context, FormattingContext::TableCell) {
            let w = t3.nodes[i].used_size.map(|s| s.width).unwrap_or(-1.0);
            eprintln!("  cell[{i}]: w={w:.1}");
        }
    }

    // Depth 4: <td><div><span><a>text</a></span></div></td>
    let dom_d4 = Dom::create_node(NodeType::Table)
        .with_child(
            Dom::create_node(NodeType::Tr)
                .with_child(Dom::create_node(NodeType::Td).with_child(
                    Dom::create_div().with_child(
                        Dom::create_node(NodeType::Span).with_child(
                            Dom::create_node(NodeType::A).with_child(Dom::create_text("AAA"))))))
                .with_child(Dom::create_node(NodeType::Td).with_child(
                    Dom::create_div().with_child(
                        Dom::create_node(NodeType::Span).with_child(
                            Dom::create_node(NodeType::A).with_child(Dom::create_text("BBB"))))))
                .with_child(Dom::create_node(NodeType::Td).with_child(
                    Dom::create_div().with_child(
                        Dom::create_node(NodeType::Span).with_child(
                            Dom::create_node(NodeType::A).with_child(Dom::create_text("CCC"))))))
        );
    let lw4 = layout_dom(dom_d4, "", 800.0, 600.0);
    let t4 = lw4.layout_cache.tree.as_ref().unwrap();
    eprintln!("--- Depth 4: <td><div><span><a>text</a></span></div></td> ---");
    for i in 0..t4.nodes.len() {
        if matches!(t4.nodes[i].formatting_context, FormattingContext::TableCell) {
            let w = t4.nodes[i].used_size.map(|s| s.width).unwrap_or(-1.0);
            eprintln!("  cell[{i}]: w={w:.1}");
        }
    }
}

// ============================================================================
// TEST 8: Does the number of cells matter? Compare 2 vs 3 vs 4 cells
// with the SAME nesting depth.
// ============================================================================
#[test]
fn diag_cell_count_matters() {
    eprintln!("\n########## DIAGNOSTIC: CELL COUNT (all nested inline) ##########");

    // 1 cell
    let dom1 = Dom::create_node(NodeType::Table)
        .with_child(
            Dom::create_node(NodeType::Tr)
                .with_child(Dom::create_node(NodeType::Td).with_child(
                    Dom::create_node(NodeType::Span).with_child(
                        Dom::create_node(NodeType::A).with_child(Dom::create_text("cell1")))))
        );
    let lw1 = layout_dom(dom1, "", 800.0, 600.0);
    let t1 = lw1.layout_cache.tree.as_ref().unwrap();
    eprintln!("--- 1 cell ---");
    for i in 0..t1.nodes.len() {
        if matches!(t1.nodes[i].formatting_context, FormattingContext::TableCell) {
            eprintln!("  cell[{}]: w={:.1}", i, t1.nodes[i].used_size.map(|s| s.width).unwrap_or(-1.0));
        }
    }

    // 2 cells
    let dom2 = Dom::create_node(NodeType::Table)
        .with_child(
            Dom::create_node(NodeType::Tr)
                .with_child(Dom::create_node(NodeType::Td).with_child(
                    Dom::create_node(NodeType::Span).with_child(
                        Dom::create_node(NodeType::A).with_child(Dom::create_text("cell1")))))
                .with_child(Dom::create_node(NodeType::Td).with_child(
                    Dom::create_node(NodeType::Span).with_child(
                        Dom::create_node(NodeType::A).with_child(Dom::create_text("cell2")))))
        );
    let lw2 = layout_dom(dom2, "", 800.0, 600.0);
    let t2 = lw2.layout_cache.tree.as_ref().unwrap();
    eprintln!("--- 2 cells ---");
    for i in 0..t2.nodes.len() {
        if matches!(t2.nodes[i].formatting_context, FormattingContext::TableCell) {
            eprintln!("  cell[{}]: w={:.1}", i, t2.nodes[i].used_size.map(|s| s.width).unwrap_or(-1.0));
        }
    }

    // 3 cells
    let dom3 = Dom::create_node(NodeType::Table)
        .with_child(
            Dom::create_node(NodeType::Tr)
                .with_child(Dom::create_node(NodeType::Td).with_child(
                    Dom::create_node(NodeType::Span).with_child(
                        Dom::create_node(NodeType::A).with_child(Dom::create_text("cell1")))))
                .with_child(Dom::create_node(NodeType::Td).with_child(
                    Dom::create_node(NodeType::Span).with_child(
                        Dom::create_node(NodeType::A).with_child(Dom::create_text("cell2")))))
                .with_child(Dom::create_node(NodeType::Td).with_child(
                    Dom::create_node(NodeType::Span).with_child(
                        Dom::create_node(NodeType::A).with_child(Dom::create_text("cell3")))))
        );
    let lw3 = layout_dom(dom3, "", 800.0, 600.0);
    let t3 = lw3.layout_cache.tree.as_ref().unwrap();
    eprintln!("--- 3 cells ---");
    for i in 0..t3.nodes.len() {
        if matches!(t3.nodes[i].formatting_context, FormattingContext::TableCell) {
            eprintln!("  cell[{}]: w={:.1}", i, t3.nodes[i].used_size.map(|s| s.width).unwrap_or(-1.0));
        }
    }

    // 4 cells
    let dom4 = Dom::create_node(NodeType::Table)
        .with_child(
            Dom::create_node(NodeType::Tr)
                .with_child(Dom::create_node(NodeType::Td).with_child(
                    Dom::create_node(NodeType::Span).with_child(
                        Dom::create_node(NodeType::A).with_child(Dom::create_text("cell1")))))
                .with_child(Dom::create_node(NodeType::Td).with_child(
                    Dom::create_node(NodeType::Span).with_child(
                        Dom::create_node(NodeType::A).with_child(Dom::create_text("cell2")))))
                .with_child(Dom::create_node(NodeType::Td).with_child(
                    Dom::create_node(NodeType::Span).with_child(
                        Dom::create_node(NodeType::A).with_child(Dom::create_text("cell3")))))
                .with_child(Dom::create_node(NodeType::Td).with_child(
                    Dom::create_node(NodeType::Span).with_child(
                        Dom::create_node(NodeType::A).with_child(Dom::create_text("cell4")))))
        );
    let lw4 = layout_dom(dom4, "", 800.0, 600.0);
    let t4 = lw4.layout_cache.tree.as_ref().unwrap();
    eprintln!("--- 4 cells ---");
    for i in 0..t4.nodes.len() {
        if matches!(t4.nodes[i].formatting_context, FormattingContext::TableCell) {
            eprintln!("  cell[{}]: w={:.1}", i, t4.nodes[i].used_size.map(|s| s.width).unwrap_or(-1.0));
        }
    }
}

// ============================================================================
// TEST 9: ROOT CAUSE CONFIRMATION
//
// The bug: In layout_bfc(), max_cross_size (which becomes overflow_size.width)
// is computed from child used_size.width. For inline children like <span>,
// used_size.width = 0 (CSS 2.2 ss 10.3.1: width does not apply to inline
// non-replaced elements). But the actual content extent lives in the child's
// overflow_content_size.
//
// This means: cell.overflow_size.width = 0 for any cell whose BFC has
// inline-only children (e.g. <td><span>...</span></td>). When
// measure_cell_content_width reads overflow_content_size.width for the cell,
// it gets 0, so the cell gets 0 min/max-content width.
//
// Proof: Compare overflow_content_size of cells vs their inline children.
// The child <span> has overflow=(87.0, 18.4) but the parent <td>'s BFC
// only sees child used_size.width = 0, so cell overflow=(0.0, 18.4).
// ============================================================================
#[test]
fn diag_root_cause_overflow_propagation() {
    eprintln!("\n########## DIAGNOSTIC: ROOT CAUSE - OVERFLOW PROPAGATION ##########");
    eprintln!("Theory: BFC's max_cross_size uses child.used_size.width for inline");
    eprintln!("children, but inline elements have used_size.width=0 (CSS spec).");
    eprintln!("The actual content width is in the child's overflow_content_size,");
    eprintln!("which is NOT propagated up to the BFC's overflow_size.");

    // Case A: Direct text in cell (works)
    // <td>text</td> -> BFC child is the IFC text run, overflow propagates correctly
    let dom_a = Dom::create_node(NodeType::Table)
        .with_child(
            Dom::create_node(NodeType::Tr)
                .with_child(
                    Dom::create_node(NodeType::Td)
                        .with_child(Dom::create_text("direct text"))
                )
                .with_child(
                    Dom::create_node(NodeType::Td)
                        .with_child(Dom::create_text("more text"))
                )
                .with_child(
                    Dom::create_node(NodeType::Td)
                        .with_child(Dom::create_text("third"))
                )
        );
    let lw_a = layout_dom(dom_a, "", 800.0, 600.0);
    let ta = lw_a.layout_cache.tree.as_ref().unwrap();
    eprintln!("\n--- Case A: <td>text</td> (WORKS) ---");
    for i in 0..ta.nodes.len() {
        let warm = &ta.warm[i];
        let hot = &ta.nodes[i];
        let used_w = hot.used_size.map(|s| s.width).unwrap_or(-1.0);
        let overflow_w = warm.overflow_content_size.map(|s| s.width).unwrap_or(-1.0);
        if matches!(hot.formatting_context, FormattingContext::TableCell) {
            eprintln!("  CELL[{}]: used_w={:.1}, overflow_w={:.1}  fc={:?}",
                i, used_w, overflow_w, hot.formatting_context);
            // Print its children
            for &c in ta.children(i) {
                let cw = &ta.warm[c];
                let ch = &ta.nodes[c];
                let cu = ch.used_size.map(|s| s.width).unwrap_or(-1.0);
                let co = cw.overflow_content_size.map(|s| s.width).unwrap_or(-1.0);
                eprintln!("    child[{}]: used_w={:.1}, overflow_w={:.1}  fc={:?}  dom={:?}",
                    c, cu, co, ch.formatting_context, ch.dom_node_id.map(|n| n.index()));
            }
        }
    }

    // Case B: Nested inline in cell (FAILS)
    // <td><span><a>text</a></span></td>
    let dom_b = Dom::create_node(NodeType::Table)
        .with_child(
            Dom::create_node(NodeType::Tr)
                .with_child(
                    Dom::create_node(NodeType::Td)
                        .with_child(
                            Dom::create_node(NodeType::Span)
                                .with_child(
                                    Dom::create_node(NodeType::A)
                                        .with_child(Dom::create_text("nested text"))
                                )
                        )
                )
                .with_child(
                    Dom::create_node(NodeType::Td)
                        .with_child(Dom::create_text("direct text"))
                )
                .with_child(
                    Dom::create_node(NodeType::Td)
                        .with_child(
                            Dom::create_node(NodeType::Span)
                                .with_child(
                                    Dom::create_node(NodeType::A)
                                        .with_child(Dom::create_text("also nested"))
                                )
                        )
                )
        );
    let lw_b = layout_dom(dom_b, "", 800.0, 600.0);
    let tb = lw_b.layout_cache.tree.as_ref().unwrap();
    eprintln!("\n--- Case B: <td><span><a>text</a></span></td> (FAILS) ---");
    for i in 0..tb.nodes.len() {
        let warm = &tb.warm[i];
        let hot = &tb.nodes[i];
        let used_w = hot.used_size.map(|s| s.width).unwrap_or(-1.0);
        let overflow_w = warm.overflow_content_size.map(|s| s.width).unwrap_or(-1.0);
        if matches!(hot.formatting_context, FormattingContext::TableCell) {
            eprintln!("  CELL[{}]: used_w={:.1}, overflow_w={:.1}  fc={:?}",
                i, used_w, overflow_w, hot.formatting_context);
            // Print its children and grandchildren
            for &c in tb.children(i) {
                let cw = &tb.warm[c];
                let ch = &tb.nodes[c];
                let cu = ch.used_size.map(|s| s.width).unwrap_or(-1.0);
                let co = cw.overflow_content_size.map(|s| s.width).unwrap_or(-1.0);
                eprintln!("    child[{}]: used_w={:.1}, overflow_w={:.1}  fc={:?}  dom={:?}",
                    c, cu, co, ch.formatting_context, ch.dom_node_id.map(|n| n.index()));
                for &gc in tb.children(c) {
                    let gw = &tb.warm[gc];
                    let gh = &tb.nodes[gc];
                    let gu = gh.used_size.map(|s| s.width).unwrap_or(-1.0);
                    let go = gw.overflow_content_size.map(|s| s.width).unwrap_or(-1.0);
                    eprintln!("      grandchild[{}]: used_w={:.1}, overflow_w={:.1}  fc={:?}  dom={:?}",
                        gc, gu, go, gh.formatting_context, gh.dom_node_id.map(|n| n.index()));
                }
            }
        }
    }

    eprintln!("\n--- ROOT CAUSE SUMMARY ---");
    eprintln!("In Case A, the cell's BFC child is the IFC text content node.");
    eprintln!("Its used_size.width reflects the text width, so max_cross_size is correct.");
    eprintln!();
    eprintln!("In Case B, the cell's BFC child is <span> (FormattingContext::Inline).");
    eprintln!("Its used_size.width = 0 (CSS 2.2 ss10.3.1: width N/A to inline non-replaced).");
    eprintln!("The BFC computes max_cross_size from used_size.width = 0.");
    eprintln!("The cell's overflow_content_size.width = 0, so measure_cell_content_width = 0.");
    eprintln!();
    eprintln!("FIX: In layout_bfc(), when computing max_cross_size for inline children,");
    eprintln!("use max(child.used_size.width, child.overflow_content_size.width) instead");
    eprintln!("of just child.used_size.width.");
    eprintln!();
    eprintln!("Alternatively: in measure_cell_content_width(), walk the subtree to find");
    eprintln!("the deepest overflow_content_size that reflects actual content width.");
}

// ============================================================================
// TEST 10: Verify the BFC overflow propagation directly.
// Compare what the BFC sees for direct text vs inline-wrapped text.
// ============================================================================
#[test]
fn diag_bfc_overflow_direct_vs_inline() {
    eprintln!("\n########## DIAGNOSTIC: BFC OVERFLOW DIRECT vs INLINE ##########");

    // Non-table case: just a plain div with inline children
    // This shows the same issue outside tables: BFC overflow doesn't
    // propagate through inline elements.
    let dom_direct = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Class("box".into())].into())
        .with_child(Dom::create_text("Hello World"));

    let css = ".box { width: auto; }";
    let lw1 = layout_dom(dom_direct, css, 800.0, 600.0);
    let t1 = lw1.layout_cache.tree.as_ref().unwrap();
    eprintln!("--- Direct text in div ---");
    for i in 0..t1.nodes.len() {
        let w = &t1.warm[i];
        let h = &t1.nodes[i];
        let uw = h.used_size.map(|s| s.width).unwrap_or(-1.0);
        let ow = w.overflow_content_size.map(|s| s.width).unwrap_or(-1.0);
        eprintln!("  [{}] fc={:?} used_w={:.1} overflow_w={:.1} dom={:?}",
            i, h.formatting_context, uw, ow, h.dom_node_id.map(|n| n.index()));
    }

    // Inline-wrapped text
    let dom_inline = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Class("box".into())].into())
        .with_child(
            Dom::create_node(NodeType::Span)
                .with_child(
                    Dom::create_node(NodeType::A)
                        .with_child(Dom::create_text("Hello World"))
                )
        );

    let lw2 = layout_dom(dom_inline, css, 800.0, 600.0);
    let t2 = lw2.layout_cache.tree.as_ref().unwrap();
    eprintln!("--- Inline-wrapped text in div ---");
    for i in 0..t2.nodes.len() {
        let w = &t2.warm[i];
        let h = &t2.nodes[i];
        let uw = h.used_size.map(|s| s.width).unwrap_or(-1.0);
        let ow = w.overflow_content_size.map(|s| s.width).unwrap_or(-1.0);
        eprintln!("  [{}] fc={:?} used_w={:.1} overflow_w={:.1} dom={:?}",
            i, h.formatting_context, uw, ow, h.dom_node_id.map(|n| n.index()));
    }
}

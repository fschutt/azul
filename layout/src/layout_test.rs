// In layout/src/layout_test.rs
// Add this at the end of the file, or within the existing test module.

#[test]
fn test_anonymous_table_cell_generation() {
    use azul_core::{
        callbacks::CallbackInfo,
        dom::{Dom, NodeData, NodeType, NodeDataInlineCssPropertyVec, NodeDataInlineCssProperty, NodeTypePath}, // Added NodeTypePath
        styled_dom::{StyledDom, DomId, NodeId, NodeHierarchyItemId},
        // ui_solver::{do_the_layout_internal, FullWindowState}, // Corrected path will be crate::solver2::do_the_layout_internal
        app_resources::{IdNamespace, RendererResources, DocumentId, Epoch, OptionGlContextPtr, AppResources},
        gl::GlApi,
        // pixman::Pixmap, // Not directly used, AppResources handles its resources
        // ui_state::UiState, // Not directly used
        window::FakeWindow, // For AppResources
        text_layout::TextLayoutOptions, // For AppResources
    };
    use azul_css::{
        Css, CssProperty,
        CssPropertyValue, LayoutHeight, LayoutWidth, StyleBackgroundColor, ColorU
    };
    // Assuming rect types are from azul_core or a commonly used geometry crate.
    // If they are from `azul_layout::rect` (i.e., `crate::rect`), adjust use statement.
    // For now, let's assume they might be top-level in azul_core or need specific import.
    // The prompt mentioned `crate::rect::...` for layout_test.rs, so let's use that.
    use crate::rect::{LogicalRect, LogicalPosition, LogicalSize};


    // Create a DOM structure that requires anonymous table cells
    let mut css = Css::new();
    css.add_rule(".table_class { width: 400px; height: 300px; background-color: red; }".parse().unwrap());

    let table_dom = Dom::new(NodeType::Table)
        .with_classes(vec!["table_class".into()].into())
        .with_child(
            Dom::new(NodeType::Div)
                .with_child(
                    Dom::new(NodeType::Span)
                        .with_child(Dom::text("Anonymous Text"))
                )
        );

    let mut renderer_resources = RendererResources::default();
    // Pass the Css rules when creating StyledDom
    let styled_dom = StyledDom::new_with_style(table_dom, &css, &mut renderer_resources);


    let root_bounds = LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(800.0, 600.0));
    let document_id = DocumentId { namespace_id: IdNamespace(0), id: 0 };

    // let mut debug_messages = Some(Vec::new()); // Uncomment for debugging

    // Corrected path for do_the_layout_internal
    let layout_result = crate::solver2::do_the_layout_internal(
        styled_dom.root,
        None,
        styled_dom,
        &mut renderer_resources,
        &document_id,
        root_bounds,
        &mut None, // &mut debug_messages,
    );

    let arena_node_hierarchy = layout_result.styled_dom.node_hierarchy.as_container();
    let arena_node_data = layout_result.styled_dom.node_data.as_container();

    assert_eq!(arena_node_data[NodeId::new(0)].node_type.get_path(), NodeTypePath::Table);
    assert!(!arena_node_data[NodeId::new(0)].is_anonymous);

    let anon_tr_id = arena_node_hierarchy[NodeId::new(0)].first_child_id(NodeId::new(0)).expect("Table should have a first child (anon TR)");
    assert_eq!(anon_tr_id, NodeId::new(1), "Anonymous TR should be NodeId(1)");
    assert_eq!(arena_node_data[anon_tr_id].node_type.get_path(), NodeTypePath::Tr);
    assert!(arena_node_data[anon_tr_id].is_anonymous);

    let anon_td_id = arena_node_hierarchy[anon_tr_id].first_child_id(anon_tr_id).expect("Anon TR should have a first child (anon TD)");
    assert_eq!(anon_td_id, NodeId::new(2), "Anonymous TD should be NodeId(2)");
    assert_eq!(arena_node_data[anon_td_id].node_type.get_path(), NodeTypePath::Td);
    assert!(arena_node_data[anon_td_id].is_anonymous);

    let original_div_id = arena_node_hierarchy[anon_td_id].first_child_id(anon_td_id).expect("Anon TD should have a first child (original Div)");
    assert_eq!(original_div_id, NodeId::new(3), "Original Div should be NodeId(3)");
    assert_eq!(arena_node_data[original_div_id].node_type.get_path(), NodeTypePath::Div);
    assert!(!arena_node_data[original_div_id].is_anonymous);

    let original_span_id = arena_node_hierarchy[original_div_id].first_child_id(original_div_id).expect("Original Div should have a first child (original Span)");
    assert_eq!(original_span_id, NodeId::new(4), "Original Span should be NodeId(4)");
    assert_eq!(arena_node_data[original_span_id].node_type.get_path(), NodeTypePath::Span);
    assert!(!arena_node_data[original_span_id].is_anonymous);

    let text_node_id = arena_node_hierarchy[original_span_id].first_child_id(original_span_id).expect("Original Span should have a first child (Text Node)");
    assert_eq!(text_node_id, NodeId::new(5), "Text Node should be NodeId(5)");
    assert_eq!(arena_node_data[text_node_id].node_type.get_path(), NodeTypePath::Text); // Text nodes are not anonymous by default
    assert!(!arena_node_data[text_node_id].is_anonymous);

    assert_eq!(layout_result.styled_dom.node_data.len(), 6, "Total node count in arena should be 6");
    assert_eq!(layout_result.styled_dom.node_hierarchy.len(), 6, "Total node hierarchy count in arena should be 6");

    let results_vec = vec![layout_result];
    let mut fake_window_state = azul_core::ui_solver::FullWindowState::default(); // Explicit path
    fake_window_state.size.dimensions = LogicalSize::new(800.0, 600.0);

    let mut fake_app_resources = AppResources::new(FakeWindow::new());

    // Populate CallbackInfo::new carefully. Many fields might not be directly relevant
    // for this specific test of get_parent but need to be valid.
    let mut timers = azul_core::FastHashMap::default();
    let mut threads = azul_core::FastHashMap::default();
    let mut timers_removed = azul_core::FastBTreeSet::default();
    let mut threads_removed = azul_core::FastBTreeSet::default();
    let mut new_windows = Vec::new();
    let system_callbacks = azul_core::task::ExternalSystemCallbacks::default();
    let mut stop_propagation = false;
    let mut focus_target = None;
    let mut words_changed = BTreeMap::default();
    let mut images_changed = BTreeMap::default();
    let mut image_masks_changed = BTreeMap::default();
    let mut css_props_changed = BTreeMap::default();
    let scroll_states = BTreeMap::default();
    let mut nodes_scrolled = BTreeMap::default();


    let callback_info = CallbackInfo::new(
        &results_vec,
        &renderer_resources, // Already mutable from layout_result
        &None, // previous_window_state
        &fake_window_state,
        &mut Default::default(), // modifiable_window_state (WindowState)
        &OptionGlContextPtr(GlApi::Unknown),
        &mut fake_app_resources.image_cache, // image_cache
        &mut fake_app_resources.font_cache, // system_fonts (FcFontCache)
        &mut timers,
        &mut threads,
        &mut timers_removed,
        &mut threads_removed,
        &fake_window_state.window_handle, // current_window_handle
        &mut new_windows,
        &system_callbacks,
        &mut stop_propagation,
        &mut focus_target,
        &mut words_changed,
        &mut images_changed,
        &mut image_masks_changed,
        &mut css_props_changed,
        &scroll_states,
        &mut nodes_scrolled,
        DomNodeId { dom: DomId { inner: 0 }, node: NodeHierarchyItemId::from_crate_internal(Some(original_div_id)) }, // hit_dom_node
        Default::default(), // cursor_relative_to_item
        Default::default()  // cursor_in_viewport
    );

    let div_dom_node_id = DomNodeId { dom: DomId { inner: 0 }, node: NodeHierarchyItemId::from_crate_internal(Some(original_div_id)) };
    let actual_parent_of_div = callback_info.get_parent(div_dom_node_id);

    assert!(actual_parent_of_div.is_some(), "get_parent should find a parent for the original div");
    assert_eq!(
        actual_parent_of_div.unwrap().node.into_crate_internal().unwrap(),
        NodeId::new(0),
        "Parent of original <div> should be the original <table> (NodeId 0), skipping anonymous nodes."
    );

    let anon_td_dom_node_id = DomId { inner: 0, node: NodeHierarchyItemId::from_crate_internal(Some(anon_td_id)) };
    let actual_parent_of_anon_td = callback_info.get_parent(anon_td_dom_node_id);

    assert!(actual_parent_of_anon_td.is_some(), "get_parent should find a parent for anonymous <td>");
    assert_eq!(
        actual_parent_of_anon_td.unwrap().node.into_crate_internal().unwrap(),
        NodeId::new(0),
        "Parent of anonymous <td> should be the original <table> (NodeId 0), skipping anonymous <tr>."
    );

    let anon_tr_dom_node_id = DomId { inner: 0, node: NodeHierarchyItemId::from_crate_internal(Some(anon_tr_id)) };
    let actual_parent_of_anon_tr = callback_info.get_parent(anon_tr_dom_node_id);

    assert!(actual_parent_of_anon_tr.is_some(), "get_parent should find a parent for anonymous <tr>");
    assert_eq!(
        actual_parent_of_anon_tr.unwrap().node.into_crate_internal().unwrap(),
        NodeId::new(0),
        "Parent of anonymous <tr> should be the original <table> (NodeId 0)."
    );
}

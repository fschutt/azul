pub mod context;
pub mod intrinsic;
pub mod layout;

use std::collections::{BTreeMap, BTreeSet};

use azul_core::{
    app_resources::{
        DecodedImage, DpiScaleFactor, Epoch, IdNamespace, ImageCache, RendererResources,
        ResourceUpdate, TextExclusionArea,
    },
    callbacks::{DocumentId, HidpiAdjustedBounds, IFrameCallbackInfo, IFrameCallbackReturn},
    display_list::RenderCallbacks,
    dom::{NodeId, NodeType},
    id_tree::NodeDataContainer,
    styled_dom::{
        ChangedCssProperty, DomId, NodeHierarchyItemId, ParentWithNodeDepth, StyleFontFamiliesHash,
        StyledDom,
    },
    ui_solver::{
        LayoutDebugMessage, LayoutResult, OverflowingScrollNode, PositionedRectangle,
        RelayoutChanges,
    },
    window::{FullWindowState, LogicalPosition, LogicalRect, LogicalSize},
};
use azul_css::{AzString, CssProperty, CssPropertyType, LayoutPoint, LayoutRect, LayoutSize};
use rust_fontconfig::FcFontCache;

use self::{
    context::{determine_formatting_contexts, FormattingContext},
    intrinsic::{calculate_intrinsic_sizes, IntrinsicSizes},
    layout::{calculate_constrained_size, calculate_layout},
};

/// Main entry point for the layout system
/// Adds the image and font resources to the app_resources but does NOT add them to the RenderAPI
pub fn do_the_layout(
    styled_dom: StyledDom,
    image_cache: &ImageCache,
    fc_cache: &FcFontCache,
    renderer_resources: &mut RendererResources,
    current_window_dpi: DpiScaleFactor,
    all_resource_updates: &mut Vec<ResourceUpdate>,
    id_namespace: IdNamespace,
    document_id: &DocumentId,
    epoch: Epoch,
    callbacks: &RenderCallbacks,
    full_window_state: &FullWindowState,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> Vec<LayoutResult> {
    if let Some(messages) = debug_messages.as_mut() {
        messages.push(LayoutDebugMessage {
            message: format!(
                "Starting layout for window size: {:?}",
                full_window_state.size.dimensions
            )
            .into(),
            location: "do_the_layout".to_string().into(),
        });
    }

    // Add fonts and images to renderer resources
    use azul_core::app_resources::add_fonts_and_images;
    use rust_fontconfig::FcFontCache;
    add_fonts_and_images(
        image_cache,
        renderer_resources,
        current_window_dpi,
        fc_cache,
        id_namespace,
        epoch,
        document_id,
        all_resource_updates,
        &styled_dom,
        callbacks.load_font_fn,
        callbacks.parse_font_fn,
        callbacks.insert_into_active_gl_textures_fn,
    );

    let window_theme = full_window_state.theme;
    let mut current_dom_id = 0;
    let mut doms = vec![(
        None,
        DomId {
            inner: current_dom_id,
        },
        styled_dom,
        LogicalRect::new(LogicalPosition::zero(), full_window_state.size.dimensions),
    )];
    let mut resolved_doms = Vec::new();
    let mut new_scroll_states = Vec::new();

    loop {
        let mut new_doms = Vec::new();

        for (parent_dom_id, dom_id, styled_dom, rect) in doms.drain(..) {
            // Process layout for this DOM
            let mut layout_result = do_the_layout_internal(
                dom_id,
                parent_dom_id,
                styled_dom,
                renderer_resources,
                document_id,
                rect,
                debug_messages,
            );

            let mut iframe_mapping = BTreeMap::new();

            // Handle iframe callbacks
            for iframe_node_id in layout_result.styled_dom.scan_for_iframe_callbacks() {
                // Generate a new DomID
                current_dom_id += 1;
                let iframe_dom_id = DomId {
                    inner: current_dom_id,
                };
                iframe_mapping.insert(iframe_node_id, iframe_dom_id);

                let bounds = &layout_result.rects.as_ref()[iframe_node_id];
                let bounds_size = LayoutSize::new(
                    bounds.size.width.round() as isize,
                    bounds.size.height.round() as isize,
                );
                let hidpi_bounds = HidpiAdjustedBounds::from_bounds(
                    bounds_size,
                    full_window_state.size.get_hidpi_factor(),
                );

                // Invoke the IFrame callback
                let iframe_return: IFrameCallbackReturn = {
                    let mut iframe_callback_info = IFrameCallbackInfo::new(
                        fc_cache,
                        image_cache,
                        window_theme,
                        hidpi_bounds,
                        bounds.size,
                        LogicalPosition::zero(),
                        bounds.size,
                        LogicalPosition::zero(),
                    );

                    let mut node_data_mut = layout_result.styled_dom.node_data.as_container_mut();
                    match &mut node_data_mut[iframe_node_id].get_iframe_node() {
                        Some(iframe_node) => (iframe_node.callback.cb)(
                            &mut iframe_node.data,
                            &mut iframe_callback_info,
                        ),
                        None => IFrameCallbackReturn::default(),
                    }
                };

                let IFrameCallbackReturn {
                    dom,
                    scroll_size,
                    scroll_offset,
                    virtual_scroll_size,
                    virtual_scroll_offset,
                } = iframe_return;

                let mut iframe_dom = dom;
                let (scroll_node_id, scroll_dom_id) = match parent_dom_id {
                    Some(s) => (iframe_node_id, s),
                    None => (NodeId::ZERO, DomId { inner: 0 }),
                };

                // Handle hover/active/focus state for iframe DOM
                let hovered_nodes = full_window_state
                    .last_hit_test
                    .hovered_nodes
                    .get(&iframe_dom_id)
                    .map(|i| i.regular_hit_test_nodes.clone())
                    .unwrap_or_default()
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>();

                let active_nodes = if !full_window_state.mouse_state.mouse_down() {
                    Vec::new()
                } else {
                    hovered_nodes.clone()
                };

                let _ = iframe_dom.restyle_nodes_hover(hovered_nodes.as_slice(), true);
                let _ = iframe_dom.restyle_nodes_active(active_nodes.as_slice(), true);
                if let Some(focused_node) = full_window_state.focused_node {
                    if focused_node.dom == iframe_dom_id {
                        let _ = iframe_dom.restyle_nodes_focus(
                            &[focused_node.node.into_crate_internal().unwrap()],
                            true,
                        );
                    }
                }

                // Calculate bounds and push iframe DOM for processing in next iteration
                let bounds =
                    LogicalRect::new(LogicalPosition::zero(), hidpi_bounds.get_logical_size());
                new_doms.push((Some(dom_id), iframe_dom_id, iframe_dom, bounds));

                // Track scroll state for iframes
                new_scroll_states.push(NewIframeScrollState {
                    dom_id: scroll_dom_id,
                    node_id: scroll_node_id,
                    child_rect: LogicalRect {
                        origin: scroll_offset,
                        size: scroll_size,
                    },
                    virtual_child_rect: LogicalRect {
                        origin: virtual_scroll_offset,
                        size: virtual_scroll_size,
                    },
                });
            }

            layout_result.iframe_mapping = iframe_mapping;
            resolved_doms.push(layout_result);
        }

        if new_doms.is_empty() {
            break;
        } else {
            doms = new_doms;
        }
    }

    // Process scroll states for all iframes
    for nss in new_scroll_states {
        if let Some(lr) = resolved_doms.get_mut(nss.dom_id.inner) {
            let mut osn = lr
                .scrollable_nodes
                .overflowing_nodes
                .entry(NodeHierarchyItemId::from_crate_internal(Some(nss.node_id)))
                .or_insert_with(|| OverflowingScrollNode::default());

            osn.child_rect = nss.child_rect;
            osn.virtual_child_rect = nss.virtual_child_rect;
        }
    }

    if let Some(messages) = debug_messages.as_mut() {
        messages.push(LayoutDebugMessage {
            message: format!("Layout completed with {} DOMs", resolved_doms.len()).into(),
            location: "do_the_layout".to_string().into(),
        });
    }

    resolved_doms
}

/// Core layout function that implements the multi-pass approach
/// At this point in time, all font keys, image keys, etc. have to be already
/// been submitted to the RenderApi and the AppResources!
#[cfg(feature = "text_layout")]
pub fn do_the_layout_internal(
    dom_id: DomId,
    parent_dom_id: Option<DomId>,
    styled_dom: StyledDom,
    renderer_resources: &mut RendererResources,
    document_id: &DocumentId,
    bounds: LogicalRect,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> LayoutResult {
    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: format!("Layout internal for DOM {}", dom_id.inner).into(),
            location: "do_the_layout_internal".to_string().into(),
        });
    }

    // Phase 1: Determine formatting context for each node
    let formatting_contexts = determine_formatting_contexts(&styled_dom);

    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: "Phase 1: Determined formatting contexts".into(),
            location: "do_the_layout_internal".to_string().into(),
        });
    }

    // Phase 2: Calculate intrinsic sizes
    let intrinsic_sizes =
        calculate_intrinsic_sizes(&styled_dom, &formatting_contexts, renderer_resources);

    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: "Phase 2: Calculated intrinsic sizes".into(),
            location: "do_the_layout_internal".to_string().into(),
        });
    }

    // Phase 3: Perform main layout calculation
    let mut layout_result = calculate_layout(
        dom_id,
        &styled_dom,
        &formatting_contexts,
        &intrinsic_sizes,
        bounds,
        renderer_resources,
        debug_messages,
    );

    // Set parent DOM ID
    layout_result.parent_dom_id = parent_dom_id;

    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: "Layout internal completed".into(),
            location: "do_the_layout_internal".to_string().into(),
        });
    }

    layout_result
}

/// Relayout function, takes an existing LayoutResult and adjusts it
/// so that only the nodes that need relayout are touched.
///
/// Returns a vec of node IDs that whose layout was changed
pub fn do_the_relayout(
    dom_id: DomId,
    root_bounds: LayoutRect,
    layout_result: &mut LayoutResult,
    image_cache: &ImageCache,
    renderer_resources: &mut RendererResources,
    document_id: &DocumentId,
    nodes_to_relayout: Option<&BTreeMap<NodeId, Vec<ChangedCssProperty>>>,
    words_to_relayout: Option<&BTreeMap<NodeId, AzString>>,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> RelayoutChanges {
    // Early return if nothing needs to be recomputed
    let root_size = root_bounds.size;
    let root_size_changed = root_bounds != layout_result.get_bounds();

    if !root_size_changed && nodes_to_relayout.is_none() && words_to_relayout.is_none() {
        return RelayoutChanges::empty();
    }

    // Filter CSS properties that can trigger relayout
    let mut nodes_to_relayout = nodes_to_relayout.map(|n| {
        n.iter()
            .filter_map(|(node_id, changed_properties)| {
                let mut relayout_properties = BTreeMap::new();

                for prop in changed_properties.iter() {
                    let prop_type = prop.previous_prop.get_type();
                    if prop_type.can_trigger_relayout() {
                        relayout_properties.insert(prop_type, prop.clone());
                    }
                }

                if relayout_properties.is_empty() {
                    None
                } else {
                    Some((*node_id, relayout_properties))
                }
            })
            .collect::<BTreeMap<NodeId, BTreeMap<CssPropertyType, ChangedCssProperty>>>()
    });

    // If after filtering there's nothing to relayout, just update GPU cache
    if !root_size_changed && nodes_to_relayout.is_none() && words_to_relayout.is_none() {
        let resized_nodes = Vec::new();
        let gpu_key_changes = layout_result
            .gpu_value_cache
            .synchronize(&layout_result.rects.as_ref(), &layout_result.styled_dom);

        return RelayoutChanges {
            resized_nodes,
            gpu_key_changes,
        };
    }

    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: format!(
                "Starting relayout: root_size_changed={}, nodes_changed={}, words_changed={}",
                root_size_changed,
                nodes_to_relayout.as_ref().map_or(0, |m| m.len()),
                words_to_relayout.as_ref().map_or(0, |m| m.len())
            )
            .into(),
            location: "do_the_relayout".to_string().into(),
        });
    }

    // Determine which nodes need to be re-processed
    let mut nodes_needing_format_context_update = BTreeSet::new();
    let mut nodes_needing_intrinsic_size_update = BTreeSet::new();
    let mut nodes_needing_layout_update = BTreeSet::new();
    let mut parents_of_changed_nodes = BTreeSet::new();

    // Track changes to display property - requires full reprocessing
    let mut display_changed = false;

    // Process changed properties to determine what needs updating
    if let Some(changed_nodes) = &nodes_to_relayout {
        for (node_id, properties) in changed_nodes {
            // Track changes that affect formatting context
            if properties.contains_key(&CssPropertyType::Display)
                || properties.contains_key(&CssPropertyType::Position)
                || properties.contains_key(&CssPropertyType::Float)
            {
                nodes_needing_format_context_update.insert(*node_id);
                display_changed = true;
            }

            // Track changes that affect intrinsic size
            if properties.contains_key(&CssPropertyType::Width)
                || properties.contains_key(&CssPropertyType::MinWidth)
                || properties.contains_key(&CssPropertyType::MaxWidth)
                || properties.contains_key(&CssPropertyType::Height)
                || properties.contains_key(&CssPropertyType::MinHeight)
                || properties.contains_key(&CssPropertyType::MaxHeight)
                || properties.contains_key(&CssPropertyType::PaddingLeft)
                || properties.contains_key(&CssPropertyType::PaddingRight)
                || properties.contains_key(&CssPropertyType::PaddingTop)
                || properties.contains_key(&CssPropertyType::PaddingBottom)
                || properties.contains_key(&CssPropertyType::FontSize)
            {
                nodes_needing_intrinsic_size_update.insert(*node_id);
            }

            // Any change means we need to update layout
            nodes_needing_layout_update.insert(*node_id);

            // Track parent for bubbling up layout changes
            if let Some(parent_id) =
                layout_result.styled_dom.node_hierarchy.as_container()[*node_id].parent_id()
            {
                parents_of_changed_nodes.insert(parent_id);
            }
        }
    }

    // Root size change requires recalculating everything
    if root_size_changed {
        let root_id = layout_result
            .styled_dom
            .root
            .into_crate_internal()
            .unwrap_or(NodeId::ZERO);
        nodes_needing_layout_update.insert(root_id);

        // Update the root size in the layout result
        layout_result.root_size = root_size;
    }

    // Handle text content changes
    if let Some(words_map) = words_to_relayout {
        for (node_id, _) in words_map {
            nodes_needing_intrinsic_size_update.insert(*node_id);
            nodes_needing_layout_update.insert(*node_id);

            // Add parents for layout recalculation
            if let Some(parent_id) =
                layout_result.styled_dom.node_hierarchy.as_container()[*node_id].parent_id()
            {
                parents_of_changed_nodes.insert(parent_id);
            }
        }
    }

    // Propagate layout updates to parents and their descendants
    for parent_id in parents_of_changed_nodes {
        nodes_needing_layout_update.insert(parent_id);

        // Add all descendants of affected parents
        let subtree = layout_result.styled_dom.get_subtree_parents(parent_id);
        nodes_needing_layout_update.extend(subtree);
    }

    // Update formatting contexts if needed
    let mut updated_formatting_contexts = None;
    if display_changed || !nodes_needing_format_context_update.is_empty() {
        updated_formatting_contexts =
            Some(determine_formatting_contexts(&layout_result.styled_dom));

        if let Some(messages) = debug_messages {
            messages.push(LayoutDebugMessage {
                message: "Re-determined formatting contexts".into(),
                location: "do_the_relayout".to_string().into(),
            });
        }
    }

    // Update intrinsic sizes for affected nodes
    let mut updated_intrinsic_sizes = None;
    if !nodes_needing_intrinsic_size_update.is_empty() {
        // We need the formatting contexts for intrinsic size calculation
        let formatting_contexts = if let Some(updated) = &updated_formatting_contexts {
            updated
        } else {
            updated_formatting_contexts =
                Some(determine_formatting_contexts(&layout_result.styled_dom));
            updated_formatting_contexts.as_ref().unwrap()
        };

        // For now, recalculate all intrinsic sizes (optimization opportunity)
        updated_intrinsic_sizes = Some(calculate_intrinsic_sizes(
            &layout_result.styled_dom,
            formatting_contexts,
            renderer_resources,
        ));

        if let Some(messages) = debug_messages {
            messages.push(LayoutDebugMessage {
                message: "Re-calculated intrinsic sizes".into(),
                location: "do_the_relayout".to_string().into(),
            });
        }
    }

    // Perform partial layout for affected nodes
    let mut all_nodes_to_update = BTreeSet::new();
    all_nodes_to_update.extend(nodes_needing_layout_update);

    // Convert root_bounds to LogicalRect
    let logical_bounds = LogicalRect::new(
        LogicalPosition::new(root_bounds.origin.x as f32, root_bounds.origin.y as f32),
        LogicalSize::new(
            root_bounds.size.width as f32,
            root_bounds.size.height as f32,
        ),
    );

    // Store the original positioned rectangles for diffing
    let original_rects: BTreeMap<NodeId, PositionedRectangle> = all_nodes_to_update
        .iter()
        .filter_map(|node_id| {
            layout_result
                .rects
                .as_ref()
                .get(*node_id)
                .map(|rect| (*node_id, rect.clone()))
        })
        .collect();

    // Perform the layout update
    // Use existing formatting contexts and intrinsic sizes where possible
    let formatting_contexts = updated_formatting_contexts
        .as_ref()
        .unwrap_or(&determine_formatting_contexts(&layout_result.styled_dom));

    let intrinsic_sizes = if let Some(updated) = &updated_intrinsic_sizes {
        updated
    } else {
        updated_intrinsic_sizes = Some(calculate_intrinsic_sizes(
            &layout_result.styled_dom,
            formatting_contexts,
            renderer_resources,
        ));
        updated_intrinsic_sizes.as_ref().unwrap()
    };

    // TODO: Optimize this to only update affected nodes
    // For now, we'll perform a full layout - a more targeted relayout would be an optimization
    let updated_layout_result = calculate_layout(
        dom_id,
        &layout_result.styled_dom,
        formatting_contexts,
        intrinsic_sizes,
        logical_bounds,
        renderer_resources,
        debug_messages,
    );

    // Track which nodes have changed size
    let mut resized_nodes = Vec::new();
    for node_id in &all_nodes_to_update {
        if let (Some(original), Some(updated)) = (
            original_rects.get(node_id),
            updated_layout_result.rects.as_ref().get(*node_id),
        ) {
            if original.size != updated.size || original.position != updated.position {
                resized_nodes.push(*node_id);
            }
        }
    }

    // Update layout_result with new values
    *layout_result = updated_layout_result;
    layout_result.root_size = root_size;
    layout_result.root_position = root_bounds.origin;

    // Update GPU cache
    let gpu_key_changes = layout_result
        .gpu_value_cache
        .synchronize(&layout_result.rects.as_ref(), &layout_result.styled_dom);

    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: format!("Relayout completed, {} nodes resized", resized_nodes.len()).into(),
            location: "do_the_relayout".to_string().into(),
        });
    }

    RelayoutChanges {
        resized_nodes,
        gpu_key_changes,
    }
}

struct NewIframeScrollState {
    dom_id: DomId,
    node_id: NodeId,
    child_rect: LogicalRect,
    virtual_child_rect: LogicalRect,
}

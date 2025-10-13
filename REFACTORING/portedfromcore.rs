impl WindowInternal {
    /// Initializes the `WindowInternal` on window creation. Calls the layout() method once to
    /// initializes the layout
    #[cfg(feature = "std")]
    pub fn new<F>(
        mut init: WindowInternalInit,
        data: &mut RefAny,
        image_cache: &ImageCache,
        gl_context: &OptionGlContextPtr,
        all_resource_updates: &mut Vec<ResourceUpdate>,
        callbacks: &RenderCallbacks,
        fc_cache_real: &mut FcFontCache,
        relayout_fn: RelayoutFn,
        hit_test_func: F,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Self
    where
        F: Fn(&FullWindowState, &ScrollStates, &[LayoutResult]) -> FullHitTest,
    {
        use crate::{
            callbacks::LayoutCallbackInfo,
            window_state::{NodesToCheck, StyleAndLayoutChanges},
        };

        // TODO: This function needs to be completely rewritten to use azul_layout::LayoutWindow
        // For now, we create a minimal LayoutResult to allow compilation

        let mut inital_renderer_resources = RendererResources::default();

        let epoch = Epoch::new();

        let styled_dom = {
            let layout_callback = &mut init.window_create_options.state.layout_callback;
            let mut layout_info = LayoutCallbackInfo::new(
                init.window_create_options.state.size,
                init.window_create_options.state.theme,
                image_cache,
                gl_context,
                &fc_cache_real,
            );

            match layout_callback {
                LayoutCallback::Raw(r) => (r.cb)(data, &mut layout_info),
                LayoutCallback::Marshaled(m) => {
                    let marshal_data = &mut m.marshal_data;
                    (m.cb.cb)(marshal_data, data, &mut layout_info)
                }
            }
        };

        let mut current_window_state = FullWindowState::from_window_state(
            /* window_state: */ &init.window_create_options.state,
            /* dropped_file: */ None,
            /* hovered_file: */ None,
            /* focused_node: */ None,
            /* last_hit_test: */ FullHitTest::empty(/* current_focus */ None),
            /* selections: */ BTreeMap::new(),
        );

        // TODO: Replace with azul_layout::LayoutWindow::new() + layout_and_generate_display_list()
        // For now, create a minimal empty LayoutResult
        let mut layout_results = vec![LayoutResult {
            dom_id: DomId { inner: 0 },
            parent_dom_id: None,
            styled_dom: styled_dom.clone(),
            root_size: LayoutSize::zero(),
            root_position: LayoutPoint::zero(),
            rects: NodeDataContainer::default(),
            scrollable_nodes: Default::default(),
            iframe_mapping: BTreeMap::new(),
            gpu_value_cache: Default::default(),
            formatting_contexts: NodeDataContainer::default(),
            intrinsic_sizes: NodeDataContainer::default(),
        }];

        let scroll_states = ScrollStates::default();

        // apply the changes for the first frame:
        // simulate an event as if the cursor has moved over the hovered elements
        let ht = hit_test_func(&current_window_state, &scroll_states, &layout_results);
        current_window_state.last_hit_test = ht.clone();

        let nodes_to_check = NodesToCheck::simulated_mouse_move(
            &ht,
            None, // focused_node
            current_window_state.mouse_state.mouse_down(),
        );

        let _ = StyleAndLayoutChanges::new(
            &nodes_to_check,
            &mut layout_results,
            &image_cache,
            &mut inital_renderer_resources,
            current_window_state.size.get_layout_size(),
            &init.document_id,
            Some(&BTreeMap::new()),
            Some(&BTreeMap::new()),
            &None,
            relayout_fn,
        );

        let gl_texture_cache = GlTextureCache::new(
            &mut layout_results,
            gl_context,
            init.id_namespace,
            &init.document_id,
            epoch,
            current_window_state.size.get_hidpi_factor(),
            image_cache,
            &fc_cache_real,
            callbacks,
            all_resource_updates,
            &mut inital_renderer_resources,
        );

        WindowInternal {
            renderer_resources: inital_renderer_resources,
            renderer_type: gl_context.as_ref().map(|r| r.renderer_type),
            id_namespace: init.id_namespace,
            previous_window_state: None,
            current_window_state,
            document_id: init.document_id,
            epoch, // = 0
            layout_results,
            gl_texture_cache,
            timers: BTreeMap::new(),
            threads: BTreeMap::new(),
            scroll_states,
        }
    }

    /// Calls the layout function again and updates the self.internal.gl_texture_cache field
    pub fn regenerate_styled_dom<F>(
        &mut self,
        data: &mut RefAny,
        image_cache: &ImageCache,
        gl_context: &OptionGlContextPtr,
        all_resource_updates: &mut Vec<ResourceUpdate>,
        current_window_dpi: DpiScaleFactor,
        callbacks: &RenderCallbacks,
        fc_cache_real: &mut FcFontCache,
        relayout_fn: RelayoutFn,
        mut hit_test_func: F,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) where
        F: FnMut(&FullWindowState, &ScrollStates, &[LayoutResult]) -> FullHitTest,
    {
        use crate::{
            callbacks::LayoutCallbackInfo,
            gl::gl_textures_remove_epochs_from_pipeline,
            styled_dom::DefaultCallbacksCfg,
            window_state::{NodesToCheck, StyleAndLayoutChanges},
        };

        // TODO: This function needs to be completely rewritten to use azul_layout::LayoutWindow

        let id_namespace = self.id_namespace;

        let mut styled_dom = {
            let layout_callback = &mut self.current_window_state.layout_callback;
            let mut layout_info = LayoutCallbackInfo::new(
                self.current_window_state.size,
                self.current_window_state.theme,
                image_cache,
                gl_context,
                &fc_cache_real,
            );

            match layout_callback {
                LayoutCallback::Raw(r) => (r.cb)(data, &mut layout_info),
                LayoutCallback::Marshaled(m) => {
                    let marshal_data = &mut m.marshal_data;
                    (m.cb.cb)(marshal_data, data, &mut layout_info)
                }
            }
        };

        styled_dom.insert_default_system_callbacks(DefaultCallbacksCfg {
            smooth_scroll: self.current_window_state.flags.smooth_scroll_enabled,
            enable_autotab: self.current_window_state.flags.autotab_enabled,
        });

        // TODO: Replace with azul_layout::LayoutWindow API
        let mut layout_results = vec![LayoutResult {
            dom_id: DomId { inner: 0 },
            parent_dom_id: None,
            styled_dom: styled_dom.clone(),
            root_size: LayoutSize::zero(),
            root_position: LayoutPoint::zero(),
            rects: NodeDataContainer::default(),
            scrollable_nodes: Default::default(),
            iframe_mapping: BTreeMap::new(),
            gpu_value_cache: Default::default(),
            formatting_contexts: NodeDataContainer::default(),
            intrinsic_sizes: NodeDataContainer::default(),
        }];

        // apply the changes for the first frame
        let ht = hit_test_func(
            &self.current_window_state,
            &self.scroll_states,
            &layout_results,
        );
        self.current_window_state.last_hit_test = ht.clone();

        // hit_test
        let nodes_to_check = NodesToCheck::simulated_mouse_move(
            &ht,
            self.current_window_state.focused_node,
            self.current_window_state.mouse_state.mouse_down(),
        );

        let sl = StyleAndLayoutChanges::new(
            &nodes_to_check,
            &mut layout_results,
            &image_cache,
            &mut self.renderer_resources,
            self.current_window_state.size.get_layout_size(),
            &self.document_id,
            Some(&BTreeMap::new()),
            Some(&BTreeMap::new()),
            &None,
            relayout_fn,
        );

        // inserts the new textures for the next frame
        let gl_texture_cache = GlTextureCache::new(
            &mut layout_results,
            gl_context,
            self.id_namespace,
            &self.document_id,
            self.epoch,
            self.current_window_state.size.get_hidpi_factor(),
            image_cache,
            &fc_cache_real,
            callbacks,
            all_resource_updates,
            &mut self.renderer_resources,
        );

        // removes the last frames' OpenGL textures
        gl_textures_remove_epochs_from_pipeline(&self.document_id, self.epoch);

        // Delete unused font and image keys (that were not used in this frame)
        self.renderer_resources.do_gc(
            all_resource_updates,
            image_cache,
            &layout_results,
            &gl_texture_cache,
        );

        // Increment epoch here!
        self.epoch.increment();
        self.layout_results = layout_results;
        self.gl_texture_cache = gl_texture_cache;
    }

    /// Returns a copy of the current scroll states + scroll positions
    pub fn get_current_scroll_states(
        &self,
    ) -> BTreeMap<DomId, BTreeMap<NodeHierarchyItemId, ScrollPosition>> {
        self.layout_results
            .iter()
            .enumerate()
            .filter_map(|(dom_id, layout_result)| {
                let scroll_positions = layout_result
                    .scrollable_nodes
                    .overflowing_nodes
                    .iter()
                    .filter_map(|(node_id, overflowing_node)| {
                        let scroll_position = ScrollPosition {
                            parent_rect: overflowing_node.parent_rect,
                            children_rect: overflowing_node.child_rect,
                        };
                        Some((*node_id, scroll_position))
                    })
                    .collect::<BTreeMap<_, _>>();

                if scroll_positions.is_empty() {
                    None
                } else {
                    Some((DomId { inner: dom_id }, scroll_positions))
                }
            })
            .collect()
    }

    /// Returns the overflowing size of the root body node. If WindowCreateOptions.size_to_content
    /// is set, the window size should be adjusted to this size before the window is shown.
    pub fn get_content_size(&self) -> LogicalSize {
        let layout_result = match self.layout_results.get(0) {
            Some(s) => s,
            None => return LogicalSize::zero(),
        };
        let root_width = layout_result.rects.as_ref()[NodeId::ZERO].get_margin_box_width();
        let root_height = layout_result.rects.as_ref()[NodeId::ZERO].get_margin_box_height();
        LogicalSize::new(root_width, root_height)
    }

    /// Does a full re-layout (without calling layout()) again:
    /// called in simple resize() scenarios
    pub fn do_quick_resize(
        &mut self,
        image_cache: &ImageCache,
        callbacks: &RenderCallbacks,
        relayout_fn: RelayoutFn,
        fc_cache: &FcFontCache,
        gl_context: &OptionGlContextPtr,
        window_size: &WindowSize,
        window_theme: WindowTheme,
    ) -> QuickResizeResult {
        // TODO: This needs to be rewritten to use azul_layout::LayoutWindow::resize_window()
        // For now, return an empty result
        QuickResizeResult {
            gpu_event_changes: Default::default(),
            updated_images: Vec::new(),
            resized_nodes: BTreeMap::new(),
        }
    }

    /// Returns whether the size or position of the window changed (if true,
    /// the caller needs to update the monitor field), since the window may have
    /// moved to a different monitor
    pub fn may_have_changed_monitor(&self) -> bool {
        let previous = match self.previous_window_state.as_ref() {
            None => return true,
            Some(s) => s,
        };
        let current = &self.current_window_state;

        previous.size.dimensions != current.size.dimensions && previous.position != current.position
    }

    pub fn get_layout_size(&self) -> LayoutSize {
        LayoutSize::new(
            libm::roundf(self.current_window_state.size.dimensions.width) as isize,
            libm::roundf(self.current_window_state.size.dimensions.height) as isize,
        )
    }

    /// Returns the menu bar set on the LayoutResults[0] node 0 or None
    pub fn get_menu_bar<'a>(&'a self) -> Option<&'a Box<Menu>> {
        let lr = self.layout_results.get(0)?;
        let ndc = lr.styled_dom.node_data.as_container();
        let nd = ndc.get_extended_lifetime(NodeId::ZERO)?;
        let mb = nd.get_menu_bar();
        mb
    }

    /// Returns the current context menu on the nearest hit node
    /// or None if no context menu was found
    pub fn get_context_menu<'a>(&'a self) -> Option<(&'a Box<Menu>, HitTestItem, DomNodeId)> {
        let mut context_menu = None;
        let hit_test = &self.current_window_state.last_hit_test;

        for (dom_id, hit_test) in hit_test.hovered_nodes.iter() {
            let layout_result = self.layout_results.get(dom_id.inner)?;
            for (node_id, hit) in hit_test.regular_hit_test_nodes.iter() {
                let ndc = layout_result.styled_dom.node_data.as_container();
                if let Some(cm) = ndc
                    .get_extended_lifetime(*node_id)
                    .and_then(|node| node.get_context_menu())
                {
                    if self
                        .current_window_state
                        .mouse_state
                        .matches(&cm.context_mouse_btn)
                    {
                        let domnode = DomNodeId {
                            dom: *dom_id,
                            node: NodeHierarchyItemId::from_crate_internal(Some(*node_id)),
                        };
                        context_menu = Some((cm, hit.clone(), domnode));
                    }
                }
            }
        }
        context_menu
    }

    // NOTE: The following 4 callback methods have been migrated to layout::LayoutWindow:
    // - run_single_timer()
    // - run_all_threads()
    // - invoke_single_callback()
    // - invoke_menu_callback()
    //
    // See layout/src/window.rs for the new implementations that use layout::CallbackInfo
}

impl WindowInternal {
    pub fn get_dpi_scale_factor(&self) -> DpiScaleFactor {
        DpiScaleFactor {
            inner: FloatValue::new(self.current_window_state.size.get_hidpi_factor()),
        }
    }
}

pub fn new_cursor_type_hit_test(hit_test: &FullHitTest, layout_results: &[LayoutResult]) -> CursorTypeHitTest {
    use azul_css::props::style::StyleCursor;

    let mut cursor_node = None;
    let mut cursor_icon = MouseCursorType::Default;

    for (dom_id, hit_nodes) in hit_test.hovered_nodes.iter() {
        for (node_id, _) in hit_nodes.regular_hit_test_nodes.iter() {
            // if the node has a non-default cursor: property, insert it
            let styled_dom = &layout_results[dom_id.inner].styled_dom;
            let node_data_container = styled_dom.node_data.as_container();
            if let Some(cursor_prop) = styled_dom.get_css_property_cache().get_cursor(
                &node_data_container[*node_id],
                node_id,
                &styled_dom.styled_nodes.as_container()[*node_id].state,
            ) {
                cursor_node = Some((*dom_id, *node_id));
                let ci = cursor_prop.get_property().copied().unwrap_or_default(); 
                cursor_icon = translate_cursor(ci);
            }
        }
    }

    Self {
        cursor_node,
        cursor_icon,
    }
}

#[derive(Debug, Clone)]
pub struct CallbacksOfHitTest {
    /// A BTreeMap where each item is already filtered by the proper hit-testing type,
    /// meaning in order to get the proper callbacks, you simply have to iterate through
    /// all node IDs
    pub nodes_with_callbacks: BTreeMap<DomId, Vec<CallbackToCall>>,
}

impl CallbacksOfHitTest {
    /// Determine which event / which callback(s) should be called and in which order
    ///
    /// This function also updates / mutates the current window states `focused_node`
    /// as well as the `window_state.previous_state`
    pub fn new(
        nodes_to_check: &NodesToCheck,
        events: &Events,
        layout_results: &[LayoutResult],
    ) -> Self {
        let mut nodes_with_callbacks = BTreeMap::new();

        if events.is_empty() {
            return Self {
                nodes_with_callbacks,
            };
        }

        let default_map = BTreeMap::new();
        let mouseenter_filter = EventFilter::Hover(HoverEventFilter::MouseEnter);
        let mouseleave_filter = EventFilter::Hover(HoverEventFilter::MouseEnter);
        let focus_received_filter = EventFilter::Focus(FocusEventFilter::FocusReceived);
        let focus_lost_filter = EventFilter::Focus(FocusEventFilter::FocusLost);

        for (dom_id, layout_result) in layout_results.iter().enumerate() {
            let dom_id = DomId { inner: dom_id };

            // Insert Window:: event filters
            let mut window_callbacks_this_dom = layout_result
                .styled_dom
                .nodes_with_window_callbacks
                .iter()
                .flat_map(|nid| {
                    let node_id = match nid.into_crate_internal() {
                        Some(s) => s,
                        None => return Vec::new(),
                    };
                    layout_result.styled_dom.node_data.as_container()[node_id]
                        .get_callbacks()
                        .iter()
                        .filter_map(|cb| match cb.event {
                            EventFilter::Window(wev) => {
                                if events.window_events.contains(&wev) {
                                    Some(CallbackToCall {
                                        event_filter: EventFilter::Window(wev),
                                        hit_test_item: None,
                                        node_id,
                                    })
                                } else {
                                    None
                                }
                            }
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();

            // window_callbacks_this_dom now contains all WindowEvent filters

            // insert Hover::MouseEnter events
            window_callbacks_this_dom.extend(
                nodes_to_check
                    .onmouseenter_nodes
                    .get(&dom_id)
                    .unwrap_or(&default_map)
                    .iter()
                    .filter_map(|(node_id, ht)| {
                        if layout_result.styled_dom.node_data.as_container()[*node_id]
                            .get_callbacks()
                            .iter()
                            .any(|e| e.event == mouseenter_filter)
                        {
                            Some(CallbackToCall {
                                event_filter: mouseenter_filter.clone(),
                                hit_test_item: Some(*ht),
                                node_id: *node_id,
                            })
                        } else {
                            None
                        }
                    }),
            );

            // insert Hover::MouseLeave events
            window_callbacks_this_dom.extend(
                nodes_to_check
                    .onmouseleave_nodes
                    .get(&dom_id)
                    .unwrap_or(&default_map)
                    .iter()
                    .filter_map(|(node_id, ht)| {
                        if layout_result.styled_dom.node_data.as_container()[*node_id]
                            .get_callbacks()
                            .iter()
                            .any(|e| e.event == mouseleave_filter)
                        {
                            Some(CallbackToCall {
                                event_filter: mouseleave_filter.clone(),
                                hit_test_item: Some(*ht),
                                node_id: *node_id,
                            })
                        } else {
                            None
                        }
                    }),
            );

            // insert other Hover:: events
            for (nid, ht) in nodes_to_check
                .new_hit_node_ids
                .get(&dom_id)
                .unwrap_or(&default_map)
                .iter()
            {
                for hev in events.hover_events.iter() {
                    window_callbacks_this_dom.extend(
                        layout_result.styled_dom.node_data.as_container()[*nid]
                            .get_callbacks()
                            .iter()
                            .filter_map(|e| {
                                if e.event == EventFilter::Hover(*hev)
                                    && e.event != mouseenter_filter
                                    && e.event != mouseleave_filter
                                {
                                    Some(CallbackToCall {
                                        event_filter: EventFilter::Hover(hev.clone()),
                                        hit_test_item: Some(*ht),
                                        node_id: *nid,
                                    })
                                } else {
                                    None
                                }
                            }),
                    );
                }
            }

            // insert Focus(FocusReceived / FocusLost) event
            if nodes_to_check.new_focus_node != nodes_to_check.old_focus_node {
                if let Some(DomNodeId {
                    dom,
                    node: az_node_id,
                }) = nodes_to_check.old_focus_node
                {
                    if dom == dom_id {
                        if let Some(nid) = az_node_id.into_crate_internal() {
                            if layout_result.styled_dom.node_data.as_container()[nid]
                                .get_callbacks()
                                .iter()
                                .any(|e| e.event == focus_lost_filter)
                            {
                                window_callbacks_this_dom.push(CallbackToCall {
                                    event_filter: focus_lost_filter.clone(),
                                    hit_test_item: events
                                        .old_hit_node_ids
                                        .get(&dom_id)
                                        .and_then(|map| map.get(&nid))
                                        .cloned(),
                                    node_id: nid,
                                })
                            }
                        }
                    }
                }

                if let Some(DomNodeId {
                    dom,
                    node: az_node_id,
                }) = nodes_to_check.new_focus_node
                {
                    if dom == dom_id {
                        if let Some(nid) = az_node_id.into_crate_internal() {
                            if layout_result.styled_dom.node_data.as_container()[nid]
                                .get_callbacks()
                                .iter()
                                .any(|e| e.event == focus_received_filter)
                            {
                                window_callbacks_this_dom.push(CallbackToCall {
                                    event_filter: focus_received_filter.clone(),
                                    hit_test_item: events
                                        .old_hit_node_ids
                                        .get(&dom_id)
                                        .and_then(|map| map.get(&nid))
                                        .cloned(),
                                    node_id: nid,
                                })
                            }
                        }
                    }
                }
            }

            // Insert other Focus: events
            if let Some(DomNodeId {
                dom,
                node: az_node_id,
            }) = nodes_to_check.new_focus_node
            {
                if dom == dom_id {
                    if let Some(nid) = az_node_id.into_crate_internal() {
                        for fev in events.focus_events.iter() {
                            for cb in layout_result.styled_dom.node_data.as_container()[nid]
                                .get_callbacks()
                                .iter()
                            {
                                if cb.event == EventFilter::Focus(*fev)
                                    && cb.event != focus_received_filter
                                    && cb.event != focus_lost_filter
                                {
                                    window_callbacks_this_dom.push(CallbackToCall {
                                        event_filter: EventFilter::Focus(fev.clone()),
                                        hit_test_item: events
                                            .old_hit_node_ids
                                            .get(&dom_id)
                                            .and_then(|map| map.get(&nid))
                                            .cloned(),
                                        node_id: nid,
                                    })
                                }
                            }
                        }
                    }
                }
            }

            if !window_callbacks_this_dom.is_empty() {
                nodes_with_callbacks.insert(dom_id, window_callbacks_this_dom);
            }
        }

        // Final: insert Not:: event filters
        for (dom_id, layout_result) in layout_results.iter().enumerate() {
            let dom_id = DomId { inner: dom_id };

            let not_event_filters = layout_result
                .styled_dom
                .nodes_with_not_callbacks
                .iter()
                .flat_map(|node_id| {
                    let node_id = match node_id.into_crate_internal() {
                        Some(s) => s,
                        None => return Vec::new(),
                    };
                    layout_result.styled_dom.node_data.as_container()[node_id]
                        .get_callbacks()
                        .iter()
                        .filter_map(|cb| match cb.event {
                            EventFilter::Not(nev) => {
                                if nodes_with_callbacks.get(&dom_id).map(|v| {
                                    v.iter().any(|cb| {
                                        cb.node_id == node_id
                                            && cb.event_filter == nev.as_event_filter()
                                    })
                                }) != Some(true)
                                {
                                    Some(CallbackToCall {
                                        event_filter: EventFilter::Not(nev.clone()),
                                        hit_test_item: events
                                            .old_hit_node_ids
                                            .get(&dom_id)
                                            .and_then(|map| map.get(&node_id))
                                            .cloned(),
                                        node_id,
                                    })
                                } else {
                                    None
                                }
                            }
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();

            for cb in not_event_filters {
                nodes_with_callbacks
                    .entry(dom_id)
                    .or_insert_with(|| Vec::new())
                    .push(cb);
            }
        }

        CallbacksOfHitTest {
            nodes_with_callbacks,
        }
    }

    /// The actual function that calls the callbacks in their proper hierarchy and order
    pub fn call(
        &mut self,
        previous_window_state: &Option<FullWindowState>,
        full_window_state: &FullWindowState,
        raw_window_handle: &RawWindowHandle,
        scroll_states: &BTreeMap<DomId, BTreeMap<NodeHierarchyItemId, ScrollPosition>>,
        gl_context: &OptionGlContextPtr,
        layout_results: &mut Vec<LayoutResult>,
        modifiable_scroll_states: &mut ScrollStates,
        image_cache: &mut ImageCache,
        system_fonts: &mut FcFontCache,
        system_callbacks: &ExternalSystemCallbacks,
        renderer_resources: &RendererResources,
    ) -> CallCallbacksResult {
        use crate::{
            callbacks::CallbackInfo, styled_dom::ParentWithNodeDepth, window::WindowState,
        };

        let mut ret = CallCallbacksResult {
            should_scroll_render: false,
            callbacks_update_screen: Update::DoNothing,
            modified_window_state: None,
            css_properties_changed: None,
            words_changed: None,
            images_changed: None,
            image_masks_changed: None,
            nodes_scrolled_in_callbacks: None,
            update_focused_node: None,
            timers: None,
            threads: None,
            timers_removed: None,
            threads_removed: None,
            windows_created: Vec::new(),
            cursor_changed: false,
        };
        let mut new_focus_target = None;

        let current_cursor = full_window_state.mouse_state.mouse_cursor_type.clone();

        if self.nodes_with_callbacks.is_empty() {
            // common case
            return ret;
        }

        let mut ret_modified_window_state: WindowState = full_window_state.clone().into();
        let mut ret_modified_window_state_unmodified = ret_modified_window_state.clone();
        let mut ret_timers = FastHashMap::new();
        let mut ret_timers_removed = FastBTreeSet::new();
        let mut ret_threads = FastHashMap::new();
        let mut ret_threads_removed = FastBTreeSet::new();
        let mut ret_words_changed = BTreeMap::new();
        let mut ret_images_changed = BTreeMap::new();
        let mut ret_image_masks_changed = BTreeMap::new();
        let mut ret_css_properties_changed = BTreeMap::new();
        let mut ret_nodes_scrolled_in_callbacks = BTreeMap::new();

        {
            for (dom_id, callbacks_filter_list) in self.nodes_with_callbacks.iter() {
                let mut callbacks = BTreeMap::new();
                for cbtc in callbacks_filter_list {
                    callbacks
                        .entry(cbtc.node_id)
                        .or_insert_with(|| Vec::new())
                        .push((cbtc.hit_test_item, cbtc.event_filter));
                }
                let callbacks = callbacks;
                let mut empty_vec = Vec::new();
                let lr = match layout_results.get(dom_id.inner) {
                    Some(s) => s,
                    None => continue,
                };

                let mut blacklisted_event_types = BTreeSet::new();

                // Run all callbacks (front to back)
                for ParentWithNodeDepth { depth: _, node_id } in
                    lr.styled_dom.non_leaf_nodes.as_ref().iter().rev()
                {
                    let parent_node_id = node_id;
                    for child_id in parent_node_id
                        .into_crate_internal()
                        .unwrap()
                        .az_children(&lr.styled_dom.node_hierarchy.as_container())
                    {
                        for (hit_test_item, event_filter) in
                            callbacks.get(&child_id).unwrap_or(&empty_vec)
                        {
                            if blacklisted_event_types.contains(&*event_filter) {
                                continue;
                            }

                            let mut new_focus = None;
                            let mut stop_propagation = false;

                            let mut callback_info = CallbackInfo::new(
                                /* layout_results: */ &layout_results,
                                /* renderer_resources: */ renderer_resources,
                                /* previous_window_state: */ &previous_window_state,
                                /* current_window_state: */ &full_window_state,
                                /* modifiable_window_state: */
                                &mut ret_modified_window_state,
                                /* gl_context, */ gl_context,
                                /* image_cache, */ image_cache,
                                /* system_fonts, */ system_fonts,
                                /* timers: */ &mut ret_timers,
                                /* threads: */ &mut ret_threads,
                                /* timers_removed: */ &mut ret_timers_removed,
                                /* threads_removed: */ &mut ret_threads_removed,
                                /* current_window_handle: */ raw_window_handle,
                                /* new_windows: */ &mut ret.windows_created,
                                /* system_callbacks */ system_callbacks,
                                /* stop_propagation: */ &mut stop_propagation,
                                /* focus_target: */ &mut new_focus,
                                /* words_changed_in_callbacks: */ &mut ret_words_changed,
                                /* images_changed_in_callbacks: */ &mut ret_images_changed,
                                /* image_masks_changed_in_callbacks: */
                                &mut ret_image_masks_changed,
                                /* css_properties_changed_in_callbacks: */
                                &mut ret_css_properties_changed,
                                /* current_scroll_states: */ scroll_states,
                                /* nodes_scrolled_in_callback: */
                                &mut ret_nodes_scrolled_in_callbacks,
                                /* hit_dom_node: */
                                DomNodeId {
                                    dom: *dom_id,
                                    node: NodeHierarchyItemId::from_crate_internal(Some(child_id)),
                                },
                                /* cursor_relative_to_item: */
                                hit_test_item
                                    .as_ref()
                                    .map(|hi| hi.point_relative_to_item)
                                    .into(),
                                /* cursor_in_viewport: */
                                hit_test_item.as_ref().map(|hi| hi.point_in_viewport).into(),
                            );

                            let callback_return = {
                                // get a MUTABLE reference to the RefAny inside of the DOM
                                let node_data_container = lr.styled_dom.node_data.as_container();
                                if let Some(callback_data) =
                                    node_data_container.get(child_id).and_then(|nd| {
                                        nd.callbacks
                                            .as_ref()
                                            .iter()
                                            .find(|i| i.event == *event_filter)
                                    })
                                {
                                    let mut callback_data_clone = callback_data.clone();
                                    // Invoke callback
                                    (callback_data_clone.callback.cb)(
                                        &mut callback_data_clone.data,
                                        &mut callback_info,
                                    )
                                } else {
                                    Update::DoNothing
                                }
                            };

                            ret.callbacks_update_screen.max_self(callback_return);

                            if let Some(new_focus) = new_focus.clone() {
                                new_focus_target = Some(new_focus);
                            }

                            if stop_propagation {
                                blacklisted_event_types.insert(event_filter.clone());
                            }
                        }
                    }
                }

                // run the callbacks for node ID 0
                loop {
                    for ((hit_test_item, event_filter), root_id) in lr
                        .styled_dom
                        .root
                        .into_crate_internal()
                        .map(|root_id| {
                            callbacks
                                .get(&root_id)
                                .unwrap_or(&empty_vec)
                                .iter()
                                .map(|item| (item, root_id))
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default()
                    {
                        if blacklisted_event_types.contains(&event_filter) {
                            break; // break out of loop
                        }

                        let mut new_focus = None;
                        let mut stop_propagation = false;

                        let mut callback_info = CallbackInfo::new(
                            /* layout_results: */ &layout_results,
                            /* renderer_resources: */ renderer_resources,
                            /* previous_window_state: */ &previous_window_state,
                            /* current_window_state: */ &full_window_state,
                            /* modifiable_window_state: */ &mut ret_modified_window_state,
                            /* gl_context, */ gl_context,
                            /* image_cache, */ image_cache,
                            /* system_fonts, */ system_fonts,
                            /* timers: */ &mut ret_timers,
                            /* threads: */ &mut ret_threads,
                            /* timers_removed: */ &mut ret_timers_removed,
                            /* threads_removed: */ &mut ret_threads_removed,
                            /* current_window_handle: */ raw_window_handle,
                            /* new_windows: */ &mut ret.windows_created,
                            /* system_callbacks */ system_callbacks,
                            /* stop_propagation: */ &mut stop_propagation,
                            /* focus_target: */ &mut new_focus,
                            /* words_changed_in_callbacks: */ &mut ret_words_changed,
                            /* images_changed_in_callbacks: */ &mut ret_images_changed,
                            /* image_masks_changed_in_callbacks: */
                            &mut ret_image_masks_changed,
                            /* css_properties_changed_in_callbacks: */
                            &mut ret_css_properties_changed,
                            /* current_scroll_states: */ scroll_states,
                            /* nodes_scrolled_in_callback: */
                            &mut ret_nodes_scrolled_in_callbacks,
                            /* hit_dom_node: */
                            DomNodeId {
                                dom: *dom_id,
                                node: NodeHierarchyItemId::from_crate_internal(Some(root_id)),
                            },
                            /* cursor_relative_to_item: */
                            hit_test_item
                                .as_ref()
                                .map(|hi| hi.point_relative_to_item)
                                .into(),
                            /* cursor_in_viewport: */
                            hit_test_item.as_ref().map(|hi| hi.point_in_viewport).into(),
                        );

                        let callback_return = {
                            // get a MUTABLE reference to the RefAny inside of the DOM
                            let node_data_container = lr.styled_dom.node_data.as_container();
                            if let Some(callback_data) =
                                node_data_container.get(root_id).and_then(|nd| {
                                    nd.callbacks
                                        .as_ref()
                                        .iter()
                                        .find(|i| i.event == *event_filter)
                                })
                            {
                                // Invoke callback
                                let mut callback_data_clone = callback_data.clone();
                                (callback_data_clone.callback.cb)(
                                    &mut callback_data_clone.data,
                                    &mut callback_info,
                                )
                            } else {
                                Update::DoNothing
                            }
                        };

                        ret.callbacks_update_screen.max_self(callback_return);

                        if let Some(new_focus) = new_focus.clone() {
                            new_focus_target = Some(new_focus);
                        }

                        if stop_propagation {
                            blacklisted_event_types.insert(event_filter.clone());
                        }
                    }

                    break;
                }
            }
        }

        // Scroll nodes from programmatic callbacks
        for (dom_id, callback_scrolled_nodes) in ret_nodes_scrolled_in_callbacks.iter() {
            let scrollable_nodes = &layout_results[dom_id.inner].scrollable_nodes;
            for (scroll_node_id, scroll_position) in callback_scrolled_nodes.iter() {
                let scroll_node = match scrollable_nodes.overflowing_nodes.get(&scroll_node_id) {
                    Some(s) => s,
                    None => continue,
                };

                modifiable_scroll_states.set_scroll_position(&scroll_node, *scroll_position);
                ret.should_scroll_render = true;
            }
        }

        // Resolve the new focus target
        if let Some(ft) = new_focus_target {
            if let Ok(new_focus_node) = ft.resolve(&layout_results, full_window_state.focused_node)
            {
                ret.update_focused_node = Some(new_focus_node);
            }
        }

        if current_cursor != ret_modified_window_state.mouse_state.mouse_cursor_type {
            ret.cursor_changed = true;
        }

        if !ret_timers.is_empty() {
            ret.timers = Some(ret_timers);
        }
        if !ret_threads.is_empty() {
            ret.threads = Some(ret_threads);
        }
        if ret_modified_window_state != ret_modified_window_state_unmodified {
            ret.modified_window_state = Some(ret_modified_window_state);
        }
        if !ret_threads_removed.is_empty() {
            ret.threads_removed = Some(ret_threads_removed);
        }
        if !ret_timers_removed.is_empty() {
            ret.timers_removed = Some(ret_timers_removed);
        }
        if !ret_words_changed.is_empty() {
            ret.words_changed = Some(ret_words_changed);
        }
        if !ret_images_changed.is_empty() {
            ret.images_changed = Some(ret_images_changed);
        }
        if !ret_image_masks_changed.is_empty() {
            ret.image_masks_changed = Some(ret_image_masks_changed);
        }
        if !ret_css_properties_changed.is_empty() {
            ret.css_properties_changed = Some(ret_css_properties_changed);
        }
        if !ret_nodes_scrolled_in_callbacks.is_empty() {
            ret.nodes_scrolled_in_callbacks = Some(ret_nodes_scrolled_in_callbacks);
        }

        ret
    }
}


/// VERY IMPORTANT: Main "callback" typedef
pub type CallbackType = extern "C" fn(&mut RefAny, &mut CallbackInfo) -> Update;

/// Stores a function pointer that is executed when the given UI element is hit
///
/// Must return an `Update` that denotes if the screen should be redrawn.
/// The style is not affected by this, so if you make changes to the window's style
/// inside the function, the screen will not be automatically redrawn, unless you return
/// an `Update::Redraw` from the function
#[repr(C)]
pub struct Callback {
    pub cb: CallbackType,
}
impl_callback!(Callback);

impl_option!(
    Callback,
    OptionCallback,
    [Debug, Eq, Copy, Clone, PartialEq, PartialOrd, Ord, Hash]
);


#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct CallbackData {
    pub event: EventFilter,
    pub callback: Callback,
    pub data: RefAny,
}

impl_vec!(CallbackData, CallbackDataVec, CallbackDataVecDestructor);
impl_vec_clone!(CallbackData, CallbackDataVec, CallbackDataVecDestructor);
impl_vec_mut!(CallbackData, CallbackDataVec);
impl_vec_debug!(CallbackData, CallbackDataVec);
impl_vec_partialord!(CallbackData, CallbackDataVec);
impl_vec_ord!(CallbackData, CallbackDataVec);
impl_vec_partialeq!(CallbackData, CallbackDataVec);
impl_vec_eq!(CallbackData, CallbackDataVec);
impl_vec_hash!(CallbackData, CallbackDataVec);

impl CallbackDataVec {
    #[inline]
    pub fn as_container<'a>(&'a self) -> NodeDataContainerRef<'a, CallbackData> {
        NodeDataContainerRef {
            internal: self.as_ref(),
        }
    }
    #[inline]
    pub fn as_container_mut<'a>(&'a mut self) -> NodeDataContainerRefMut<'a, CallbackData> {
        NodeDataContainerRefMut {
            internal: self.as_mut(),
        }
    }
}

// --- Render Image Callback / OpenGL callback ---

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct ImageCallback {
    pub data: RefAny,
    pub callback: RenderImageCallback,
}

/// Callbacks that returns a rendered OpenGL texture
#[repr(C)]
pub struct RenderImageCallback {
    pub cb: RenderImageCallbackType,
}
impl_callback!(RenderImageCallback);

#[derive(Debug)]
#[repr(C)]
pub struct RenderImageCallbackInfo {
    /// The ID of the DOM node that the ImageCallback was attached to
    callback_node_id: DomNodeId,
    /// Bounds of the laid-out node
    bounds: HidpiAdjustedBounds,
    /// Optional OpenGL context pointer
    gl_context: *const OptionGlContextPtr,
    image_cache: *const ImageCache,
    system_fonts: *const FcFontCache,
    node_hierarchy: *const NodeHierarchyItemVec,
    positioned_rects: *const NodeDataContainer<PositionedRectangle>,
    /// Extension for future ABI stability (referenced data)
    _abi_ref: *const c_void,
    /// Extension for future ABI stability (mutable data)
    _abi_mut: *mut c_void,
}

// same as the implementations on CallbackInfo, just slightly adjusted for the
// RenderImageCallbackInfo
impl Clone for RenderImageCallbackInfo {
    fn clone(&self) -> Self {
        Self {
            callback_node_id: self.callback_node_id,
            bounds: self.bounds,
            gl_context: self.gl_context,
            image_cache: self.image_cache,
            system_fonts: self.system_fonts,
            node_hierarchy: self.node_hierarchy,
            positioned_rects: self.positioned_rects,
            _abi_ref: self._abi_ref,
            _abi_mut: self._abi_mut,
        }
    }
}

impl RenderImageCallbackInfo {
    pub fn new<'a>(
        gl_context: &'a OptionGlContextPtr,
        image_cache: &'a ImageCache,
        system_fonts: &'a FcFontCache,
        node_hierarchy: &'a NodeHierarchyItemVec,
        positioned_rects: &'a NodeDataContainer<PositionedRectangle>,
        bounds: HidpiAdjustedBounds,
        callback_node_id: DomNodeId,
    ) -> Self {
        Self {
            callback_node_id,
            gl_context: gl_context as *const OptionGlContextPtr,
            image_cache: image_cache as *const ImageCache,
            system_fonts: system_fonts as *const FcFontCache,
            node_hierarchy: node_hierarchy as *const NodeHierarchyItemVec,
            positioned_rects: positioned_rects as *const NodeDataContainer<PositionedRectangle>,
            bounds,
            _abi_ref: core::ptr::null(),
            _abi_mut: core::ptr::null_mut(),
        }
    }

    fn internal_get_gl_context<'a>(&'a self) -> &'a OptionGlContextPtr {
        unsafe { &*self.gl_context }
    }
    fn internal_get_image_cache<'a>(&'a self) -> &'a ImageCache {
        unsafe { &*self.image_cache }
    }
    fn internal_get_system_fonts<'a>(&'a self) -> &'a FcFontCache {
        unsafe { &*self.system_fonts }
    }
    fn internal_get_bounds<'a>(&'a self) -> HidpiAdjustedBounds {
        self.bounds
    }
    fn internal_get_node_hierarchy<'a>(&'a self) -> &'a NodeHierarchyItemVec {
        unsafe { &*self.node_hierarchy }
    }
    fn internal_get_positioned_rectangles<'a>(
        &'a self,
    ) -> &'a NodeDataContainer<PositionedRectangle> {
        unsafe { &*self.positioned_rects }
    }

    pub fn get_gl_context(&self) -> OptionGlContextPtr {
        self.internal_get_gl_context().clone()
    }
    pub fn get_bounds(&self) -> HidpiAdjustedBounds {
        self.internal_get_bounds()
    }
    pub fn get_callback_node_id(&self) -> DomNodeId {
        self.callback_node_id
    }

    pub fn get_parent(&self, node_id: DomNodeId) -> Option<DomNodeId> {
        if node_id.dom != self.get_callback_node_id().dom {
            None
        } else {
            self.internal_get_node_hierarchy()
                .as_container()
                .get(node_id.node.into_crate_internal()?)?
                .parent_id()
                .map(|nid| DomNodeId {
                    dom: node_id.dom,
                    node: NodeHierarchyItemId::from_crate_internal(Some(nid)),
                })
        }
    }

    pub fn get_previous_sibling(&self, node_id: DomNodeId) -> Option<DomNodeId> {
        if node_id.dom != self.get_callback_node_id().dom {
            None
        } else {
            self.internal_get_node_hierarchy()
                .as_container()
                .get(node_id.node.into_crate_internal()?)?
                .previous_sibling_id()
                .map(|nid| DomNodeId {
                    dom: node_id.dom,
                    node: NodeHierarchyItemId::from_crate_internal(Some(nid)),
                })
        }
    }

    pub fn get_next_sibling(&self, node_id: DomNodeId) -> Option<DomNodeId> {
        if node_id.dom != self.get_callback_node_id().dom {
            None
        } else {
            self.internal_get_node_hierarchy()
                .as_container()
                .get(node_id.node.into_crate_internal()?)?
                .next_sibling_id()
                .map(|nid| DomNodeId {
                    dom: node_id.dom,
                    node: NodeHierarchyItemId::from_crate_internal(Some(nid)),
                })
        }
    }

    pub fn get_first_child(&self, node_id: DomNodeId) -> Option<DomNodeId> {
        if node_id.dom != self.get_callback_node_id().dom {
            None
        } else {
            let nid = node_id.node.into_crate_internal()?;
            self.internal_get_node_hierarchy()
                .as_container()
                .get(nid)?
                .first_child_id(nid)
                .map(|nid| DomNodeId {
                    dom: node_id.dom,
                    node: NodeHierarchyItemId::from_crate_internal(Some(nid)),
                })
        }
    }

    pub fn get_last_child(&self, node_id: DomNodeId) -> Option<DomNodeId> {
        if node_id.dom != self.get_callback_node_id().dom {
            None
        } else {
            self.internal_get_node_hierarchy()
                .as_container()
                .get(node_id.node.into_crate_internal()?)?
                .last_child_id()
                .map(|nid| DomNodeId {
                    dom: node_id.dom,
                    node: NodeHierarchyItemId::from_crate_internal(Some(nid)),
                })
        }
    }
}

/// Callback that - given the width and height of the expected image - renders an image
pub type RenderImageCallbackType =
    extern "C" fn(&mut RefAny, &mut RenderImageCallbackInfo) -> ImageRef;


// --- timer callback ---


/// A `Timer` is a function that is run on every frame.
///
/// There are often a lot of visual Threads such as animations or fetching the
/// next frame for a GIF or video, etc. - that need to run every frame or every X milliseconds,
/// but they aren't heavy enough to warrant creating a thread - otherwise the framework
/// would create too many threads, which leads to a lot of context switching and bad performance.
///
/// The callback of a `Timer` should be fast enough to run under 16ms,
/// otherwise running timers will block the main UI thread.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct Timer {
    /// Data that is internal to the timer
    pub data: RefAny,
    /// Optional node that the timer is attached to - timers attached to a DOM node
    /// will be automatically stopped when the UI is recreated.
    pub node_id: OptionDomNodeId,
    /// Stores when the timer was created (usually acquired by `Instant::now()`)
    pub created: Instant,
    /// When the timer was last called (`None` only when the timer hasn't been called yet).
    pub last_run: OptionInstant,
    /// How many times the callback was run
    pub run_count: usize,
    /// If the timer shouldn't start instantly, but rather be delayed by a certain timeframe
    pub delay: OptionDuration,
    /// How frequently the timer should run, i.e. set this to `Some(Duration::from_millis(16))`
    /// to run the timer every 16ms. If this value is set to `None`, (the default), the timer
    /// will execute the timer as-fast-as-possible (i.e. at a faster framerate
    /// than the framework itself) - which might be  performance intensive.
    pub interval: OptionDuration,
    /// When to stop the timer (for example, you can stop the
    /// execution after 5s using `Some(Duration::from_secs(5))`).
    pub timeout: OptionDuration,
    /// Callback to be called for this timer
    pub callback: TimerCallback,
}

impl Timer {
    /// Create a new timer
    pub fn new(
        data: RefAny,
        callback: TimerCallbackType,
        get_system_time_fn: GetSystemTimeCallback,
    ) -> Self {
        Timer {
            data,
            node_id: None.into(),
            created: (get_system_time_fn.cb)(),
            run_count: 0,
            last_run: OptionInstant::None,
            delay: OptionDuration::None,
            interval: OptionDuration::None,
            timeout: OptionDuration::None,
            callback: TimerCallback { cb: callback },
        }
    }

    pub fn tick_millis(&self) -> u64 {
        match self.interval.as_ref() {
            Some(Duration::System(s)) => s.millis(),
            Some(Duration::Tick(s)) => s.tick_diff,
            None => 10, // ms
        }
    }

    /// Returns true ONCE on the LAST invocation of the timer
    /// This is useful if you want to run some animation and then
    /// when the timer finishes (i.e. all animations finish),
    /// rebuild the UI / DOM (so that the user does not notice any dropped frames).
    pub fn is_about_to_finish(&self, instant_now: &Instant) -> bool {
        let mut finish = false;
        if let OptionDuration::Some(timeout) = self.timeout {
            finish = instant_now
                .duration_since(&self.created)
                .greater_than(&timeout);
        }
        finish
    }

    /// Returns when the timer needs to run again
    pub fn instant_of_next_run(&self) -> Instant {
        let last_run = match self.last_run.as_ref() {
            Some(s) => s,
            None => &self.created,
        };

        last_run
            .clone()
            .add_optional_duration(self.delay.as_ref())
            .add_optional_duration(self.interval.as_ref())
    }

    /// Delays the timer to not start immediately but rather
    /// start after a certain time frame has elapsed.
    #[inline]
    pub fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = OptionDuration::Some(delay);
        self
    }

    /// Converts the timer into a timer, running the function only
    /// if the given `Duration` has elapsed since the last run
    #[inline]
    pub fn with_interval(mut self, interval: Duration) -> Self {
        self.interval = OptionDuration::Some(interval);
        self
    }

    /// Converts the timer into a countdown, by giving it a maximum duration
    /// (counted from the creation of the Timer, not the first use).
    #[inline]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = OptionDuration::Some(timeout);
        self
    }

    pub fn invoke(&mut self) {
        // Moved to azul_layout::timer.rs
    }
}


/// Callback that can runs on every frame on the main thread - can modify the app data model
#[repr(C)]
pub struct TimerCallback {
    pub cb: TimerCallbackType,
}
impl_callback!(TimerCallback);

#[derive(Debug)]
#[repr(C)]
pub struct TimerCallbackInfo {
    /// Callback info for this timer
    pub callback_info: CallbackInfo,
    /// If the timer is attached to a DOM node, this will contain the node ID
    pub node_id: OptionDomNodeId,
    /// Time when the frame was started rendering
    pub frame_start: Instant,
    /// How many times this callback has been called
    pub call_count: usize,
    /// Set to true ONCE on the LAST invocation of the timer (if the timer has a timeout set)
    /// This is useful to rebuild the DOM once the timer (usually an animation) has finished.
    pub is_about_to_finish: bool,
    /// Extension for future ABI stability (referenced data)
    pub(crate) _abi_ref: *const c_void,
    /// Extension for future ABI stability (mutable data)
    pub(crate) _abi_mut: *mut c_void,
}

impl Clone for TimerCallbackInfo {
    fn clone(&self) -> Self {
        Self {
            callback_info: self.callback_info.clone(),
            node_id: self.node_id,
            frame_start: self.frame_start.clone(),
            call_count: self.call_count,
            is_about_to_finish: self.is_about_to_finish,
            _abi_ref: self._abi_ref,
            _abi_mut: self._abi_mut,
        }
    }
}

pub type WriteBackCallbackType = extern "C" fn(
    /* original data */ &mut RefAny,
    /* data to write back */ &mut RefAny,
    &mut CallbackInfo,
) -> Update;

pub type ThreadCallbackType = extern "C" fn(RefAny, ThreadSender, ThreadReceiver);

#[repr(C)]
pub struct ThreadCallback {
    pub cb: ThreadCallbackType,
}
impl_callback!(ThreadCallback);

/// Callback that can runs when a thread receives a `WriteBack` message
#[repr(C)]
pub struct WriteBackCallback {
    pub cb: WriteBackCallbackType,
}
impl_callback!(WriteBackCallback);

pub type TimerCallbackType = extern "C" fn(
    /* timer internal data */ &mut RefAny,
    &mut TimerCallbackInfo,
) -> TimerCallbackReturn;

// callback that drives an animation
extern "C" fn drive_animation_func(
    anim_data: &mut RefAny,
    info: &mut TimerCallbackInfo,
) -> TimerCallbackReturn {
    let mut anim_data = match anim_data.downcast_mut::<AnimationData>() {
        Some(s) => s,
        None => {
            return TimerCallbackReturn {
                should_update: Update::DoNothing,
                should_terminate: TerminateTimer::Terminate,
            };
        }
    };

    let mut anim_data = &mut *anim_data;

    let node_id = match info.node_id.into_option() {
        Some(s) => s,
        None => {
            return TimerCallbackReturn {
                should_update: Update::DoNothing,
                should_terminate: TerminateTimer::Terminate,
            };
        }
    };

    // calculate the interpolated CSS property
    let resolver = InterpolateResolver {
        parent_rect_width: anim_data.parent_rect_width,
        parent_rect_height: anim_data.parent_rect_height,
        current_rect_width: anim_data.current_rect_width,
        current_rect_height: anim_data.current_rect_height,
        interpolate_func: anim_data.interpolate,
    };

    let anim_next_end = anim_data
        .start
        .add_optional_duration(Some(&anim_data.duration));
    let now = (anim_data.get_system_time_fn.cb)();
    let t = now.linear_interpolate(anim_data.start.clone(), anim_next_end.clone());
    let interpolated_css = anim_data.from.interpolate(&anim_data.to, t, &resolver);

    // actual animation happens here
    info.callback_info
        .set_css_property(node_id, interpolated_css);

    // if the timer has finished one iteration, what next?
    if now > anim_next_end {
        match anim_data.repeat {
            AnimationRepeat::Loop => {
                // reset timer
                anim_data.start = now;
            }
            AnimationRepeat::PingPong => {
                use core::mem;
                // swap start and end and reset timer
                mem::swap(&mut anim_data.from, &mut anim_data.to);
                anim_data.start = now;
            }
            AnimationRepeat::NoRepeat => {
                // remove / cancel timer
                return TimerCallbackReturn {
                    should_terminate: TerminateTimer::Terminate,
                    should_update: if anim_data.relayout_on_finish {
                        Update::RefreshDom
                    } else {
                        Update::DoNothing
                    },
                };
            }
        }
    }

    // if the timer has finished externally, what next?
    if info.is_about_to_finish {
        TimerCallbackReturn {
            should_terminate: TerminateTimer::Terminate,
            should_update: if anim_data.relayout_on_finish {
                Update::RefreshDom
            } else {
                Update::DoNothing
            },
        }
    } else {
        TimerCallbackReturn {
            should_terminate: TerminateTimer::Continue,
            should_update: Update::DoNothing,
        }
    }
}


#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C)]
pub struct ThreadWriteBackMsg {
    // The data to write back into. Will be passed as the second argument to the thread
    pub data: RefAny,
    // The callback to call on this data.
    pub callback: WriteBackCallback,
}

impl ThreadWriteBackMsg {
    pub fn new(callback: WriteBackCallbackType, data: RefAny) -> Self {
        Self {
            data,
            callback: WriteBackCallback { cb: callback },
        }
    }
}

// Message that is received from the running thread
#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C, u8)]
pub enum ThreadReceiveMsg {
    WriteBack(ThreadWriteBackMsg),
    Update(Update),
}

impl_option!(
    ThreadReceiveMsg,
    OptionThreadReceiveMsg,
    copy = false,
    [Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash]
);


#[derive(Debug)]
#[repr(C)]
pub struct ThreadSender {
    #[cfg(feature = "std")]
    pub ptr: Box<Arc<Mutex<ThreadSenderInner>>>,
    #[cfg(not(feature = "std"))]
    pub ptr: *const c_void,
    pub run_destructor: bool,
}

impl Clone for ThreadSender {
    fn clone(&self) -> Self {
        Self {
            ptr: self.ptr.clone(),
            run_destructor: true,
        }
    }
}

impl Drop for ThreadSender {
    fn drop(&mut self) {
        self.run_destructor = false;
    }
}

impl ThreadSender {
    #[cfg(not(feature = "std"))]
    pub fn new(t: ThreadSenderInner) -> Self {
        Self {
            ptr: core::ptr::null(),
            run_destructor: false,
        }
    }

    #[cfg(feature = "std")]
    pub fn new(t: ThreadSenderInner) -> Self {
        Self {
            ptr: Box::new(Arc::new(Mutex::new(t))),
            run_destructor: true,
        }
    }

    #[cfg(not(feature = "std"))]
    pub fn send(&mut self, msg: ThreadReceiveMsg) -> bool {
        false
    }

    // send data from the user thread to the main thread
    #[cfg(feature = "std")]
    pub fn send(&mut self, msg: ThreadReceiveMsg) -> bool {
        let ts = match self.ptr.lock().ok() {
            Some(s) => s,
            None => return false,
        };
        (ts.send_fn.cb)(ts.ptr.as_ref() as *const _ as *const c_void, msg)
    }
}


#[derive(Debug)]
#[cfg_attr(not(feature = "std"), derive(PartialEq, PartialOrd, Eq, Ord))]
#[repr(C)]
pub struct ThreadSenderInner {
    #[cfg(feature = "std")]
    pub ptr: Box<Sender<ThreadReceiveMsg>>,
    #[cfg(not(feature = "std"))]
    pub ptr: *const c_void,
    pub send_fn: ThreadSendCallback,
    pub destructor: ThreadSenderDestructorCallback,
}

#[cfg(not(feature = "std"))]
unsafe impl Send for ThreadSenderInner {}

#[cfg(feature = "std")]
impl core::hash::Hash for ThreadSenderInner {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        (self.ptr.as_ref() as *const _ as usize).hash(state);
    }
}

#[cfg(feature = "std")]
impl PartialEq for ThreadSenderInner {
    fn eq(&self, other: &Self) -> bool {
        (self.ptr.as_ref() as *const _ as usize) == (other.ptr.as_ref() as *const _ as usize)
    }
}

#[cfg(feature = "std")]
impl Eq for ThreadSenderInner {}

#[cfg(feature = "std")]
impl PartialOrd for ThreadSenderInner {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(
            (self.ptr.as_ref() as *const _ as usize)
                .cmp(&(other.ptr.as_ref() as *const _ as usize)),
        )
    }
}

#[cfg(feature = "std")]
impl Ord for ThreadSenderInner {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        (self.ptr.as_ref() as *const _ as usize).cmp(&(other.ptr.as_ref() as *const _ as usize))
    }
}

impl Drop for ThreadSenderInner {
    fn drop(&mut self) {
        (self.destructor.cb)(self);
    }
}


// function to receive a message from the thread
pub type LibraryReceiveThreadMsgCallbackType =
    extern "C" fn(/* Receiver<ThreadReceiveMsg> */ *const c_void) -> OptionThreadReceiveMsg;
#[repr(C)]
pub struct LibraryReceiveThreadMsgCallback {
    pub cb: LibraryReceiveThreadMsgCallbackType,
}
impl_callback!(LibraryReceiveThreadMsgCallback);


// function that the RUNNING THREAD can call to send messages to the main thread
pub type ThreadSendCallbackType =
    extern "C" fn(/* sender.ptr */ *const c_void, ThreadReceiveMsg) -> bool; // return false on error
#[repr(C)]
pub struct ThreadSendCallback {
    pub cb: ThreadSendCallbackType,
}
impl_callback!(ThreadSendCallback);


// destructor of the ThreadSender
pub type ThreadSenderDestructorCallbackType = extern "C" fn(*mut ThreadSenderInner);
#[repr(C)]
pub struct ThreadSenderDestructorCallback {
    pub cb: ThreadSenderDestructorCallbackType,
}
impl_callback!(ThreadSenderDestructorCallback);


/// Wrapper around Thread because Thread needs to be clone-able for Python
#[derive(Debug)]
#[repr(C)]
pub struct Thread {
    #[cfg(feature = "std")]
    pub ptr: Box<Arc<Mutex<ThreadInner>>>,
    #[cfg(not(feature = "std"))]
    pub ptr: *const c_void,
    pub run_destructor: bool,
}

impl Clone for Thread {
    fn clone(&self) -> Self {
        Self {
            ptr: self.ptr.clone(),
            run_destructor: true,
        }
    }
}

impl Drop for Thread {
    fn drop(&mut self) {
        self.run_destructor = false;
    }
}

impl Thread {
    #[cfg(feature = "std")]
    pub fn new(ti: ThreadInner) -> Self {
        Self {
            ptr: Box::new(Arc::new(Mutex::new(ti))),
            run_destructor: true,
        }
    }
    #[cfg(not(feature = "std"))]
    pub fn new(ti: ThreadInner) -> Self {
        Self {
            ptr: core::ptr::null(),
            run_destructor: false,
        }
    }
}

/// A `Thread` is a seperate thread that is owned by the framework.
///
/// In difference to a `Thread`, you don't have to `await()` the result of a `Thread`,
/// you can just hand the Thread to the framework (via `RendererResources::add_Thread`) and
/// the framework will automatically update the UI when the Thread is finished.
/// This is useful to offload actions such as loading long files, etc. to a background thread.
///
/// Azul will join the thread automatically after it is finished (joining won't block the UI).
#[derive(Debug)]
#[repr(C)]
pub struct ThreadInner {
    // Thread handle of the currently in-progress Thread
    #[cfg(feature = "std")]
    pub thread_handle: Box<Option<JoinHandle<()>>>,
    #[cfg(not(feature = "std"))]
    pub thread_handle: *const c_void,

    #[cfg(feature = "std")]
    pub sender: Box<Sender<ThreadSendMsg>>,
    #[cfg(not(feature = "std"))]
    pub sender: *const c_void,

    #[cfg(feature = "std")]
    pub receiver: Box<Receiver<ThreadReceiveMsg>>,
    #[cfg(not(feature = "std"))]
    pub receiver: *const c_void,

    #[cfg(feature = "std")]
    pub dropcheck: Box<Weak<()>>,
    #[cfg(not(feature = "std"))]
    pub dropcheck: *const c_void,

    pub writeback_data: RefAny,
    pub check_thread_finished_fn: CheckThreadFinishedCallback,
    pub send_thread_msg_fn: LibrarySendThreadMsgCallback,
    pub receive_thread_msg_fn: LibraryReceiveThreadMsgCallback,
    pub thread_destructor_fn: ThreadDestructorCallback,
}

#[cfg(feature = "std")]
impl ThreadInner {
    /// Returns true if the Thread has been finished, false otherwise
    pub fn is_finished(&self) -> bool {
        (self.check_thread_finished_fn.cb)(self.dropcheck.as_ref() as *const _ as *const c_void)
    }

    /// Send a message to the thread
    pub fn sender_send(&mut self, msg: ThreadSendMsg) -> bool {
        (self.send_thread_msg_fn.cb)(self.sender.as_ref() as *const _ as *const c_void, msg)
    }

    /// Try to receive a message from the thread (non-blocking)
    pub fn receiver_try_recv(&mut self) -> OptionThreadReceiveMsg {
        (self.receive_thread_msg_fn.cb)(self.receiver.as_ref() as *const _ as *const c_void)
    }
}

#[cfg(feature = "std")]
pub extern "C" fn create_thread_libstd(
    thread_initialize_data: RefAny,
    writeback_data: RefAny,
    callback: ThreadCallback,
) -> Thread {
    let (sender_receiver, receiver_receiver) = std::sync::mpsc::channel::<ThreadReceiveMsg>();
    let sender_receiver = ThreadSender::new(ThreadSenderInner {
        ptr: Box::new(sender_receiver),
        send_fn: ThreadSendCallback {
            cb: default_send_thread_msg_fn,
        },
        destructor: ThreadSenderDestructorCallback {
            cb: thread_sender_drop,
        },
    });

    let (sender_sender, receiver_sender) = std::sync::mpsc::channel::<ThreadSendMsg>();
    let receiver_sender = ThreadReceiver::new(ThreadReceiverInner {
        ptr: Box::new(receiver_sender),
        recv_fn: ThreadRecvCallback {
            cb: default_receive_thread_msg_fn,
        },
        destructor: ThreadReceiverDestructorCallback {
            cb: thread_receiver_drop,
        },
    });

    let thread_check = Arc::new(());
    let dropcheck = Arc::downgrade(&thread_check);

    let thread_handle = Some(thread::spawn(move || {
        let _ = thread_check;
        (callback.cb)(thread_initialize_data, sender_receiver, receiver_sender);
        // thread_check gets dropped here, signals that the thread has finished
    }));

    let thread_handle: Box<Option<JoinHandle<()>>> = Box::new(thread_handle);
    let sender: Box<Sender<ThreadSendMsg>> = Box::new(sender_sender);
    let receiver: Box<Receiver<ThreadReceiveMsg>> = Box::new(receiver_receiver);
    let dropcheck: Box<Weak<()>> = Box::new(dropcheck);

    Thread::new(ThreadInner {
        thread_handle,
        sender,
        receiver,
        writeback_data,
        dropcheck,
        thread_destructor_fn: ThreadDestructorCallback {
            cb: default_thread_destructor_fn,
        },
        check_thread_finished_fn: CheckThreadFinishedCallback {
            cb: default_check_thread_finished,
        },
        send_thread_msg_fn: LibrarySendThreadMsgCallback {
            cb: library_send_thread_msg_fn,
        },
        receive_thread_msg_fn: LibraryReceiveThreadMsgCallback {
            cb: library_receive_thread_msg_fn,
        },
    })
}

impl Drop for ThreadInner {
    fn drop(&mut self) {
        (self.thread_destructor_fn.cb)(self);
    }
}


#[cfg(not(feature = "std"))]
impl ThreadInner {
    /// Returns true if the Thread has been finished, false otherwise
    pub fn is_finished(&self) -> bool {
        true
    }

    /// Send a message to the thread (no-op in no_std)
    pub fn sender_send(&mut self, msg: ThreadSendMsg) -> bool {
        false
    }

    /// Try to receive a message from the thread (always returns None in no_std)
    pub fn receiver_try_recv(&mut self) -> OptionThreadReceiveMsg {
        None.into()
    }
}

#[cfg(feature = "std")]
pub extern "C" fn get_system_time_libstd() -> Instant {
    StdInstant::now().into()
}

#[cfg(not(feature = "std"))]
pub extern "C" fn get_system_time_libstd() -> Instant {
    Instant::Tick(SystemTick::new(0))
}

#[cfg(not(feature = "std"))]
pub extern "C" fn create_thread_libstd(
    thread_initialize_data: RefAny,
    writeback_data: RefAny,
    callback: ThreadCallback,
) -> Thread {
    Thread {
        ptr: core::ptr::null(),
        run_destructor: false,
    }
}

#[cfg(feature = "std")]
extern "C" fn default_thread_destructor_fn(thread: *mut ThreadInner) {
    let thread = unsafe { &mut *thread };

    if let Some(thread_handle) = thread.thread_handle.take() {
        let _ = thread.sender.send(ThreadSendMsg::TerminateThread);
        let _ = thread_handle.join(); // ignore the result, don't panic
    }
}

#[cfg(not(feature = "std"))]
extern "C" fn default_thread_destructor_fn(thread: *mut ThreadInner) {}

#[cfg(feature = "std")]
extern "C" fn library_send_thread_msg_fn(sender: *const c_void, msg: ThreadSendMsg) -> bool {
    unsafe { &*(sender as *const Sender<ThreadSendMsg>) }
        .send(msg)
        .is_ok()
}

#[cfg(not(feature = "std"))]
extern "C" fn library_send_thread_msg_fn(sender: *const c_void, msg: ThreadSendMsg) -> bool {
    false
}

#[cfg(feature = "std")]
extern "C" fn library_receive_thread_msg_fn(receiver: *const c_void) -> OptionThreadReceiveMsg {
    unsafe { &*(receiver as *const Receiver<ThreadReceiveMsg>) }
        .try_recv()
        .ok()
        .into()
}

#[cfg(not(feature = "std"))]
extern "C" fn library_receive_thread_msg_fn(receiver: *const c_void) -> OptionThreadReceiveMsg {
    None.into()
}

#[cfg(feature = "std")]
extern "C" fn default_send_thread_msg_fn(sender: *const c_void, msg: ThreadReceiveMsg) -> bool {
    unsafe { &*(sender as *const Sender<ThreadReceiveMsg>) }
        .send(msg)
        .is_ok()
}

#[cfg(not(feature = "std"))]
extern "C" fn default_send_thread_msg_fn(sender: *const c_void, msg: ThreadReceiveMsg) -> bool {
    false
}

#[cfg(feature = "std")]
extern "C" fn default_receive_thread_msg_fn(receiver: *const c_void) -> OptionThreadSendMsg {
    unsafe { &*(receiver as *const Receiver<ThreadSendMsg>) }
        .try_recv()
        .ok()
        .into()
}

#[cfg(not(feature = "std"))]
extern "C" fn default_receive_thread_msg_fn(receiver: *const c_void) -> OptionThreadSendMsg {
    None.into()
}

#[cfg(feature = "std")]
extern "C" fn default_check_thread_finished(dropcheck: *const c_void) -> bool {
    unsafe { &*(dropcheck as *const Weak<()>) }
        .upgrade()
        .is_none()
}

#[cfg(not(feature = "std"))]
extern "C" fn default_check_thread_finished(dropcheck: *const c_void) -> bool {
    true
}

#[cfg(feature = "std")]
extern "C" fn thread_sender_drop(_: *mut ThreadSenderInner) {}

#[cfg(not(feature = "std"))]
extern "C" fn thread_sender_drop(_: *mut ThreadSenderInner) {}

#[cfg(feature = "std")]
extern "C" fn thread_receiver_drop(_: *mut ThreadReceiverInner) {}

#[cfg(not(feature = "std"))]
extern "C" fn thread_receiver_drop(_: *mut ThreadReceiverInner) {}


// function called on Thread::drop()
pub type ThreadDestructorCallbackType = extern "C" fn(*mut ThreadInner);
#[repr(C)]
pub struct ThreadDestructorCallback {
    pub cb: ThreadDestructorCallbackType,
}
impl_callback!(ThreadDestructorCallback);

/// Config that is necessary so that threading + animations can compile on no_std
///
/// See the `default` implementations in this module for an example on how to
/// create a thread
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct ExternalSystemCallbacks {
    pub create_thread_fn: CreateThreadCallback,
    pub get_system_time_fn: GetSystemTimeCallback,
}

impl ExternalSystemCallbacks {
    #[cfg(not(feature = "std"))]
    pub fn rust_internal() -> Self {
        Self {
            create_thread_fn: CreateThreadCallback {
                cb: create_thread_libstd,
            },
            get_system_time_fn: GetSystemTimeCallback {
                cb: get_system_time_libstd,
            },
        }
    }

    #[cfg(feature = "std")]
    pub fn rust_internal() -> Self {
        Self {
            create_thread_fn: CreateThreadCallback {
                cb: create_thread_libstd,
            },
            get_system_time_fn: GetSystemTimeCallback {
                cb: get_system_time_libstd,
            },
        }
    }
}


/// Function that creates a new `Thread` object
pub type CreateThreadCallbackType = extern "C" fn(RefAny, RefAny, ThreadCallback) -> Thread;
#[repr(C)]
pub struct CreateThreadCallback {
    pub cb: CreateThreadCallbackType,
}
impl_callback!(CreateThreadCallback);


/// --- originally in core/window.rs ---

#[derive(Debug)]
pub struct CallCallbacksResult {
    /// Whether the UI should be rendered anyways due to a (programmatic or user input) scroll
    /// event
    pub should_scroll_render: bool,
    /// Whether the callbacks say to rebuild the UI or not
    pub callbacks_update_screen: Update,
    /// WindowState that was (potentially) modified in the callbacks
    pub modified_window_state: Option<WindowState>,
    /// If a word changed (often times the case with text input), we don't need to relayout /
    /// rerender the whole screen. The result is passed to the `relayout()` function, which
    /// will only change the single node that was modified
    pub words_changed: Option<BTreeMap<DomId, BTreeMap<NodeId, AzString>>>,
    /// A callback can "exchange" and image for a new one without requiring a new display list to
    /// be rebuilt. This is important for animated images, especially video.
    pub images_changed: Option<BTreeMap<DomId, BTreeMap<NodeId, (ImageRef, UpdateImageType)>>>,
    /// Same as images, clip masks can be changed in callbacks, often the case with vector
    /// animations
    pub image_masks_changed: Option<BTreeMap<DomId, BTreeMap<NodeId, ImageMask>>>,
    /// If the focus target changes in the callbacks, the function will automatically
    /// restyle the DOM and set the new focus target
    pub css_properties_changed: Option<BTreeMap<DomId, BTreeMap<NodeId, Vec<CssProperty>>>>,
    /// If the callbacks have scrolled any nodes, the new scroll position will be stored here
    pub nodes_scrolled_in_callbacks:
        Option<BTreeMap<DomId, BTreeMap<NodeHierarchyItemId, LogicalPosition>>>,
    /// Whether the focused node was changed from the callbacks
    pub update_focused_node: Option<Option<DomNodeId>>,
    /// Timers that were added in the callbacks
    pub timers: Option<FastHashMap<TimerId, Timer>>,
    /// Tasks that were added in the callbacks
    pub threads: Option<FastHashMap<ThreadId, Thread>>,
    /// Timers that were added in the callbacks
    pub timers_removed: Option<FastBTreeSet<TimerId>>,
    /// Tasks that were added in the callbacks
    pub threads_removed: Option<FastBTreeSet<ThreadId>>,
    /// Windows that were created in the callbacks
    pub windows_created: Vec<WindowCreateOptions>,
    /// Whether the cursor changed in the callbacks
    pub cursor_changed: bool,
}

impl CallCallbacksResult {
    pub fn cursor_changed(&self) -> bool {
        self.cursor_changed
    }
    pub fn focus_changed(&self) -> bool {
        self.update_focused_node.is_some()
    }
}


#[derive(Debug, Clone)]
#[repr(C)]
pub struct WindowCreateOptions {
    // Initial window state
    pub state: WindowState,
    /// If set, the first UI redraw will be called with a size of (0, 0) and the
    /// window size depends on the size of the overflowing UI. This is good for
    /// windows that do not want to take up unnecessary extra space
    pub size_to_content: bool,
    /// Renderer type: Hardware-with-software-fallback, pure software or pure hardware renderer?
    pub renderer: OptionRendererOptions,
    /// Override the default window theme (set to `None` to use the OS-provided theme)
    pub theme: OptionWindowTheme,
    /// Optional callback to run when the window has been created (runs only once on startup)
    pub create_callback: OptionCallback,
    /// If set to true, will hot-reload the UI every 200ms, useful in combination with
    /// `StyledDom::from_file()` to hot-reload the UI from a file while developing.
    pub hot_reload: bool,
}

impl Default for WindowCreateOptions {
    fn default() -> Self {
        Self {
            state: WindowState::default(),
            size_to_content: false,
            renderer: OptionRendererOptions::None,
            theme: OptionWindowTheme::None,
            create_callback: OptionCallback::None,
            hot_reload: false,
        }
    }
}

impl WindowCreateOptions {
    pub fn new(callback: LayoutCallbackType) -> Self {
        Self {
            state: WindowState::new(callback),
            ..WindowCreateOptions::default()
        }
    }
    pub fn renderer_types(&self) -> Vec<RendererType> {
        match self.renderer.into_option() {
            Some(s) => match s.hw_accel {
                HwAcceleration::DontCare => vec![RendererType::Hardware, RendererType::Software],
                HwAcceleration::Enabled => vec![RendererType::Hardware],
                HwAcceleration::Disabled => vec![RendererType::Software],
            },
            None => vec![RendererType::Hardware, RendererType::Software],
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FullWindowState {
    /// Theme of this window (dark or light) - can be set / overridden by the user
    ///
    /// Usually the operating system will set this field. On change, it will
    /// emit a `WindowEventFilter::ThemeChanged` event
    pub theme: WindowTheme,
    /// Current title of the window
    pub title: AzString,
    /// Size of the window + max width / max height: 800 x 600 by default
    pub size: WindowSize,
    /// The x and y position, or None to let the WM decide where to put the window (default)
    pub position: WindowPosition,
    /// Flags such as whether the window is minimized / maximized, fullscreen, etc.
    pub flags: WindowFlags,
    /// Mostly used for debugging, shows WebRender-builtin graphs on the screen.
    /// Used for performance monitoring and displaying frame times (rendering-only).
    pub debug_state: DebugState,
    /// Current keyboard state - NOTE: mutating this field (currently) does nothing
    /// (doesn't get synchronized with OS-level window)!
    pub keyboard_state: KeyboardState,
    /// Current mouse state
    pub mouse_state: MouseState,
    /// Stores all states of currently connected touch input devices, pencils, tablets, etc.
    pub touch_state: TouchState,
    /// Sets location of IME candidate box in client area coordinates
    /// relative to the top left of the window.
    pub ime_position: ImePosition,
    /// Window options that can only be set on a certain platform
    /// (`WindowsWindowOptions` / `LinuxWindowOptions` / `MacWindowOptions`).
    pub platform_specific_options: PlatformSpecificOptions,
    /// Information about vsync and hardware acceleration
    pub renderer_options: RendererOptions,
    /// Background color of the window
    pub background_color: ColorU,
    /// The `layout()` function for this window, stored as a callback function pointer,
    /// There are multiple reasons for doing this (instead of requiring `T: Layout` everywhere):
    ///
    /// - It seperates the `Dom` from the `Layout` trait, making it possible to split the UI
    ///   solving and styling into reusable crates
    /// - It's less typing work (prevents having to type `<T: Layout>` everywhere)
    /// - It's potentially more efficient to compile (less type-checking required)
    /// - It's a preparation for the C ABI, in which traits don't exist (for language bindings). In
    ///   the C ABI "traits" are simply structs with function pointers (and void* instead of T)
    pub layout_callback: LayoutCallback,
    /// Callback to run before the window closes. If this callback returns `DoNothing`,
    /// the window won't close, otherwise it'll close regardless
    pub close_callback: OptionCallback,
    // --
    /// Current monitor
    pub monitor: Monitor,
    /// Whether there is a file currently hovering over the window
    pub hovered_file: Option<AzString>, // Option<PathBuf>
    /// Whether there was a file currently dropped on the window
    pub dropped_file: Option<AzString>, // Option<PathBuf>
    /// What node is currently hovered over, default to None. Only necessary internal
    /// to the crate, for emitting `On::FocusReceived` and `On::FocusLost` events,
    /// as well as styling `:focus` elements
    pub focused_node: Option<DomNodeId>,
    /// Last hit-test that was performed: necessary because the
    /// events are stored in a queue and only storing the hovered
    /// nodes is not sufficient to correctly determine events
    pub last_hit_test: FullHitTest,
    /// Map of active selections, keyed by the root DOM ID of the
    /// formatting context containing the text (usually the IFrame or main DOM).
    pub selections: BTreeMap<DomId, SelectionState>,
}

impl Default for FullWindowState {
    fn default() -> Self {
        Self {
            theme: WindowTheme::default(),
            title: AzString::from_const_str(DEFAULT_TITLE),
            size: WindowSize::default(),
            position: WindowPosition::Uninitialized,
            flags: WindowFlags::default(),
            debug_state: DebugState::default(),
            keyboard_state: KeyboardState::default(),
            mouse_state: MouseState::default(),
            touch_state: TouchState::default(),
            ime_position: ImePosition::Uninitialized,
            platform_specific_options: PlatformSpecificOptions::default(),
            background_color: ColorU::WHITE,
            layout_callback: LayoutCallback::default(),
            close_callback: OptionCallback::None,
            renderer_options: RendererOptions::default(),
            monitor: Monitor::default(),
            // --
            hovered_file: None,
            dropped_file: None,
            focused_node: None,
            last_hit_test: FullHitTest::empty(None),
            selections: BTreeMap::new(),
        }
    }
}

impl FullWindowState {
    pub fn get_mouse_state(&self) -> &MouseState {
        &self.mouse_state
    }

    pub fn get_keyboard_state(&self) -> &KeyboardState {
        &self.keyboard_state
    }

    pub fn get_hovered_file(&self) -> Option<&AzString> {
        self.hovered_file.as_ref()
    }

    pub fn get_dropped_file(&self) -> Option<&AzString> {
        self.dropped_file.as_ref()
    }

    pub fn get_scroll_amount(&self) -> Option<(f32, f32)> {
        self.mouse_state.get_scroll_amount()
    }

    pub fn layout_callback_changed(&self, other: &Option<Self>) -> bool {
        match other {
            Some(s) => self.layout_callback != s.layout_callback,
            None => false,
        }
    }

    /// Creates a FullWindowState from a regular WindowState,
    /// fills non-available fields with the given values
    ///
    /// You need to pass the extra fields explicitly in order
    /// to prevent state management bugs
    pub fn from_window_state(
        window_state: &WindowState,
        dropped_file: Option<AzString>,
        hovered_file: Option<AzString>,
        focused_node: Option<DomNodeId>,
        last_hit_test: FullHitTest,
        selections: BTreeMap<DomId, SelectionState>,
    ) -> Self {
        Self {
            monitor: window_state.monitor.clone(),
            theme: window_state.theme,
            title: window_state.title.clone(),
            size: window_state.size,
            position: window_state.position.into(),
            flags: window_state.flags,
            debug_state: window_state.debug_state,
            keyboard_state: window_state.keyboard_state.clone(),
            mouse_state: window_state.mouse_state,
            touch_state: window_state.touch_state,
            ime_position: window_state.ime_position.into(),
            platform_specific_options: window_state.platform_specific_options.clone(),
            background_color: window_state.background_color,
            layout_callback: window_state.layout_callback.clone(),
            close_callback: window_state.close_callback,
            renderer_options: window_state.renderer_options,
            dropped_file,
            hovered_file,
            focused_node,
            last_hit_test,
            selections,
        }
    }

    pub fn process_system_scroll(&mut self, scroll_states: &ScrollStates) -> Option<ScrollResult> {
        let (x, y) = self.mouse_state.get_scroll_amount()?;
        // TODO
        Some(ScrollResult {})
    }
}

#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct WindowState {
    pub title: AzString,
    /// Theme of this window (dark or light) - can be set / overridden by the user
    ///
    /// Usually the operating system will set this field. On change, it will
    /// emit a `WindowEventFilter::ThemeChanged` event
    pub theme: WindowTheme,
    /// Size of the window + max width / max height: 800 x 600 by default
    pub size: WindowSize,
    /// The x and y position, or None to let the WM decide where to put the window (default)
    pub position: WindowPosition,
    /// Flags such as whether the window is minimized / maximized, fullscreen, etc.
    pub flags: WindowFlags,
    /// Mostly used for debugging, shows WebRender-builtin graphs on the screen.
    /// Used for performance monitoring and displaying frame times (rendering-only).
    pub debug_state: DebugState,
    /// Current keyboard state - NOTE: mutating this field (currently) does nothing
    /// (doesn't get synchronized with OS-level window)!
    pub keyboard_state: KeyboardState,
    /// Current mouse state
    pub mouse_state: MouseState,
    /// Stores all states of currently connected touch input devices, pencils, tablets, etc.
    pub touch_state: TouchState,
    /// Sets location of IME candidate box in client area coordinates
    /// relative to the top left of the window.
    pub ime_position: ImePosition,
    /// Which monitor the window is currently residing on
    pub monitor: Monitor,
    /// Window options that can only be set on a certain platform
    /// (`WindowsWindowOptions` / `LinuxWindowOptions` / `MacWindowOptions`).
    pub platform_specific_options: PlatformSpecificOptions,
    /// Whether this window has SRGB / vsync / hardware acceleration
    pub renderer_options: RendererOptions,
    /// Color of the window background (can be transparent if necessary)
    pub background_color: ColorU,
    /// The `layout()` function for this window, stored as a callback function pointer,
    /// There are multiple reasons for doing this (instead of requiring `T: Layout` everywhere):
    ///
    /// - It seperates the `Dom` from the `Layout` trait, making it possible to split the UI
    ///   solving and styling into reusable crates
    /// - It's less typing work (prevents having to type `<T: Layout>` everywhere)
    /// - It's potentially more efficient to compile (less type-checking required)
    /// - It's a preparation for the C ABI, in which traits don't exist (for language bindings). In
    ///   the C ABI "traits" are simply structs with function pointers (and void* instead of T)
    pub layout_callback: LayoutCallback,
    /// Optional callback to run when the window closes
    pub close_callback: OptionCallback,
}

impl_option!(
    WindowState,
    OptionWindowState,
    copy = false,
    [Debug, Clone, PartialEq]
);


impl From<FullWindowState> for WindowState {
    fn from(full_window_state: FullWindowState) -> WindowState {
        WindowState {
            monitor: full_window_state.monitor.clone(),
            theme: full_window_state.theme,
            title: full_window_state.title.into(),
            size: full_window_state.size,
            position: full_window_state.position.into(),
            flags: full_window_state.flags,
            debug_state: full_window_state.debug_state,
            keyboard_state: full_window_state.keyboard_state,
            mouse_state: full_window_state.mouse_state,
            touch_state: full_window_state.touch_state,
            ime_position: full_window_state.ime_position.into(),
            platform_specific_options: full_window_state.platform_specific_options,
            background_color: full_window_state.background_color,
            layout_callback: full_window_state.layout_callback,
            close_callback: full_window_state.close_callback,
            renderer_options: full_window_state.renderer_options,
        }
    }
}

/// Warning: if the previous_window_state is none, this will return an empty Vec!
pub fn create_new_events(
    current_window_state: &FullWindowState,
    previous_window_state: &Option<FullWindowState>,
) -> Events {
    let mut current_window_events =
        get_window_events(current_window_state, previous_window_state);
    let mut current_hover_events = get_hover_events(&current_window_events);
    let mut current_focus_events = get_focus_events(&current_hover_events);

    let event_was_mouse_down = current_window_events
        .iter()
        .any(|e| *e == WindowEventFilter::MouseDown);
    let event_was_mouse_release = current_window_events
        .iter()
        .any(|e| *e == WindowEventFilter::MouseUp);
    let event_was_mouse_leave = current_window_events
        .iter()
        .any(|e| *e == WindowEventFilter::MouseLeave);
    let current_window_state_mouse_is_down = current_window_state.mouse_state.mouse_down();
    let previous_window_state_mouse_is_down = previous_window_state
        .as_ref()
        .map(|f| f.mouse_state.mouse_down())
        .unwrap_or(false);

    let old_focus_node = previous_window_state
        .as_ref()
        .and_then(|f| f.focused_node.clone());
    let old_hit_node_ids = previous_window_state
        .as_ref()
        .map(|f| {
            if f.last_hit_test.hovered_nodes.is_empty() {
                BTreeMap::new()
            } else {
                f.last_hit_test
                    .hovered_nodes
                    .iter()
                    .map(|(dom_id, hit_test)| {
                        (*dom_id, hit_test.regular_hit_test_nodes.clone())
                    })
                    .collect()
            }
        })
        .unwrap_or_default();

    if let Some(prev_state) = previous_window_state.as_ref() {
        if prev_state.theme != current_window_state.theme {
            current_window_events.push(WindowEventFilter::ThemeChanged);
        }
        if current_window_state.last_hit_test.hovered_nodes
            != prev_state.last_hit_test.hovered_nodes.clone()
        {
            current_hover_events.push(HoverEventFilter::MouseLeave);
            current_hover_events.push(HoverEventFilter::MouseEnter);
        }
    }

    // even if there are no window events, the focus node can changed
    if current_window_state.focused_node != old_focus_node {
        current_focus_events.push(FocusEventFilter::FocusReceived);
        current_focus_events.push(FocusEventFilter::FocusLost);
    }

    Events {
        window_events: current_window_events,
        hover_events: current_hover_events,
        focus_events: current_focus_events,
        event_was_mouse_down,
        event_was_mouse_release,
        event_was_mouse_leave,
        current_window_state_mouse_is_down,
        previous_window_state_mouse_is_down,
        old_focus_node,
        old_hit_node_ids,
    }
}

fn get_window_events(
    current_window_state: &FullWindowState,
    previous_window_state: &Option<FullWindowState>,
) -> Vec<WindowEventFilter> {
    use crate::window::{CursorPosition::*, WindowPosition};

    let mut events = Vec::new();

    let previous_window_state = match previous_window_state.as_ref() {
        Some(s) => s,
        None => return events,
    };

    // match mouse move events first since they are the most common

    match (
        previous_window_state.mouse_state.cursor_position,
        current_window_state.mouse_state.cursor_position,
    ) {
        (InWindow(_), OutOfWindow(_)) | (InWindow(_), Uninitialized) => {
            events.push(WindowEventFilter::MouseLeave);
        }
        (OutOfWindow(_), InWindow(_)) | (Uninitialized, InWindow(_)) => {
            events.push(WindowEventFilter::MouseEnter);
        }
        (InWindow(a), InWindow(b)) => {
            if a != b {
                events.push(WindowEventFilter::MouseOver);
            }
        }
        _ => {}
    }

    if current_window_state.mouse_state.mouse_down()
        && !previous_window_state.mouse_state.mouse_down()
    {
        events.push(WindowEventFilter::MouseDown);
    }

    if current_window_state.mouse_state.left_down && !previous_window_state.mouse_state.left_down {
        events.push(WindowEventFilter::LeftMouseDown);
    }

    if current_window_state.mouse_state.right_down && !previous_window_state.mouse_state.right_down
    {
        events.push(WindowEventFilter::RightMouseDown);
    }

    if current_window_state.mouse_state.middle_down
        && !previous_window_state.mouse_state.middle_down
    {
        events.push(WindowEventFilter::MiddleMouseDown);
    }

    if previous_window_state.mouse_state.mouse_down()
        && !current_window_state.mouse_state.mouse_down()
    {
        events.push(WindowEventFilter::MouseUp);
    }

    if previous_window_state.mouse_state.left_down && !current_window_state.mouse_state.left_down {
        events.push(WindowEventFilter::LeftMouseUp);
    }

    if previous_window_state.mouse_state.right_down && !current_window_state.mouse_state.right_down
    {
        events.push(WindowEventFilter::RightMouseUp);
    }

    if previous_window_state.mouse_state.middle_down
        && !current_window_state.mouse_state.middle_down
    {
        events.push(WindowEventFilter::MiddleMouseUp);
    }

    // resize, move, close events

    if current_window_state.flags.has_focus != previous_window_state.flags.has_focus {
        if current_window_state.flags.has_focus {
            events.push(WindowEventFilter::FocusReceived);
            events.push(WindowEventFilter::WindowFocusReceived);
        } else {
            events.push(WindowEventFilter::FocusLost);
            events.push(WindowEventFilter::WindowFocusLost);
        }
    }

    if current_window_state.size.dimensions != previous_window_state.size.dimensions
        || current_window_state.size.dpi != previous_window_state.size.dpi
    {
        events.push(WindowEventFilter::Resized);
    }

    match (
        current_window_state.position,
        previous_window_state.position,
    ) {
        (WindowPosition::Initialized(cur_pos), WindowPosition::Initialized(prev_pos)) => {
            if prev_pos != cur_pos {
                events.push(WindowEventFilter::Moved);
            }
        }
        (WindowPosition::Initialized(_), WindowPosition::Uninitialized) => {
            events.push(WindowEventFilter::Moved);
        }
        _ => {}
    }

    let about_to_close_equals = current_window_state.flags.is_about_to_close
        == previous_window_state.flags.is_about_to_close;
    if current_window_state.flags.is_about_to_close && !about_to_close_equals {
        events.push(WindowEventFilter::CloseRequested);
    }

    // scroll events

    let is_scroll_previous = previous_window_state.mouse_state.scroll_x.is_some()
        || previous_window_state.mouse_state.scroll_y.is_some();

    let is_scroll_now = current_window_state.mouse_state.scroll_x.is_some()
        || current_window_state.mouse_state.scroll_y.is_some();

    if !is_scroll_previous && is_scroll_now {
        events.push(WindowEventFilter::ScrollStart);
    }

    if is_scroll_now {
        events.push(WindowEventFilter::Scroll);
    }

    if is_scroll_previous && !is_scroll_now {
        events.push(WindowEventFilter::ScrollEnd);
    }

    // keyboard events
    let cur_vk_equal = current_window_state.keyboard_state.current_virtual_keycode
        == previous_window_state.keyboard_state.current_virtual_keycode;
    let cur_char_equal = current_window_state.keyboard_state.current_char
        == previous_window_state.keyboard_state.current_char;

    if !cur_vk_equal
        && previous_window_state
            .keyboard_state
            .current_virtual_keycode
            .is_none()
        && current_window_state
            .keyboard_state
            .current_virtual_keycode
            .is_some()
    {
        events.push(WindowEventFilter::VirtualKeyDown);
    }

    if !cur_char_equal && current_window_state.keyboard_state.current_char.is_some() {
        events.push(WindowEventFilter::TextInput);
    }

    if !cur_vk_equal
        && previous_window_state
            .keyboard_state
            .current_virtual_keycode
            .is_some()
        && current_window_state
            .keyboard_state
            .current_virtual_keycode
            .is_none()
    {
        events.push(WindowEventFilter::VirtualKeyUp);
    }

    // misc events

    let hovered_file_equals =
        previous_window_state.hovered_file == current_window_state.hovered_file;
    if previous_window_state.hovered_file.is_none()
        && current_window_state.hovered_file.is_some()
        && !hovered_file_equals
    {
        events.push(WindowEventFilter::HoveredFile);
    }

    if previous_window_state.hovered_file.is_some() && current_window_state.hovered_file.is_none() {
        if current_window_state.dropped_file.is_some() {
            events.push(WindowEventFilter::DroppedFile);
        } else {
            events.push(WindowEventFilter::HoveredFileCancelled);
        }
    }

    if current_window_state.theme != previous_window_state.theme {
        events.push(WindowEventFilter::ThemeChanged);
    }

    events
}

/// Overwrites all fields of the `FullWindowState` with the fields of the `WindowState`,
/// but leaves the extra fields such as `.hover_nodes` untouched
pub fn update_full_window_state(
    full_window_state: &mut FullWindowState,
    window_state: &WindowState,
) {
    full_window_state.title = window_state.title.clone();
    full_window_state.size = window_state.size.into();
    full_window_state.position = window_state.position.into();
    full_window_state.flags = window_state.flags;
    full_window_state.debug_state = window_state.debug_state;
    full_window_state.keyboard_state = window_state.keyboard_state.clone();
    full_window_state.mouse_state = window_state.mouse_state;
    full_window_state.ime_position = window_state.ime_position.into();
    full_window_state.platform_specific_options = window_state.platform_specific_options.clone();
}

pub struct WindowInternalInit {
    pub window_create_options: WindowCreateOptions,
    pub document_id: DocumentId,
    pub id_namespace: IdNamespace,
}

impl WindowState {
    /// Creates a new, default `WindowState` with the given CSS style
    pub fn new(callback: LayoutCallbackType) -> Self {
        use crate::callbacks::LayoutCallbackInner;
        Self {
            layout_callback: LayoutCallback::Raw(LayoutCallbackInner { cb: callback }),
            ..Default::default()
        }
    }

    /// Returns the current keyboard keyboard state. We don't want the library
    /// user to be able to modify this state, only to read it.
    pub fn get_mouse_state(&self) -> &MouseState {
        &self.mouse_state
    }

    /// Returns the current windows mouse state. We don't want the library
    /// user to be able to modify this state, only to read it.
    pub fn get_keyboard_state(&self) -> &KeyboardState {
        &self.keyboard_state
    }

    /// Returns the physical (width, height) in pixel of this window
    pub fn get_physical_size(&self) -> (usize, usize) {
        (
            self.size.dimensions.width as usize,
            self.size.dimensions.height as usize,
        )
    }

    /// Returns the current HiDPI factor for this window.
    pub fn get_hidpi_factor(&self) -> f32 {
        self.size.get_hidpi_factor()
    }
}

impl Default for WindowState {
    fn default() -> Self {
        FullWindowState::default().into()
    }
}

/// --- menu.rs ---



impl StyledDom {
        /// Inject a menu bar into the root component
    pub fn inject_menu_bar(mut self, menu_bar: &Menu) -> Self {
        use azul_css::parser2::CssApiWrapper;

        use crate::window::MenuItem;

        let menu_dom = menu_bar
            .items
            .as_ref()
            .iter()
            .map(|mi| match mi {
                MenuItem::String(smi) => Dom::text(smi.label.clone().into_library_owned_string())
                    .with_inline_style("font-family:sans-serif;".into()),
                MenuItem::Separator => {
                    Dom::div().with_inline_style("padding:1px;background:grey;".into())
                }
                MenuItem::BreakLine => Dom::div(),
            })
            .collect::<Dom>()
            .with_inline_style(
                "
            height:20px;
            display:flex;
            flex-direction:row;"
                    .into(),
            )
            .style(CssApiWrapper::empty());

        let mut core_container = Dom::body().style(CssApiWrapper::empty());
        core_container.append_child(menu_dom);
        core_container.append_child(self);
        core_container
    }
    pub fn set_menu_bar(&mut self, menu: Menu) {
        if let Some(root) = self.root.into_crate_internal() {
            self.node_data.as_mut()[root.index()].set_menu_bar(menu)
        }
    }

    pub fn set_context_menu(&mut self, menu: Menu) {
        if let Some(root) = self.root.into_crate_internal() {
            self.node_data.as_mut()[root.index()].set_context_menu(menu);

            // add a new hit-testing tag for root node
            let mut new_tags = self.tag_ids_to_node_ids.clone().into_library_owned_vec();

            let tag_id = match self.styled_nodes.as_mut()[root.index()].tag_id {
                OptionTagId::Some(s) => s,
                OptionTagId::None => AzTagId::from_crate_internal(TagId::unique()),
            };

            new_tags.push(TagIdToNodeIdMapping {
                tag_id,
                node_id: self.root,
                tab_index: OptionTabIndex::None,
                parent_node_ids: NodeIdVec::from_const_slice(&[]),
            });

            self.styled_nodes.as_mut()[root.index()].tag_id = OptionTagId::Some(tag_id);
            self.tag_ids_to_node_ids = new_tags.into();
        }
    }
}

/// Position of where the menu should popup on the screen
///
/// Ignored for application-level menus
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Hash, Eq, Ord)]
#[repr(C)]
pub enum MenuPopupPosition {
    // relative to cursor
    BottomLeftOfCursor,
    BottomRightOfCursor,
    TopLeftOfCursor,
    TopRightOfCursor,

    // relative to the rect that was clicked on
    BottomOfHitRect,
    LeftOfHitRect,
    TopOfHitRect,
    RightOfHitRect,

    // calculate the position based on how much space
    // is available for the context menu to either side
    // of the screen
    AutoCursor,
    AutoHitRect,
}

impl Default for MenuPopupPosition {
    fn default() -> Self {
        Self::AutoCursor
    }
}

impl Menu {
    pub fn get_hash(&self) -> u64 {
        use highway::{HighwayHash, HighwayHasher, Key};
        let mut hasher = HighwayHasher::new(Key([0; 4]));
        self.hash(&mut hasher);
        hasher.finalize64()
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Hash, Eq, Ord)]
#[repr(C, u8)]
pub enum MenuItem {
    /// Regular menu item
    String(StringMenuItem),
    /// Separator line, only rendered when the direction is vertical
    Separator,
    /// Breaks the menu item into separate lines if laid out horizontally
    BreakLine,
}

impl_vec!(MenuItem, MenuItemVec, MenuItemVecDestructor);
impl_vec_clone!(MenuItem, MenuItemVec, MenuItemVecDestructor);
impl_vec_debug!(MenuItem, MenuItemVec);
impl_vec_partialeq!(MenuItem, MenuItemVec);
impl_vec_partialord!(MenuItem, MenuItemVec);
impl_vec_hash!(MenuItem, MenuItemVec);
impl_vec_eq!(MenuItem, MenuItemVec);
impl_vec_ord!(MenuItem, MenuItemVec);

#[derive(Debug, Clone, PartialEq, PartialOrd, Hash, Eq, Ord)]
#[repr(C)]
pub struct StringMenuItem {
    /// Label of the menu
    pub label: AzString,
    /// Optional accelerator combination
    /// (ex. "CTRL + X" = [VirtualKeyCode::Ctrl, VirtualKeyCode::X]) for keyboard shortcut
    pub accelerator: OptionVirtualKeyCodeCombo,
    /// Optional callback to call
    pub callback: OptionMenuCallback,
    /// State (normal, greyed, disabled)
    pub state: MenuItemState,
    /// Optional icon for the menu entry
    pub icon: OptionMenuItemIcon,
    /// Sub-menus of this item (separators and line-breaks can't have sub-menus)
    pub children: MenuItemVec,
}

impl StringMenuItem {
    pub fn new(label: AzString) -> Self {
        StringMenuItem {
            label,
            accelerator: None.into(),
            callback: None.into(),
            state: MenuItemState::Normal,
            icon: None.into(),
            children: MenuItemVec::from_const_slice(&[]),
        }
    }

    pub fn swap_with_default(&mut self) -> Self {
        let mut default = Self {
            label: AzString::from_const_str(""),
            accelerator: None.into(),
            callback: None.into(),
            state: MenuItemState::Normal,
            icon: None.into(),
            children: Vec::new().into(),
        };
        core::mem::swap(&mut default, self);
        default
    }

    pub fn with_children(mut self, children: MenuItemVec) -> Self {
        self.children = children;
        self
    }

    pub fn with_callback(mut self, data: RefAny, callback: CallbackType) -> Self {
        self.callback = Some(MenuCallback {
            data,
            callback: Callback { cb: callback },
        })
        .into();
        self
    }
}


/// Menu callback: What data / function pointer should
/// be called when the menu item is clicked?
#[derive(Debug, Clone, PartialEq, PartialOrd, Hash, Eq, Ord)]
#[repr(C)]
pub struct MenuCallback {
    pub callback: Callback,
    pub data: RefAny,
}

impl_option!(
    MenuCallback,
    OptionMenuCallback,
    copy = false,
    [Debug, Clone, PartialEq, PartialOrd, Hash, Eq, Ord]
);



/// Menu callback: What data / function pointer should
/// be called when the menu item is clicked?
#[derive(Debug, Clone, PartialEq, PartialOrd, Hash, Eq, Ord)]
#[repr(C)]
pub struct MenuCallback {
    pub callback: Callback,
    pub data: RefAny,
}

impl_option!(
    MenuCallback,
    OptionMenuCallback,
    copy = false,
    [Debug, Clone, PartialEq, PartialOrd, Hash, Eq, Ord]
);

/// --- NOTE: This is already replaced by the LayoutWindow::resolve_focus_target() function ---
impl FocusTarget {
    pub fn resolve(
        &self,
        layout_results: &[LayoutResult],
        current_focus: Option<DomNodeId>,
    ) -> Result<Option<DomNodeId>, UpdateFocusWarning> {
        use crate::{callbacks::FocusTarget::*, style::matches_html_element};

        if layout_results.is_empty() {
            return Ok(None);
        }

        macro_rules! search_for_focusable_node_id {
            (
                $layout_results:expr,
                $start_dom_id:expr,
                $start_node_id:expr,
                $get_next_node_fn:ident
            ) => {{
                let mut start_dom_id = $start_dom_id;
                let mut start_node_id = $start_node_id;

                let min_dom_id = DomId::ROOT_ID;
                let max_dom_id = DomId {
                    inner: layout_results.len() - 1,
                };

                // iterate through all DOMs
                loop {
                    // 'outer_dom_iter

                    let layout_result = $layout_results
                        .get(start_dom_id.inner)
                        .ok_or(UpdateFocusWarning::FocusInvalidDomId(start_dom_id.clone()))?;

                    let node_id_valid = layout_result
                        .styled_dom
                        .node_data
                        .as_container()
                        .get(start_node_id)
                        .is_some();

                    if !node_id_valid {
                        return Err(UpdateFocusWarning::FocusInvalidNodeId(
                            NodeHierarchyItemId::from_crate_internal(Some(start_node_id.clone())),
                        ));
                    }

                    if layout_result.styled_dom.node_data.is_empty() {
                        return Err(UpdateFocusWarning::FocusInvalidDomId(start_dom_id.clone()));
                        // ???
                    }

                    let max_node_id = NodeId::new(layout_result.styled_dom.node_data.len() - 1);
                    let min_node_id = NodeId::ZERO;

                    // iterate through nodes in DOM
                    loop {
                        let current_node_id =
                            NodeId::new(start_node_id.index().$get_next_node_fn(1))
                                .max(min_node_id)
                                .min(max_node_id);

                        if layout_result.styled_dom.node_data.as_container()[current_node_id]
                            .is_focusable()
                        {
                            return Ok(Some(DomNodeId {
                                dom: start_dom_id,
                                node: NodeHierarchyItemId::from_crate_internal(Some(
                                    current_node_id,
                                )),
                            }));
                        }

                        if current_node_id == min_node_id && current_node_id < start_node_id {
                            // going in decreasing (previous) direction
                            if start_dom_id == min_dom_id {
                                // root node / root dom encountered
                                return Ok(None);
                            } else {
                                start_dom_id.inner -= 1;
                                start_node_id = NodeId::new(
                                    $layout_results[start_dom_id.inner]
                                        .styled_dom
                                        .node_data
                                        .len()
                                        - 1,
                                );
                                break; // continue 'outer_dom_iter
                            }
                        } else if current_node_id == max_node_id && current_node_id > start_node_id
                        {
                            // going in increasing (next) direction
                            if start_dom_id == max_dom_id {
                                // last dom / last node encountered
                                return Ok(None);
                            } else {
                                start_dom_id.inner += 1;
                                start_node_id = NodeId::ZERO;
                                break; // continue 'outer_dom_iter
                            }
                        } else {
                            start_node_id = current_node_id;
                        }
                    }
                }
            }};
        }

        match self {
            Path(FocusTargetPath { dom, css_path }) => {
                let layout_result = layout_results
                    .get(dom.inner)
                    .ok_or(UpdateFocusWarning::FocusInvalidDomId(dom.clone()))?;
                let html_node_tree = &layout_result.styled_dom.cascade_info;
                let node_hierarchy = &layout_result.styled_dom.node_hierarchy;
                let node_data = &layout_result.styled_dom.node_data;
                let resolved_node_id = html_node_tree
                    .as_container()
                    .linear_iter()
                    .find(|node_id| {
                        matches_html_element(
                            css_path,
                            *node_id,
                            &node_hierarchy.as_container(),
                            &node_data.as_container(),
                            &html_node_tree.as_container(),
                            None,
                        )
                    })
                    .ok_or(UpdateFocusWarning::CouldNotFindFocusNode(css_path.clone()))?;
                Ok(Some(DomNodeId {
                    dom: dom.clone(),
                    node: NodeHierarchyItemId::from_crate_internal(Some(resolved_node_id)),
                }))
            }
            Id(dom_node_id) => {
                let layout_result = layout_results.get(dom_node_id.dom.inner).ok_or(
                    UpdateFocusWarning::FocusInvalidDomId(dom_node_id.dom.clone()),
                )?;
                let node_is_valid = dom_node_id
                    .node
                    .into_crate_internal()
                    .map(|o| {
                        layout_result
                            .styled_dom
                            .node_data
                            .as_container()
                            .get(o)
                            .is_some()
                    })
                    .unwrap_or(false);

                if !node_is_valid {
                    Err(UpdateFocusWarning::FocusInvalidNodeId(
                        dom_node_id.node.clone(),
                    ))
                } else {
                    Ok(Some(dom_node_id.clone()))
                }
            }
            Previous => {
                let last_layout_dom_id = DomId {
                    inner: layout_results.len() - 1,
                };

                // select the previous focusable element or `None`
                // if this was the first focusable element in the DOM
                let (current_focus_dom, current_focus_node_id) = match current_focus {
                    Some(s) => match s.node.into_crate_internal() {
                        Some(n) => (s.dom, n),
                        None => {
                            if let Some(layout_result) = layout_results.get(s.dom.inner) {
                                (
                                    s.dom,
                                    NodeId::new(layout_result.styled_dom.node_data.len() - 1),
                                )
                            } else {
                                (
                                    last_layout_dom_id,
                                    NodeId::new(
                                        layout_results[last_layout_dom_id.inner]
                                            .styled_dom
                                            .node_data
                                            .len()
                                            - 1,
                                    ),
                                )
                            }
                        }
                    },
                    None => (
                        last_layout_dom_id,
                        NodeId::new(
                            layout_results[last_layout_dom_id.inner]
                                .styled_dom
                                .node_data
                                .len()
                                - 1,
                        ),
                    ),
                };

                search_for_focusable_node_id!(
                    layout_results,
                    current_focus_dom,
                    current_focus_node_id,
                    saturating_sub
                );
            }
            Next => {
                // select the previous focusable element or `None`
                // if this was the first focusable element in the DOM, select the first focusable
                // element
                let (current_focus_dom, current_focus_node_id) = match current_focus {
                    Some(s) => match s.node.into_crate_internal() {
                        Some(n) => (s.dom, n),
                        None => {
                            if layout_results.get(s.dom.inner).is_some() {
                                (s.dom, NodeId::ZERO)
                            } else {
                                (DomId::ROOT_ID, NodeId::ZERO)
                            }
                        }
                    },
                    None => (DomId::ROOT_ID, NodeId::ZERO),
                };

                search_for_focusable_node_id!(
                    layout_results,
                    current_focus_dom,
                    current_focus_node_id,
                    saturating_add
                );
            }
            First => {
                let (current_focus_dom, current_focus_node_id) = (DomId::ROOT_ID, NodeId::ZERO);
                search_for_focusable_node_id!(
                    layout_results,
                    current_focus_dom,
                    current_focus_node_id,
                    saturating_add
                );
            }
            Last => {
                let last_layout_dom_id = DomId {
                    inner: layout_results.len() - 1,
                };
                let (current_focus_dom, current_focus_node_id) = (
                    last_layout_dom_id,
                    NodeId::new(
                        layout_results[last_layout_dom_id.inner]
                            .styled_dom
                            .node_data
                            .len()
                            - 1,
                    ),
                );
                search_for_focusable_node_id!(
                    layout_results,
                    current_focus_dom,
                    current_focus_node_id,
                    saturating_add
                );
            }
            NoFocus => Ok(None),
        }
    }
}

impl RendererResources {
        /// Updates the internal cache, adds `ResourceUpdate::Remove()`
    /// to the `all_resource_updates`
    ///
    /// This function will query all current images and fonts submitted
    /// into the cache and set them for the next frame so that unused
    /// resources will be cleaned up.
    ///
    /// This function should be called after the StyledDom has been
    /// exchanged for the next frame and AFTER all OpenGL textures
    /// and image callbacks have been resolved.
    pub fn do_gc(
        &mut self,
        all_resource_updates: &mut Vec<ResourceUpdate>,
        css_image_cache: &ImageCache,
        // layout calculated for the NEXT frame
        new_layout_results: &[LayoutResult],
        // initialized texture cache of the NEXT frame
        gl_texture_cache: &GlTextureCache,
    ) {
        use alloc::collections::btree_set::BTreeSet;

        // Get all fonts / images that are in the DOM for the next frame
        let mut next_frame_image_keys = BTreeSet::new();

        for layout_result in new_layout_results {
            for image_key in layout_result
                .styled_dom
                .scan_for_image_keys(css_image_cache)
            {
                let hash = image_ref_get_hash(image_key);
                next_frame_image_keys.insert(hash);
            }
        }

        for ((_dom_id, _node_id, _callback_imageref_hash), image_ref_hash) in
            gl_texture_cache.hashes.iter()
        {
            next_frame_image_keys.insert(*image_ref_hash);
        }

        // If the current frame contains a font key but the next frame doesn't, delete the font key
        let mut delete_font_resources = Vec::new();
        for (font_key, font_instances) in self.last_frame_registered_fonts.iter() {
            delete_font_resources.extend(
                font_instances
                    .iter()
                    .filter(|(au, _)| {
                        !(self
                            .currently_registered_fonts
                            .get(font_key)
                            .map(|f| f.1.contains_key(au))
                            .unwrap_or(false))
                    })
                    .map(|(au, font_instance_key)| {
                        (
                            font_key.clone(),
                            DeleteFontMsg::Instance(*font_instance_key, *au),
                        )
                    }),
            );
            // Delete the font and all instances if there are no more instances of the font
            // NOTE: deletion is in reverse order - instances are deleted first, then the font is
            // deleted
            if !self.currently_registered_fonts.contains_key(font_key) || font_instances.is_empty()
            {
                delete_font_resources
                    .push((font_key.clone(), DeleteFontMsg::Font(font_key.clone())));
            }
        }

        // If the current frame contains an image, but the next frame does not, delete it
        let delete_image_resources = self
            .currently_registered_images
            .iter()
            .filter(|(image_ref_hash, _)| !next_frame_image_keys.contains(image_ref_hash))
            .map(|(image_ref_hash, resolved_image)| {
                (
                    image_ref_hash.clone(),
                    DeleteImageMsg(resolved_image.key.clone()),
                )
            })
            .collect::<Vec<_>>();

        for (image_ref_hash_to_delete, _) in delete_image_resources.iter() {
            self.currently_registered_images
                .remove(image_ref_hash_to_delete);
        }

        all_resource_updates.extend(
            delete_font_resources
                .iter()
                .map(|(_, f)| f.into_resource_update()),
        );
        all_resource_updates.extend(
            delete_image_resources
                .iter()
                .map(|(_, i)| i.into_resource_update()),
        );

        self.last_frame_registered_fonts = self
            .currently_registered_fonts
            .iter()
            .map(|(fk, (_, fi))| (fk.clone(), fi.clone()))
            .collect();

        self.remove_font_families_with_zero_references();
    }


    // Re-invokes the RenderImageCallback on the given node (if there is any),
    // updates the internal texture (without exchanging the hashes, so that
    // the GC still works) and updates the internal texture cache.
    #[must_use]
    pub fn rerender_image_callback(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        document_id: DocumentId,
        epoch: Epoch,
        id_namespace: IdNamespace,
        gl_context: &OptionGlContextPtr,
        image_cache: &ImageCache,
        system_fonts: &FcFontCache,
        hidpi_factor: f32,
        callbacks: &RenderCallbacks,
        layout_results: &mut [LayoutResult],
        gl_texture_cache: &mut GlTextureCache,
    ) -> Option<UpdateImageResult> {
        use crate::{
            callbacks::{HidpiAdjustedBounds, RenderImageCallbackInfo},
            gl::{insert_into_active_gl_textures, remove_single_texture_from_active_gl_textures},
        };

        let mut layout_result = layout_results.get_mut(dom_id.inner)?;
        let mut node_data_vec = layout_result.styled_dom.node_data.as_container_mut();
        let mut node_data = node_data_vec.get_mut(node_id)?;
        let (mut render_image_callback, render_image_callback_hash) =
            node_data.get_render_image_callback_node()?;

        let callback_domnode_id = DomNodeId {
            dom: dom_id,
            node: NodeHierarchyItemId::from_crate_internal(Some(node_id)),
        };

        let rect_size = layout_result.rects.as_ref().get(node_id)?.size.clone();

        let size = LayoutSize::new(
            rect_size.width.round() as isize,
            rect_size.height.round() as isize,
        );

        // NOTE: all of these extra arguments are necessary so that the callback
        // has access to information about the text layout, which is used to render
        // the "text selection" highlight (the text selection is nothing but an image
        // or an image mask).
        let mut gl_callback_info = RenderImageCallbackInfo::new(
            /* gl_context: */ gl_context,
            /* image_cache: */ image_cache,
            /* system_fonts: */ system_fonts,
            /* node_hierarchy */ &layout_result.styled_dom.node_hierarchy,
            /* positioned_rects */ &layout_result.rects,
            /* bounds: */ HidpiAdjustedBounds::from_bounds(size, hidpi_factor),
            /* hit_dom_node */ callback_domnode_id,
        );

        let new_imageref = (render_image_callback.callback.cb)(
            &mut render_image_callback.data,
            &mut gl_callback_info,
        );

        // remove old imageref from GlTextureCache and active textures
        let existing_image_key = gl_texture_cache
            .solved_textures
            .get(&dom_id)
            .and_then(|m| m.get(&node_id))
            .map(|k| k.0.clone())
            .or(self
                .currently_registered_images
                .get(&render_image_callback_hash)
                .map(|i| i.key.clone()))?;

        if let Some(dom_map) = gl_texture_cache.solved_textures.get_mut(&dom_id) {
            if let Some((image_key, image_descriptor, external_image_id)) = dom_map.remove(&node_id)
            {
                remove_single_texture_from_active_gl_textures(
                    &document_id,
                    &epoch,
                    &external_image_id,
                );
            }
        }

        match new_imageref.into_inner()? {
            DecodedImage::Gl(new_tex) => {
                // for GL textures, generate a new external image ID
                let new_descriptor = new_tex.get_descriptor();
                let new_external_id = insert_into_active_gl_textures(document_id, epoch, new_tex);
                let new_image_data = ImageData::External(ExternalImageData {
                    id: new_external_id,
                    channel_index: 0,
                    image_type: ExternalImageType::TextureHandle(ImageBufferKind::Texture2D),
                });

                gl_texture_cache
                    .solved_textures
                    .entry(dom_id)
                    .or_insert_with(|| BTreeMap::new())
                    .insert(
                        node_id,
                        (existing_image_key, new_descriptor.clone(), new_external_id),
                    );

                Some(UpdateImageResult {
                    key_to_update: existing_image_key,
                    new_descriptor,
                    new_image_data,
                })
            }
            DecodedImage::Raw((descriptor, data)) => {
                if let Some(existing_image) = self
                    .currently_registered_images
                    .get_mut(&render_image_callback_hash)
                {
                    existing_image.descriptor = descriptor.clone(); // update descriptor, key stays the same
                    Some(UpdateImageResult {
                        key_to_update: existing_image_key,
                        new_descriptor: descriptor,
                        new_image_data: data,
                    })
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    // Updates images and image mask resources
    // NOTE: assumes the GL context is made current
    #[must_use]
    pub fn update_image_resources(
        &mut self,
        layout_results: &[LayoutResult],
        images_to_update: BTreeMap<DomId, BTreeMap<NodeId, (ImageRef, UpdateImageType)>>,
        image_masks_to_update: BTreeMap<DomId, BTreeMap<NodeId, ImageMask>>,
        callbacks: &RenderCallbacks,
        image_cache: &ImageCache,
        gl_texture_cache: &mut GlTextureCache,
        document_id: DocumentId,
        epoch: Epoch,
    ) -> Vec<UpdateImageResult> {
        use crate::dom::NodeType;

        let mut updated_images = Vec::new();
        let mut renderer_resources: &mut RendererResources = self;

        // update images
        for (dom_id, image_map) in images_to_update {
            let layout_result = match layout_results.get(dom_id.inner) {
                Some(s) => s,
                None => continue,
            };

            for (node_id, (image_ref, image_type)) in image_map {
                // get the existing key + extents of the image
                let existing_image_ref_hash = match image_type {
                    UpdateImageType::Content => {
                        match layout_result
                            .styled_dom
                            .node_data
                            .as_container()
                            .get(node_id)
                            .map(|n| n.get_node_type())
                        {
                            Some(NodeType::Image(image_ref)) => image_ref_get_hash(&image_ref),
                            _ => continue,
                        }
                    }
                    UpdateImageType::Background => {
                        let node_data = layout_result.styled_dom.node_data.as_container();
                        let node_data = match node_data.get(node_id) {
                            Some(s) => s,
                            None => continue,
                        };

                        let styled_node_states =
                            layout_result.styled_dom.styled_nodes.as_container();
                        let node_state = match styled_node_states.get(node_id) {
                            Some(s) => s.state.clone(),
                            None => continue,
                        };

                        let default =
                            azul_css::props::style::StyleBackgroundContentVec::from_const_slice(&[]);

                        // TODO: only updates the first image background - usually not a problem
                        let bg_hash = layout_result
                            .styled_dom
                            .css_property_cache
                            .ptr
                            .get_background_content(node_data, &node_id, &node_state)
                            .and_then(|bg| {
                                bg.get_property()
                                    .unwrap_or(&default)
                                    .as_ref()
                                    .iter()
                                    .find_map(|b| match b {
                                        azul_css::props::style::StyleBackgroundContent::Image(
                                            id,
                                        ) => {
                                            let image_ref = image_cache.get_css_image_id(id)?;
                                            Some(image_ref_get_hash(&image_ref))
                                        }
                                        _ => None,
                                    })
                            });

                        match bg_hash {
                            Some(h) => h,
                            None => continue,
                        }
                    }
                };

                let new_image_ref_hash = image_ref_get_hash(&image_ref);

                let decoded_image = match image_ref.into_inner() {
                    Some(s) => s,
                    None => continue,
                };

                // Try getting the existing image key either
                // from the textures or from the renderer resources
                let existing_key = gl_texture_cache
                    .solved_textures
                    .get(&dom_id)
                    .and_then(|map| map.get(&node_id))
                    .map(|val| val.0);

                let existing_key = match existing_key {
                    Some(s) => Some(s),
                    None => renderer_resources
                        .get_image(&existing_image_ref_hash)
                        .map(|resolved_image| resolved_image.key),
                };

                let key = match existing_key {
                    Some(s) => s,
                    None => continue, /* updating an image requires at
                                       * least one image to be present */
                };

                let (descriptor, data) = match decoded_image {
                    DecodedImage::Gl(texture) => {
                        let descriptor = texture.get_descriptor();
                        let new_external_image_id = match gl_texture_cache.update_texture(
                            dom_id,
                            node_id,
                            document_id,
                            epoch,
                            texture,
                            callbacks,
                        ) {
                            Some(s) => s,
                            None => continue,
                        };

                        let data = ImageData::External(ExternalImageData {
                            id: new_external_image_id,
                            channel_index: 0,
                            image_type: ExternalImageType::TextureHandle(
                                ImageBufferKind::Texture2D,
                            ),
                        });

                        (descriptor, data)
                    }
                    DecodedImage::Raw((descriptor, data)) => {
                        // use the hash to get the existing image key
                        // TODO: may lead to problems when the same ImageRef is used more than once?
                        renderer_resources.update_image(&existing_image_ref_hash, descriptor);
                        (descriptor, data)
                    }
                    DecodedImage::NullImage { .. } => continue, // TODO: NULL image descriptor?
                    DecodedImage::Callback(callback) => {
                        // TODO: re-render image callbacks?
                        /*
                        let (key, descriptor) = match gl_texture_cache.solved_textures.get(&dom_id).and_then(|textures| textures.get(&node_id)) {
                            Some((k, d)) => (k, d),
                            None => continue,
                        };*/

                        continue;
                    }
                };

                // update the image descriptor in the renderer resources

                updated_images.push(UpdateImageResult {
                    key_to_update: key,
                    new_descriptor: descriptor,
                    new_image_data: data,
                });
            }
        }

        // TODO: update image masks
        for (dom_id, image_mask_map) in image_masks_to_update {}

        updated_images
    }
}

impl GlTextureCache {
        /// Invokes all ImageCallbacks with the sizes given by the LayoutResult
    /// and adds them to the `RendererResources`.
    pub fn new(
        layout_results: &mut [LayoutResult],
        gl_context: &OptionGlContextPtr,
        id_namespace: IdNamespace,
        document_id: &DocumentId,
        epoch: Epoch,
        hidpi_factor: f32,
        image_cache: &ImageCache,
        system_fonts: &FcFontCache,
        callbacks: &RenderCallbacks,
        all_resource_updates: &mut Vec<ResourceUpdate>,
        renderer_resources: &mut RendererResources,
    ) -> Self {
        use gl_context_loader::gl;

        use crate::{
            callbacks::{HidpiAdjustedBounds, RenderImageCallbackInfo},
            dom::NodeType,
            resources::{
                add_resources, AddImage, DecodedImage, ExternalImageData, ExternalImageType,
                ImageBufferKind, ImageData, ImageRef,
            },
        };

        let mut solved_image_callbacks = BTreeMap::new();

        // Now that the layout is done, render the OpenGL textures and add them to the RenderAPI
        for (dom_id, layout_result) in layout_results.iter_mut().enumerate() {
            for callback_node_id in layout_result.styled_dom.scan_for_gltexture_callbacks() {
                // Invoke OpenGL callback, render texture
                let rect_size = layout_result.rects.as_ref()[callback_node_id].size;

                let callback_image = {
                    let callback_domnode_id = DomNodeId {
                        dom: DomId { inner: dom_id },
                        node: NodeHierarchyItemId::from_crate_internal(Some(callback_node_id)),
                    };

                    let size = LayoutSize::new(
                        rect_size.width.round() as isize,
                        rect_size.height.round() as isize,
                    );

                    // NOTE: all of these extra arguments are necessary so that the callback
                    // has access to information about the text layout, which is used to render
                    // the "text selection" highlight (the text selection is nothing but an image
                    // or an image mask).
                    let mut gl_callback_info = RenderImageCallbackInfo::new(
                        /* gl_context: */ &gl_context,
                        /* image_cache: */ image_cache,
                        /* system_fonts: */ system_fonts,
                        /* node_hierarchy */ &layout_result.styled_dom.node_hierarchy,
                        /* positioned_rects */ &layout_result.rects,
                        /* bounds: */ HidpiAdjustedBounds::from_bounds(size, hidpi_factor),
                        /* hit_dom_node */ callback_domnode_id,
                    );

                    let callback_image: Option<(ImageRef, ImageRefHash)> = {
                        // get a MUTABLE reference to the RefAny inside of the DOM
                        let mut node_data_mut =
                            layout_result.styled_dom.node_data.as_container_mut();
                        match &mut node_data_mut[callback_node_id].node_type {
                            NodeType::Image(img) => {
                                let callback_imageref_hash = img.get_hash();

                                img.get_image_callback_mut().map(|gl_texture_callback| {
                                    (
                                        (gl_texture_callback.callback.cb)(
                                            &mut gl_texture_callback.data,
                                            &mut gl_callback_info,
                                        ),
                                        callback_imageref_hash,
                                    )
                                })
                            }
                            _ => None,
                        }
                    };

                    // Reset the framebuffer and SRGB color target to 0
                    if let Some(gl) = gl_context.as_ref() {
                        gl.bind_framebuffer(gl::FRAMEBUFFER, 0);
                        gl.disable(gl::FRAMEBUFFER_SRGB);
                        gl.disable(gl::MULTISAMPLE);
                    }

                    callback_image
                };

                if let Some((image_ref, callback_imageref_hash)) = callback_image {
                    solved_image_callbacks
                        .entry(layout_result.dom_id.clone())
                        .or_insert_with(|| BTreeMap::default())
                        .insert(callback_node_id, (callback_imageref_hash, image_ref));
                }
            }
        }

        let mut image_resource_updates = Vec::new();
        let mut gl_texture_cache = Self::empty();

        for (dom_id, image_refs) in solved_image_callbacks {
            for (node_id, (callback_imageref_hash, image_ref)) in image_refs {
                // callback_imageref_hash = the hash of the ImageRef::callback()
                // that is currently in the DOM
                //
                // image_ref_hash = the hash of the ImageRef::gl_texture() that was
                // returned by invoking the ImageRef::callback()

                let image_ref_hash = image_ref_get_hash(&image_ref);
                let image_data = match image_ref.into_inner() {
                    Some(s) => s,
                    None => continue,
                };

                let image_result = match image_data {
                    DecodedImage::Gl(texture) => {
                        let descriptor = texture.get_descriptor();
                        let key = ImageKey::unique(id_namespace);
                        let external_image_id = (callbacks.insert_into_active_gl_textures_fn)(
                            *document_id,
                            epoch,
                            texture,
                        );

                        gl_texture_cache
                            .solved_textures
                            .entry(dom_id.clone())
                            .or_insert_with(|| BTreeMap::new())
                            .insert(node_id, (key, descriptor, external_image_id));

                        gl_texture_cache
                            .hashes
                            .insert((dom_id, node_id, callback_imageref_hash), image_ref_hash);

                        Some((
                            image_ref_hash,
                            AddImageMsg(AddImage {
                                key,
                                data: ImageData::External(ExternalImageData {
                                    id: external_image_id,
                                    channel_index: 0,
                                    image_type: ExternalImageType::TextureHandle(
                                        ImageBufferKind::Texture2D,
                                    ),
                                }),
                                descriptor,
                                tiling: None,
                            }),
                        ))
                    }
                    DecodedImage::Raw((descriptor, data)) => {
                        let key = ImageKey::unique(id_namespace);
                        Some((
                            image_ref_hash,
                            AddImageMsg(AddImage {
                                key,
                                data,
                                descriptor,
                                tiling: None,
                            }),
                        ))
                    }
                    DecodedImage::NullImage {
                        width: _,
                        height: _,
                        format: _,
                        tag: _,
                    } => None,
                    // Texture callbacks inside of texture callbacks are not rendered
                    DecodedImage::Callback(_) => None,
                };

                if let Some((image_ref_hash, add_img_msg)) = image_result {
                    image_resource_updates.push((
                        callback_imageref_hash,
                        image_ref_hash,
                        add_img_msg,
                    ));
                }
            }
        }

        // Add the new rendered images to the RenderApi
        add_gl_resources(
            renderer_resources,
            all_resource_updates,
            image_resource_updates,
        );

        gl_texture_cache
    }
}

/// Inserts default On::Scroll and On::Tab handle for scroll-able
/// and tabindex-able nodes.
#[inline]
pub fn insert_default_system_callbacks(&mut self, config: DefaultCallbacksCfg) {
    use crate::{
        callbacks::Callback,
        dom::{CallbackData, EventFilter, FocusEventFilter, HoverEventFilter},
    };

    let scroll_refany = RefAny::new(DefaultScrollCallbackData {
        smooth_scroll: config.smooth_scroll,
    });

    for n in self.node_data.iter_mut() {
        // TODO: ScrollStart / ScrollEnd?
        if !n
            .callbacks
            .iter()
            .any(|cb| cb.event == EventFilter::Hover(HoverEventFilter::Scroll))
        {
            n.callbacks.push(CallbackData {
                event: EventFilter::Hover(HoverEventFilter::Scroll),
                data: scroll_refany.clone(),
                callback: Callback {
                    cb: default_on_scroll,
                },
            });
        }
    }

    if !config.enable_autotab {
        return;
    }

    let tab_data = RefAny::new(DefaultTabIndexCallbackData {});
    for focusable_node in self.tag_ids_to_node_ids.iter() {
        if focusable_node.tab_index.is_some() {
            let focusable_node_id = match focusable_node.node_id.into_crate_internal() {
                Some(s) => s,
                None => continue,
            };

            let mut node_data = &mut self.node_data.as_container_mut()[focusable_node_id];
            if !node_data
                .callbacks
                .iter()
                .any(|cb| cb.event == EventFilter::Focus(FocusEventFilter::VirtualKeyDown))
            {
                node_data.callbacks.push(CallbackData {
                    event: EventFilter::Focus(FocusEventFilter::VirtualKeyDown),
                    data: tab_data.clone(),
                    callback: Callback {
                        cb: default_on_tabindex,
                    },
                });
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DefaultCallbacksCfg {
    pub smooth_scroll: bool,
    pub enable_autotab: bool,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct DefaultScrollCallbackData {
    pub smooth_scroll: bool,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct DefaultTabIndexCallbackData {}

/// Default On::TabIndex event handler
extern "C" fn default_on_tabindex(data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    let mut data = match data.downcast_mut::<DefaultTabIndexCallbackData>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    Update::DoNothing
}
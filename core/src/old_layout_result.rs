pub struct LayoutResult {
    pub dom_id: DomId,
    pub parent_dom_id: Option<DomId>,
    pub styled_dom: StyledDom,
    pub root_size: LayoutSize,
    pub root_position: LayoutPoint,
    pub rects: NodeDataContainer<PositionedRectangle>,
    pub scrollable_nodes: ScrolledNodes,
    pub iframe_mapping: BTreeMap<NodeId, DomId>,
    pub gpu_value_cache: GpuValueCache,
    pub formatting_contexts: NodeDataContainer<FormattingContext>,
    pub intrinsic_sizes: NodeDataContainer<IntrinsicSizes>,
}

impl LayoutResult {
    // New method to create a default LayoutResult with essential fields
    pub fn new_minimal(
        dom_id: DomId,
        parent_dom_id: Option<DomId>,
        styled_dom: StyledDom,
        root_size: LayoutSize,
        root_position: LayoutPoint,
        rects: NodeDataContainer<PositionedRectangle>,
        formatting_contexts: NodeDataContainer<FormattingContext>,
        intrinsic_sizes: NodeDataContainer<IntrinsicSizes>,
    ) -> Self {
        LayoutResult {
            dom_id,
            parent_dom_id,
            styled_dom,
            root_size,
            root_position,
            rects,
            formatting_contexts,
            intrinsic_sizes,
            scrollable_nodes: Default::default(),
            iframe_mapping: BTreeMap::new(),
            gpu_value_cache: Default::default(),
        }
    }

    pub fn print_layout_rects(&self, use_static_offset: bool) -> String {
        let mut output = String::new();

        // Start with the root node
        let root_node_id = self
            .styled_dom
            .root
            .into_crate_internal()
            .unwrap_or(NodeId::ZERO);
        self.print_rect_recursive(root_node_id, use_static_offset, 0, &mut output);

        output
    }

    fn print_rect_recursive(
        &self,
        node_id: NodeId,
        use_static_offset: bool,
        indent: usize,
        output: &mut String,
    ) {
        let indent_str = " ".repeat(indent);
        let rect = &self.rects.as_ref()[node_id];
        let node_hierarchy = self.styled_dom.node_hierarchy.as_container();

        // Get position
        let position = if use_static_offset {
            rect.get_logical_static_offset()
        } else {
            rect.get_logical_relative_offset()
        };

        // Print basic rect info
        output.push_str(&format!(
            "{}- {}: {}x{} @ ({},{})",
            indent_str,
            node_id.index(),
            rect.size.width as i32,
            rect.size.height as i32,
            position.x as i32,
            position.y as i32
        ));

        // Print margin if non-zero
        if rect.margin.top != 0.0
            || rect.margin.right != 0.0
            || rect.margin.bottom != 0.0
            || rect.margin.left != 0.0
        {
            output.push_str(&format!(
                " (margin {}px {}px {}px {}px)",
                rect.margin.top as i32,
                rect.margin.right as i32,
                rect.margin.bottom as i32,
                rect.margin.left as i32
            ));
        }

        // Print padding if non-zero
        if rect.padding.top != 0.0
            || rect.padding.right != 0.0
            || rect.padding.bottom != 0.0
            || rect.padding.left != 0.0
        {
            output.push_str(&format!(
                " (padding {}px {}px {}px {}px)",
                rect.padding.top as i32,
                rect.padding.right as i32,
                rect.padding.bottom as i32,
                rect.padding.left as i32
            ));
        }

        // Print border if non-zero
        if rect.border_widths.top != 0.0
            || rect.border_widths.right != 0.0
            || rect.border_widths.bottom != 0.0
            || rect.border_widths.left != 0.0
        {
            output.push_str(&format!(
                " (border {}px {}px {}px {}px)",
                rect.border_widths.top as i32,
                rect.border_widths.right as i32,
                rect.border_widths.bottom as i32,
                rect.border_widths.left as i32
            ));
        }

        // Print text lines if present
        if let Some((_, ref inline_text_layout)) = rect.resolved_text_layout_options {
            output.push('\n');

            for (i, line) in inline_text_layout.lines.as_ref().iter().enumerate() {
                // TODO: Try to get text content for this line
                let line_text = String::new();

                output.push_str(&format!(
                    "{}   - line {}: {}{}x{} @ ({},{})",
                    indent_str,
                    i,
                    line_text,
                    line.bounds.size.width as i32,
                    line.bounds.size.height as i32,
                    line.bounds.origin.x as i32,
                    line.bounds.origin.y as i32
                ));
                output.push_str("\n");
            }
        } else {
            output.push_str("\n");
        }

        // Recurse for all children
        for child_id in node_id.az_children(&node_hierarchy) {
            self.print_rect_recursive(child_id, use_static_offset, indent + 3, output);
        }
    }
}

impl fmt::Debug for LayoutResult {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("LayoutResult")
            .field("dom_id", &self.dom_id)
            .field("parent_dom_id", &self.parent_dom_id)
            .field("root_size", &self.root_size)
            .field("root_position", &self.root_position)
            .field("styled_dom_len", &self.styled_dom.node_hierarchy.len())
            .field(
                "styled_dom",
                &self
                    .styled_dom
                    .get_html_string("", "", true)
                    .lines()
                    .collect::<Vec<_>>(),
            )
            .field("formatting_contexts", &self.formatting_contexts)
            .field("intrinsic_sizes", &self.intrinsic_sizes)
            .field("rects", &self.rects)
            .field("gpu_value_cache", &self.gpu_value_cache)
            .finish()
    }
}


impl LayoutResult {
    pub fn get_bounds(&self) -> LayoutRect {
        LayoutRect::new(self.root_position, self.root_size)
    }

    // NOTE: get_cached_display_list has been removed.
    // Display list generation is now handled by azul_layout::LayoutWindow.
    // Use azul_layout::LayoutWindow::layout_and_generate_display_list() instead.

    // NOTE: do_quick_resize has been removed.
    // Window resizing with layout is now handled by azul_layout::LayoutWindow.
    // Use azul_layout::LayoutWindow::resize_window() instead.

    pub fn resize_images(
        id_namespace: IdNamespace,
        document_id: DocumentId,
        epoch: Epoch,
        dom_id: DomId,
        image_cache: &ImageCache,
        gl_context: &OptionGlContextPtr,
        layout_results: &mut [LayoutResult],
        gl_texture_cache: &mut GlTextureCache,
        renderer_resources: &mut RendererResources,
        callbacks: &RenderCallbacks,
        relayout_fn: RelayoutFn,
        fc_cache: &FcFontCache,
        window_size: &WindowSize,
        window_theme: WindowTheme,
        rsn: &BTreeMap<DomId, Vec<NodeId>>,
    ) -> Vec<UpdateImageResult> {
        let mut updated_images = Vec::new();

        for (dom_id, node_ids) in rsn.iter() {
            for node_id in node_ids.iter() {
                if let Some(update) = renderer_resources.rerender_image_callback(
                    *dom_id,
                    *node_id,
                    document_id,
                    epoch,
                    id_namespace,
                    gl_context,
                    image_cache,
                    fc_cache,
                    window_size.get_hidpi_factor(),
                    callbacks,
                    layout_results,
                    gl_texture_cache,
                ) {
                    updated_images.push(update);
                }
            }
        }

        updated_images
    }

    // Calls the IFrame callbacks again if they are currently
    // scrolled out of bounds
    pub fn scroll_iframes(
        document_id: &DocumentId,
        dom_id: DomId,
        epoch: Epoch,
        layout_results: &[LayoutResult],
        full_window_state: &FullWindowState,
        gl_texture_cache: &GlTextureCache,
        renderer_resources: &RendererResources,
        image_cache: &ImageCache,
    ) {
        // TODO
    }
}



#[derive(Default, Debug, Clone, PartialEq, PartialOrd)]
pub struct RelayoutChanges {
    pub resized_nodes: Vec<NodeId>,
    pub gpu_key_changes: GpuEventChanges,
}

impl RelayoutChanges {
    pub const EMPTY: RelayoutChanges = RelayoutChanges {
        resized_nodes: Vec::new(),
        gpu_key_changes: GpuEventChanges {
            transform_key_changes: Vec::new(),
            opacity_key_changes: Vec::new(),
        },
    };

    pub fn empty() -> Self {
        Self::EMPTY.clone()
    }
}

/// Layout options that can impact the flow of word positions
#[derive(Debug, Clone, PartialEq, PartialOrd, Default)]
pub struct TextLayoutOptions {
    /// Font size (in pixels) that this text has been laid out with
    pub font_size_px: PixelValue,
    /// Multiplier for the line height, default to 1.0
    pub line_height: Option<f32>,
    /// Additional spacing between glyphs (in pixels)
    pub letter_spacing: Option<PixelValue>,
    /// Additional spacing between words (in pixels)
    pub word_spacing: Option<PixelValue>,
    /// How many spaces should a tab character emulate
    /// (multiplying value, i.e. `4.0` = one tab = 4 spaces)?
    pub tab_width: Option<f32>,
    /// Maximum width of the text (in pixels) - if the text is set to `overflow:visible`, set this
    /// to None.
    pub max_horizontal_width: Option<f32>,
    /// How many pixels of leading does the first line have? Note that this added onto to the
    /// holes, so for effects like `:first-letter`, use a hole instead of a leading.
    pub leading: Option<f32>,
    /// This is more important for inline text layout where items can punch "holes"
    /// into the text flow, for example an image that floats to the right.
    ///
    /// TODO: Currently unused!
    pub holes: Vec<LayoutRect>,
}

/// Same as `TextLayoutOptions`, but with the widths / heights of the `PixelValue`s
/// resolved to regular f32s (because `letter_spacing`, `word_spacing`, etc. may be %-based value)
#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct ResolvedTextLayoutOptions {
    /// Font size (in pixels) that this text has been laid out with
    pub font_size_px: f32,
    /// Multiplier for the line height, default to 1.0
    pub line_height: OptionF32,
    /// Additional spacing between glyphs (in pixels)
    pub letter_spacing: OptionF32,
    /// Additional spacing between words (in pixels)
    pub word_spacing: OptionF32,
    /// How many spaces should a tab character emulate
    /// (multiplying value, i.e. `4.0` = one tab = 4 spaces)?
    pub tab_width: OptionF32,
    /// Maximum width of the text (in pixels) - if the text is set to `overflow:visible`, set this
    /// to None.
    pub max_horizontal_width: OptionF32,
    /// How many pixels of leading does the first line have? Note that this added onto to the
    /// holes, so for effects like `:first-letter`, use a hole instead of a leading.
    pub leading: OptionF32,
    /// This is more important for inline text layout where items can punch "holes"
    /// into the text flow, for example an image that floats to the right.
    ///
    /// TODO: Currently unused!
    pub holes: LogicalRectVec,
    // Stop layout after y coordinate
    pub max_vertical_height: OptionF32,
    // Whether text can break lines
    pub can_break: bool,
    // Whether text can be hyphenated
    pub can_hyphenate: bool,
    // Custom hyphenation character
    pub hyphenation_character: OptionChar,
    // Force RTL or LTR (Mixed = auto-detect)
    pub is_rtl: ScriptType,
    // Text justification mode
    pub text_justify: OptionStyleTextAlign,
}

impl Default for ResolvedTextLayoutOptions {
    fn default() -> Self {
        Self {
            font_size_px: DEFAULT_FONT_SIZE_PX as f32,
            line_height: None.into(),
            letter_spacing: None.into(),
            word_spacing: None.into(),
            tab_width: None.into(),
            max_horizontal_width: None.into(),
            leading: None.into(),
            holes: Vec::new().into(),
            max_vertical_height: None.into(),
            can_break: true,
            can_hyphenate: true,
            hyphenation_character: Some('-' as u32).into(),
            is_rtl: ScriptType::default(),
            text_justify: None.into(),
        }
    }
}


// Struct to hold script information for text spans
#[derive(Debug, Default, Clone, Copy, PartialEq, PartialOrd)]
#[repr(C)]
pub struct TextScriptInfo {
    pub script: ScriptType,
    pub start: usize,
    pub end: usize,
}

// Define script types
#[derive(Debug, Default, Clone, Copy, PartialEq, PartialOrd)]
#[repr(C)]
pub enum ScriptType {
    #[default]
    Mixed,
    LTR,
    RTL,
}


impl_option!(
    ResolvedTextLayoutOptions,
    OptionResolvedTextLayoutOptions,
    copy = false,
    [Debug, Clone, PartialEq, PartialOrd]
);

#[derive(Debug, Default, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct ResolvedOffsets {
    pub top: f32,
    pub left: f32,
    pub right: f32,
    pub bottom: f32,
}

impl ResolvedOffsets {
    pub const fn zero() -> Self {
        Self {
            top: 0.0,
            left: 0.0,
            right: 0.0,
            bottom: 0.0,
        }
    }
    pub fn total_vertical(&self) -> f32 {
        self.top + self.bottom
    }
    pub fn total_horizontal(&self) -> f32 {
        self.left + self.right
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct PositionedRectangle {
    /// Outer bounds of the rectangle
    pub size: LogicalSize,
    /// How the rectangle should be positioned
    pub position: PositionInfo,
    /// Padding of the rectangle
    pub padding: ResolvedOffsets,
    /// Margin of the rectangle
    pub margin: ResolvedOffsets,
    /// Border widths of the rectangle
    pub border_widths: ResolvedOffsets,
    /// Widths of the box shadow(s), necessary to calculate clip rect
    pub box_shadow: StyleBoxShadowOffsets,
    /// Whether the borders are included in the size or not
    pub box_sizing: LayoutBoxSizing,
    /// Evaluated result of the overflow-x property
    pub overflow_x: LayoutOverflow,
    /// Evaluated result of the overflow-y property
    pub overflow_y: LayoutOverflow,
    // TODO: box_shadow_widths
    /// If this is an inline rectangle, resolve the %-based font sizes
    /// and store them here.
    pub resolved_text_layout_options: Option<(ResolvedTextLayoutOptions, InlineTextLayout)>,
}

impl Default for PositionedRectangle {
    fn default() -> Self {
        PositionedRectangle {
            size: LogicalSize::zero(),
            overflow_x: LayoutOverflow::default(),
            overflow_y: LayoutOverflow::default(),
            position: PositionInfo::Static(PositionInfoInner {
                x_offset: 0.0,
                y_offset: 0.0,
                static_x_offset: 0.0,
                static_y_offset: 0.0,
            }),
            padding: ResolvedOffsets::zero(),
            margin: ResolvedOffsets::zero(),
            border_widths: ResolvedOffsets::zero(),
            box_shadow: StyleBoxShadowOffsets::default(),
            box_sizing: LayoutBoxSizing::default(),
            resolved_text_layout_options: None,
        }
    }
}

impl PositionedRectangle {
    #[inline]
    pub fn get_approximate_static_bounds(&self) -> LayoutRect {
        LayoutRect::new(self.get_static_offset(), self.get_content_size())
    }

    // Returns the rect where the content should be placed (for example the text itself)
    #[inline]
    fn get_content_size(&self) -> LayoutSize {
        LayoutSize::new(
            libm::roundf(self.size.width) as isize,
            libm::roundf(self.size.height) as isize,
        )
    }

    #[inline]
    fn get_logical_static_offset(&self) -> LogicalPosition {
        match self.position {
            PositionInfo::Static(p)
            | PositionInfo::Fixed(p)
            | PositionInfo::Absolute(p)
            | PositionInfo::Relative(p) => {
                LogicalPosition::new(p.static_x_offset, p.static_y_offset)
            }
        }
    }

    #[inline]
    fn get_logical_relative_offset(&self) -> LogicalPosition {
        match self.position {
            PositionInfo::Static(p)
            | PositionInfo::Fixed(p)
            | PositionInfo::Absolute(p)
            | PositionInfo::Relative(p) => LogicalPosition::new(p.x_offset, p.y_offset),
        }
    }

    #[inline]
    fn get_static_offset(&self) -> LayoutPoint {
        match self.position {
            PositionInfo::Static(p)
            | PositionInfo::Fixed(p)
            | PositionInfo::Absolute(p)
            | PositionInfo::Relative(p) => LayoutPoint::new(
                libm::roundf(p.static_x_offset) as isize,
                libm::roundf(p.static_y_offset) as isize,
            ),
        }
    }

    // Returns the rect that includes bounds, expanded by the padding + the border widths
    #[inline]
    pub fn get_background_bounds(&self) -> (LogicalSize, PositionInfo) {
        use crate::ui_solver::PositionInfo::*;

        let b_size = LogicalSize {
            width: self.size.width
                + self.padding.total_horizontal()
                + self.border_widths.total_horizontal(),
            height: self.size.height
                + self.padding.total_vertical()
                + self.border_widths.total_vertical(),
        };

        let x_offset_add = 0.0 - self.padding.left - self.border_widths.left;
        let y_offset_add = 0.0 - self.padding.top - self.border_widths.top;

        let b_position = match self.position {
            Static(PositionInfoInner {
                x_offset,
                y_offset,
                static_x_offset,
                static_y_offset,
            }) => Static(PositionInfoInner {
                x_offset: x_offset + x_offset_add,
                y_offset: y_offset + y_offset_add,
                static_x_offset,
                static_y_offset,
            }),
            Fixed(PositionInfoInner {
                x_offset,
                y_offset,
                static_x_offset,
                static_y_offset,
            }) => Fixed(PositionInfoInner {
                x_offset: x_offset + x_offset_add,
                y_offset: y_offset + y_offset_add,
                static_x_offset,
                static_y_offset,
            }),
            Relative(PositionInfoInner {
                x_offset,
                y_offset,
                static_x_offset,
                static_y_offset,
            }) => Relative(PositionInfoInner {
                x_offset: x_offset + x_offset_add,
                y_offset: y_offset + y_offset_add,
                static_x_offset,
                static_y_offset,
            }),
            Absolute(PositionInfoInner {
                x_offset,
                y_offset,
                static_x_offset,
                static_y_offset,
            }) => Absolute(PositionInfoInner {
                x_offset: x_offset + x_offset_add,
                y_offset: y_offset + y_offset_add,
                static_x_offset,
                static_y_offset,
            }),
        };

        (b_size, b_position)
    }

    #[inline]
    pub fn get_margin_box_width(&self) -> f32 {
        self.size.width
            + self.padding.total_horizontal()
            + self.border_widths.total_horizontal()
            + self.margin.total_horizontal()
    }

    #[inline]
    pub fn get_margin_box_height(&self) -> f32 {
        self.size.height
            + self.padding.total_vertical()
            + self.border_widths.total_vertical()
            + self.margin.total_vertical()
    }

    #[inline]
    pub fn get_left_leading(&self) -> f32 {
        self.margin.left + self.padding.left + self.border_widths.left
    }

    #[inline]
    pub fn get_top_leading(&self) -> f32 {
        self.margin.top + self.padding.top + self.border_widths.top
    }
}

/// Style and layout changes
#[derive(Debug, Clone, PartialEq)]
pub struct StyleAndLayoutChanges {
    /// Changes that were made to style properties of nodes
    pub style_changes: Option<BTreeMap<DomId, RestyleNodes>>,
    /// Changes that were made to layout properties of nodes
    pub layout_changes: Option<BTreeMap<DomId, RelayoutNodes>>,
    /// Whether the focus has actually changed
    pub focus_change: Option<FocusChange>,
    /// Used to call `On::Resize` handlers
    pub nodes_that_changed_size: Option<BTreeMap<DomId, Vec<NodeId>>>,
    /// Changes to the text content
    pub nodes_that_changed_text_content: Option<BTreeMap<DomId, Vec<NodeId>>>,
    /// Changes to GPU-cached opacity / transform values
    pub gpu_key_changes: Option<BTreeMap<DomId, GpuEventChanges>>,
}

impl StyleAndLayoutChanges {
    /// Determines and immediately applies the changes to the layout results
    pub fn new(
        nodes: &NodesToCheck,
        layout_results: &mut [LayoutResult],
        image_cache: &ImageCache,
        renderer_resources: &mut RendererResources,
        window_size: LayoutSize,
        document_id: &DocumentId,
        css_changes: Option<&BTreeMap<DomId, BTreeMap<NodeId, Vec<CssProperty>>>>,
        word_changes: Option<&BTreeMap<DomId, BTreeMap<NodeId, AzString>>>,
        callbacks_new_focus: &Option<Option<DomNodeId>>,
        relayout_cb: RelayoutFn,
    ) -> StyleAndLayoutChanges {
        // immediately restyle the DOM to reflect the new :hover, :active and :focus nodes
        // and determine if the DOM needs a redraw or a relayout
        let mut style_changes = None;
        let mut layout_changes = None;

        let is_mouse_down = nodes.current_window_state_mouse_is_down;
        let nodes_that_changed_text_content = word_changes.and_then(|word_changes| {
            if word_changes.is_empty() {
                None
            } else {
                Some(
                    word_changes
                        .iter()
                        .map(|(dom_id, m)| (*dom_id, m.keys().cloned().collect()))
                        .collect(),
                )
            }
        });

        macro_rules! insert_props {
            ($dom_id:expr, $prop_map:expr) => {{
                let dom_id: DomId = $dom_id;
                for (node_id, prop_map) in $prop_map.into_iter() {
                    for changed_prop in prop_map.into_iter() {
                        let prop_key = changed_prop.previous_prop.get_type();
                        if prop_key.can_trigger_relayout() {
                            layout_changes
                                .get_or_insert_with(|| BTreeMap::new())
                                .entry(dom_id)
                                .or_insert_with(|| BTreeMap::new())
                                .entry(node_id)
                                .or_insert_with(|| Vec::new())
                                .push(changed_prop);
                        } else {
                            style_changes
                                .get_or_insert_with(|| BTreeMap::new())
                                .entry(dom_id)
                                .or_insert_with(|| BTreeMap::new())
                                .entry(node_id)
                                .or_insert_with(|| Vec::new())
                                .push(changed_prop);
                        }
                    }
                }
            }};
        }

        for (dom_id, onmouseenter_nodes) in nodes.onmouseenter_nodes.iter() {
            let layout_result = &mut layout_results[dom_id.inner];

            let keys = onmouseenter_nodes.keys().copied().collect::<Vec<_>>();
            let onmouseenter_nodes_hover_restyle_props = layout_result
                .styled_dom
                .restyle_nodes_hover(&keys, /* currently_hovered = */ true);
            let onmouseleave_nodes_active_restyle_props = layout_result
                .styled_dom
                .restyle_nodes_active(&keys, /* currently_active = */ is_mouse_down);

            insert_props!(*dom_id, onmouseenter_nodes_hover_restyle_props);
            insert_props!(*dom_id, onmouseleave_nodes_active_restyle_props);
        }

        for (dom_id, onmouseleave_nodes) in nodes.onmouseleave_nodes.iter() {
            let layout_result = &mut layout_results[dom_id.inner];
            let keys = onmouseleave_nodes.keys().copied().collect::<Vec<_>>();
            let onmouseleave_nodes_hover_restyle_props = layout_result
                .styled_dom
                .restyle_nodes_hover(&keys, /* currently_hovered = */ false);
            let onmouseleave_nodes_active_restyle_props = layout_result
                .styled_dom
                .restyle_nodes_active(&keys, /* currently_active = */ false);

            insert_props!(*dom_id, onmouseleave_nodes_hover_restyle_props);
            insert_props!(*dom_id, onmouseleave_nodes_active_restyle_props);
        }

        let new_focus_node = if let Some(new) = callbacks_new_focus.as_ref() {
            new
        } else {
            &nodes.new_focus_node
        };

        let focus_change = if nodes.old_focus_node != *new_focus_node {
            if let Some(DomNodeId { dom, node }) = nodes.old_focus_node.as_ref() {
                if let Some(node_id) = node.into_crate_internal() {
                    let layout_result = &mut layout_results[dom.inner];
                    let onfocus_leave_restyle_props = layout_result
                        .styled_dom
                        .restyle_nodes_focus(&[node_id], /* currently_focused = */ false);
                    let dom_id: DomId = *dom;
                    insert_props!(dom_id, onfocus_leave_restyle_props);
                }
            }

            if let Some(DomNodeId { dom, node }) = new_focus_node.as_ref() {
                if let Some(node_id) = node.into_crate_internal() {
                    let layout_result = &mut layout_results[dom.inner];
                    let onfocus_enter_restyle_props = layout_result
                        .styled_dom
                        .restyle_nodes_focus(&[node_id], /* currently_focused = */ true);
                    let dom_id: DomId = *dom;
                    insert_props!(dom_id, onfocus_enter_restyle_props);
                }
            }

            Some(FocusChange {
                old: nodes.old_focus_node,
                new: *new_focus_node,
            })
        } else {
            None
        };

        // restyle all the nodes according to the existing_changed_styles
        if let Some(css_changes) = css_changes {
            for (dom_id, existing_changes_map) in css_changes.iter() {
                let layout_result = &mut layout_results[dom_id.inner];
                let dom_id: DomId = *dom_id;
                for (node_id, changed_css_property_vec) in existing_changes_map.iter() {
                    let current_prop_changes = layout_result
                        .styled_dom
                        .restyle_user_property(node_id, &changed_css_property_vec);
                    insert_props!(dom_id, current_prop_changes);
                }
            }
        }

        let mut nodes_that_changed_size = None;
        let mut gpu_key_change_events = None;

        // recursively relayout if there are layout_changes or the window size has changed
        let window_was_resized = window_size != layout_results[DomId::ROOT_ID.inner].root_size;
        let need_root_relayout = layout_changes.is_some()
            || window_was_resized
            || nodes_that_changed_text_content.is_some();

        let mut doms_to_relayout = Vec::new();
        if need_root_relayout {
            doms_to_relayout.push(DomId::ROOT_ID);
        } else {
            // if no nodes were resized or styles changed,
            // still update the GPU-only properties
            for (dom_id, layout_result) in layout_results.iter_mut().enumerate() {
                let gpu_key_changes = layout_result
                    .gpu_value_cache
                    .synchronize(&layout_result.rects.as_ref(), &layout_result.styled_dom);

                if !gpu_key_changes.is_empty() {
                    gpu_key_change_events
                        .get_or_insert_with(|| BTreeMap::new())
                        .insert(DomId { inner: dom_id }, gpu_key_changes);
                }
            }
        }

        loop {
            let mut new_iframes_to_relayout = Vec::new();

            for dom_id in doms_to_relayout.drain(..) {
                let parent_rect = match layout_results[dom_id.inner].parent_dom_id.as_ref() {
                    None => LayoutRect::new(LayoutPoint::zero(), window_size),
                    Some(parent_dom_id) => {
                        let parent_layout_result = &layout_results[parent_dom_id.inner];
                        let parent_iframe_node_id = parent_layout_result
                            .iframe_mapping
                            .iter()
                            .find_map(|(k, v)| if *v == dom_id { Some(*k) } else { None })
                            .unwrap();
                        parent_layout_result.rects.as_ref()[parent_iframe_node_id]
                            .get_approximate_static_bounds()
                    }
                };

                let layout_changes = layout_changes.as_ref().and_then(|w| w.get(&dom_id));
                let word_changes = word_changes.and_then(|w| w.get(&dom_id));

                // TODO: avoid allocation
                let RelayoutChanges {
                    resized_nodes,
                    gpu_key_changes,
                } = (relayout_cb)(
                    dom_id,
                    parent_rect,
                    &mut layout_results[dom_id.inner],
                    image_cache,
                    renderer_resources,
                    document_id,
                    layout_changes,
                    word_changes,
                    &mut None, // no debug messages
                );

                if !gpu_key_changes.is_empty() {
                    gpu_key_change_events
                        .get_or_insert_with(|| BTreeMap::new())
                        .insert(dom_id, gpu_key_changes);
                }

                if !resized_nodes.is_empty() {
                    new_iframes_to_relayout.extend(
                        layout_results[dom_id.inner]
                            .iframe_mapping
                            .iter()
                            .filter_map(|(node_id, dom_id)| {
                                if resized_nodes.contains(node_id) {
                                    Some(dom_id)
                                } else {
                                    None
                                }
                            }),
                    );
                    nodes_that_changed_size
                        .get_or_insert_with(|| BTreeMap::new())
                        .insert(dom_id, resized_nodes);
                }
            }

            if new_iframes_to_relayout.is_empty() {
                break;
            } else {
                doms_to_relayout = new_iframes_to_relayout;
            }
        }

        StyleAndLayoutChanges {
            style_changes,
            layout_changes,
            nodes_that_changed_size,
            nodes_that_changed_text_content,
            focus_change,
            gpu_key_changes: gpu_key_change_events,
        }
    }

    pub fn did_resize_nodes(&self) -> bool {
        use azul_css::props::property::CssPropertyType;

        if let Some(l) = self.nodes_that_changed_size.as_ref() {
            if !l.is_empty() {
                return true;
            }
        }

        if let Some(l) = self.nodes_that_changed_text_content.as_ref() {
            if !l.is_empty() {
                return true;
            }
        }

        // check if any changed node is a CSS transform
        if let Some(s) = self.style_changes.as_ref() {
            for restyle_nodes in s.values() {
                for changed in restyle_nodes.values() {
                    for changed in changed.iter() {
                        if changed.current_prop.get_type() == CssPropertyType::Transform {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    // Note: this can be false in case that only opacity: / transform: properties changed!
    pub fn need_regenerate_display_list(&self) -> bool {
        if !self.nodes_that_changed_size.is_none() {
            return true;
        }
        if !self.nodes_that_changed_text_content.is_none() {
            return true;
        }
        if !self.need_redraw() {
            return false;
        }

        // is_gpu_only_property = is the changed CSS property an opacity /
        // transform / rotate property (which doesn't require to regenerate the display list)
        if let Some(style_changes) = self.style_changes.as_ref() {
            !(style_changes.iter().all(|(_, restyle_nodes)| {
                restyle_nodes.iter().all(|(_, changed_css_properties)| {
                    changed_css_properties.iter().all(|changed_prop| {
                        changed_prop.current_prop.get_type().is_gpu_only_property()
                    })
                })
            }))
        } else {
            false
        }
    }

    pub fn is_empty(&self) -> bool {
        self.style_changes.is_none()
            && self.layout_changes.is_none()
            && self.focus_change.is_none()
            && self.nodes_that_changed_size.is_none()
            && self.nodes_that_changed_text_content.is_none()
            && self.gpu_key_changes.is_none()
    }

    pub fn need_redraw(&self) -> bool {
        !(self.style_changes.is_none()
            && self.layout_changes.is_none()
            && self.nodes_that_changed_text_content.is_none()
            && self.nodes_that_changed_size.is_none())
    }
}

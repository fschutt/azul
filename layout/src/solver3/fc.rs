//! solver3/fc/mod.rs
//!
//! Formatting context managers for different CSS display types

use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use azul_core::{
    dom::{FormattingContext, NodeId, NodeType},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::{StyledDom, StyledNodeState},
};
use azul_css::{
    css::CssPropertyValue,
    props::{
        layout::{
            LayoutClear, LayoutDisplay, LayoutFloat, LayoutJustifyContent, LayoutOverflow,
            LayoutPosition, LayoutTextJustify, LayoutWritingMode,
        },
        property::CssProperty,
        style::{StyleHyphens, StyleTextAlign, StyleVerticalAlign},
    },
};
use taffy::{AvailableSpace, LayoutInput, Line, Size as TaffySize};

use crate::{
    solver3::{
        cascade::get_resolved_font_size,
        geometry::{BoxProps, EdgeSizes, IntrinsicSizes},
        getters::{get_display_property, get_style_properties, get_writing_mode},
        layout_tree::{LayoutNode, LayoutTree},
        positioning::get_position_type,
        scrollbar::ScrollbarInfo,
        sizing::extract_text_from_node,
        taffy_bridge, LayoutContext, LayoutError, Result,
    },
    text3::{
        self,
        cache::{
            ContentIndex, FontLoaderTrait, ImageSource, InlineContent, InlineImage, InlineShape,
            LayoutCache as TextLayoutCache, LayoutFragment, ObjectFit, ParsedFontTrait,
            SegmentAlignment, ShapeBoundary, ShapeDefinition, ShapedItem, Size, StyleProperties,
            StyledRun, UnifiedConstraints,
        },
    },
};

/// The CSS `overflow` property behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverflowBehavior {
    Visible,
    Hidden,
    Clip,
    Scroll,
    Auto,
}

impl OverflowBehavior {
    pub fn is_clipped(&self) -> bool {
        matches!(self, Self::Hidden | Self::Clip | Self::Scroll | Self::Auto)
    }

    pub fn is_scroll(&self) -> bool {
        matches!(self, Self::Scroll | Self::Auto)
    }
}

/// Input constraints for a layout function.
#[derive(Debug)]
pub struct LayoutConstraints<'a> {
    /// The available space for the content, excluding padding and borders.
    pub available_size: LogicalSize,
    /// The CSS writing-mode of the context.
    pub writing_mode: LayoutWritingMode,
    /// The state of the parent Block Formatting Context, if applicable.
    /// This is how state (like floats) is passed down.
    pub bfc_state: Option<&'a mut BfcState>,
    // Other properties like text-align would go here.
    pub text_align: TextAlign,
}

/// Manages all layout state for a single Block Formatting Context.
/// This struct is created by the BFC root and lives for the duration of its layout.
#[derive(Debug, Clone)]
pub struct BfcState {
    /// The current position for the next in-flow block element.
    pub pen: LogicalPosition,
    /// The state of all floated elements within this BFC.
    pub floats: FloatingContext,
    /// The state of margin collapsing within this BFC.
    pub margins: MarginCollapseContext,
}

impl BfcState {
    pub fn new() -> Self {
        Self {
            pen: LogicalPosition::zero(),
            floats: FloatingContext::default(),
            margins: MarginCollapseContext::default(),
        }
    }
}

/// Manages vertical margin collapsing within a BFC.
#[derive(Debug, Default, Clone)]
pub struct MarginCollapseContext {
    /// The bottom margin of the last in-flow, block-level element.
    /// Can be positive or negative.
    pub last_in_flow_margin_bottom: f32,
}

/// The result of laying out a formatting context.
#[derive(Debug, Default)]
pub struct LayoutOutput {
    /// The final positions of child nodes, relative to the container's content-box origin.
    pub positions: BTreeMap<usize, LogicalPosition>,
    /// The total size occupied by the content, which may exceed `available_size`.
    pub overflow_size: LogicalSize,
    /// The baseline of the context, if applicable, measured from the top of its content box.
    pub baseline: Option<f32>,
}

/// Text alignment options
#[derive(Debug, Clone, Copy, Default)]
pub enum TextAlign {
    #[default]
    Start,
    End,
    Center,
    Justify,
}

/// Represents a single floated element within a BFC.
#[derive(Debug, Clone, Copy)]
struct FloatBox {
    /// The type of float (Left or Right).
    kind: LayoutFloat,
    /// The rectangle occupied by the float's margin-box.
    rect: LogicalRect,
}

/// Manages the state of all floated elements within a Block Formatting Context.
#[derive(Debug, Default, Clone)]
pub struct FloatingContext {
    floats: Vec<FloatBox>,
}

impl FloatingContext {
    /// Finds the available space on the cross-axis for a line box at a given main-axis range.
    ///
    /// Returns a tuple of (`cross_start_offset`, `cross_end_offset`) relative to the
    /// BFC content box, defining the available space for an in-flow element.
    pub fn available_line_box_space(
        &self,
        main_start: f32,
        main_end: f32,
        bfc_cross_size: f32,
        wm: LayoutWritingMode,
    ) -> (f32, f32) {
        let mut available_cross_start = 0.0_f32;
        let mut available_cross_end = bfc_cross_size;

        for float in &self.floats {
            // Get the logical main-axis span of the existing float.
            let float_main_start = float.rect.origin.main(wm);
            let float_main_end = float_main_start + float.rect.size.main(wm);

            // Check for overlap on the main axis.
            if main_end > float_main_start && main_start < float_main_end {
                // The float overlaps with the main-axis range of the element we're placing.
                let float_cross_start = float.rect.origin.cross(wm);
                let float_cross_end = float_cross_start + float.rect.size.cross(wm);

                if float.kind == LayoutFloat::Left {
                    // "line-left", i.e., cross-start
                    available_cross_start = available_cross_start.max(float_cross_end);
                } else {
                    // Float::Right, i.e., cross-end
                    available_cross_end = available_cross_end.min(float_cross_start);
                }
            }
        }
        (available_cross_start, available_cross_end)
    }

    /// Returns the main-axis offset needed to be clear of floats of the given type.
    pub fn clearance_offset(
        &self,
        clear: LayoutClear,
        current_main_offset: f32,
        wm: LayoutWritingMode,
    ) -> f32 {
        let mut max_end_offset = 0.0_f32;

        let check_left = clear == LayoutClear::Left || clear == LayoutClear::Both;
        let check_right = clear == LayoutClear::Right || clear == LayoutClear::Both;

        for float in &self.floats {
            let should_clear_this_float = (check_left && float.kind == LayoutFloat::Left)
                || (check_right && float.kind == LayoutFloat::Right);

            if should_clear_this_float {
                let float_main_end = float.rect.origin.main(wm) + float.rect.size.main(wm);
                max_end_offset = max_end_offset.max(float_main_end);
            }
        }

        if max_end_offset > current_main_offset {
            max_end_offset
        } else {
            current_main_offset
        }
    }
}

/// Encapsulates all state needed to lay out a single Block Formatting Context.
struct BfcLayoutState {
    /// The current position for the next in-flow block element.
    pen: LogicalPosition,
    floats: FloatingContext,
    margins: MarginCollapseContext,
    /// The writing mode of the BFC root.
    writing_mode: LayoutWritingMode,
}

/// Result of a formatting context layout operation
#[derive(Debug, Default)]
pub struct LayoutResult {
    pub positions: Vec<(usize, LogicalPosition)>,
    pub overflow_size: Option<LogicalSize>,
    pub baseline_offset: f32,
}

fn translate_taffy_size(size: LogicalSize) -> TaffySize<Option<f32>> {
    TaffySize {
        width: Some(size.width),
        height: Some(size.height),
    }
}

/// Creates StyleProperties for a node, resolving inherited values by walking up the tree.
///
/// This function correctly implements CSS inheritance. Properties like `font-size`
/// are resolved by checking ancestors if not explicitly set on the current node.
fn get_style_properties_with_context<T: ParsedFontTrait>(
    tree: &LayoutTree<T>,
    styled_dom: &StyledDom,
    node_index: usize,
) -> Arc<StyleProperties> {
    let node = tree.get(node_index).unwrap();
    let dom_id = node.dom_node_id.unwrap();
    let node_data = &styled_dom.node_data.as_container()[dom_id];
    let node_state = &styled_dom.styled_nodes.as_container()[dom_id].state;
    let cache = &styled_dom.css_property_cache.ptr;

    // Resolve inherited properties by walking the tree
    let font_size = get_resolved_font_size(tree, styled_dom, node_index);

    // Get non-inherited properties directly
    let font_family_name = cache
        .get_font_family(node_data, &dom_id, node_state)
        .and_then(|v| v.get_property().cloned())
        .and_then(|v| v.get(0).map(|f| f.as_string()))
        .unwrap_or_else(|| "sans-serif".to_string());

    let color = cache
        .get_text_color(node_data, &dom_id, node_state)
        .and_then(|v| v.get_property().cloned())
        .map(|v| v.inner)
        .unwrap_or_default();

    let line_height = cache
        .get_line_height(node_data, &dom_id, node_state)
        .and_then(|v| v.get_property().cloned())
        .map(|v| v.inner.normalized() * font_size) // Resolve line-height against computed font-size
        .unwrap_or(font_size * 1.2);

    Arc::new(StyleProperties {
        font_selector: crate::text3::cache::FontSelector {
            family: font_family_name,
            weight: rust_fontconfig::FcWeight::Normal, // STUB
            style: crate::text3::cache::FontStyle::Normal,   // STUB
            unicode_ranges: Vec::new(),
        },
        font_size_px: font_size,
        color,
        line_height,
        ..Default::default()
    })
}

/// Main dispatcher for formatting context layout.
pub fn layout_formatting_context<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &mut LayoutContext<T, Q>,
    tree: &mut LayoutTree<T>,
    text_cache: &mut text3::cache::LayoutCache<T>,
    node_index: usize,
    constraints: &LayoutConstraints,
) -> Result<LayoutOutput> {
    let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;

    match node.formatting_context {
        FormattingContext::Block { .. } => {
            layout_bfc(ctx, tree, text_cache, node_index, constraints)
        }
        FormattingContext::Inline => layout_ifc(ctx, text_cache, tree, node_index, constraints),
        FormattingContext::Table => layout_table_fc(ctx, tree, text_cache, node_index, constraints),
        FormattingContext::Flex | FormattingContext::Grid => {
            let available_space = TaffySize {
                width: AvailableSpace::Definite(constraints.available_size.width),
                height: AvailableSpace::Definite(constraints.available_size.height),
            };

            let taffy_inputs = LayoutInput {
                known_dimensions: TaffySize::NONE,
                parent_size: translate_taffy_size(constraints.available_size),
                available_space,
                run_mode: taffy::RunMode::PerformLayout,
                // Sizing mode is ContentSize because solver3's `constraints.available_size`
                // represents the parent's content-box (inner size after padding/border).
                sizing_mode: taffy::SizingMode::ContentSize,
                // We are in the main layout pass, not a measurement pass. We need Taffy
                // to compute the final size and position for both axes.
                axis: taffy::RequestedAxis::Both,
                // Flex and Grid containers establish a new Block Formatting Context (BFC),
                // which prevents the margins of their children from collapsing with their own.
                vertical_margins_are_collapsible: Line::FALSE,
            };

            let taffy_output =
                taffy_bridge::layout_taffy_subtree(ctx, tree, node_index, taffy_inputs);

            // The bridge has already updated the positions and sizes of the children in the tree.
            // We just need to construct the LayoutOutput for the parent.
            let mut output = LayoutOutput::default();
            output.overflow_size = translate_taffy_size_back(taffy_output.size);

            // Taffy's results are stored directly on the nodes, so we read them back here.
            for &child_idx in &tree.get(node_index).unwrap().children {
                if let Some(child_node) = tree.get(child_idx) {
                    if let Some(pos) = child_node.relative_position {
                        output.positions.insert(child_idx, pos);
                    }
                }
            }

            Ok(output)
        }
        _ => layout_bfc(ctx, tree, text_cache, node_index, constraints),
    }
}

pub fn translate_taffy_size_back(size: TaffySize<f32>) -> LogicalSize {
    LogicalSize {
        width: size.width,
        height: size.height,
    }
}

pub fn translate_taffy_point_back(point: taffy::Point<f32>) -> LogicalPosition {
    LogicalPosition {
        x: point.x,
        y: point.y,
    }
}

/// Lays out a Block Formatting Context (BFC).
///
/// Lays out a Block Formatting Context (BFC).
///
/// This is the corrected, architecturally-sound implementation. It solves the
/// "chicken-and-egg" problem by performing its own two-pass layout:
///
/// 1. **Sizing Pass:** It first iterates through its children and triggers their layout recursively
///    by calling `calculate_layout_for_subtree`. This ensures that the `used_size` property of each
///    child is correctly populated.
///
/// 2. **Positioning Pass:** It then iterates through the children again. Now that each child has a
///    valid size, it can apply the standard block-flow logic: stacking them vertically and
///    advancing a "pen" by each child's outer height.
///
/// This approach is compliant with the CSS visual formatting model and works within
/// the constraints of the existing layout engine architecture.
fn layout_bfc<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &mut LayoutContext<T, Q>,
    tree: &mut LayoutTree<T>,
    text_cache: &mut text3::cache::LayoutCache<T>,
    node_index: usize,
    constraints: &LayoutConstraints,
) -> Result<LayoutOutput> {
    eprintln!("[layout_bfc] CALLED for node_index={}", node_index);
    
    let node = tree
        .get(node_index)
        .ok_or(LayoutError::InvalidTree)?
        .clone();
    let writing_mode = constraints.writing_mode;
    let mut output = LayoutOutput::default();

    // --- Pass 1: Sizing ---
    // We must first calculate the size of all child nodes before we can position them.
    // We do this by recursively calling the main layout function for each child.
    // This populates the `used_size` field on each child LayoutNode.
    for &child_index in &node.children {
        let child_node = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
        let child_dom_id = child_node.dom_node_id;

        // Skip out-of-flow children, as they don't affect the BFC's content size.
        let position_type = get_position_type(ctx.styled_dom, child_dom_id);
        if position_type == LayoutPosition::Absolute || position_type == LayoutPosition::Fixed {
            continue;
        }

        // We use a temporary, discarded position map to prevent this sizing pass
        // from polluting the final `calculated_positions` map with incorrect values.
        let mut temp_positions = BTreeMap::new();

        // The child's containing block is its parent's content box.
        // The position is a placeholder because we only care about the size calculation here.
        crate::solver3::cache::calculate_layout_for_subtree(
            ctx,
            tree,
            text_cache,
            child_index,
            LogicalPosition::zero(), // Placeholder position for sizing pass
            constraints.available_size, /* The parent's content-box size is the child's containing
                                      * block size */
            &mut temp_positions,
            &mut bool::default(), // Placeholder for scrollbar reflow detection
        )?;
    }

    // --- Pass 2: Positioning ---
    // Now that all children have a valid `used_size`, we can position them.
    let mut main_pen = 0.0f32;
    let mut max_cross_size = 0.0f32;

    eprintln!("[layout_bfc] Pass 2: Positioning {} children", node.children.len());

    for &child_index in &node.children {
        let child_node = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
        let child_dom_id = child_node.dom_node_id;

        let position_type = get_position_type(ctx.styled_dom, child_dom_id);
        if position_type == LayoutPosition::Absolute || position_type == LayoutPosition::Fixed {
            continue;
        }

        // Now this will be a valid, non-zero size.
        let child_size = child_node.used_size.unwrap_or_default();
        let child_margin = &child_node.box_props.margin;

        // 1. Advance the pen by the child's starting margin.
        main_pen += child_margin.main_start(writing_mode);

        // 2. Determine the child's position relative to the parent's content-box.
        let child_cross_pos = child_margin.cross_start(writing_mode);
        let child_main_pos = main_pen;

        let final_pos =
            LogicalPosition::from_main_cross(child_main_pos, child_cross_pos, writing_mode);
        
        eprintln!("[layout_bfc]   Child {}: main_pos={:.2}, cross_pos={:.2}, size={:?}", 
            child_index, child_main_pos, child_cross_pos, child_size);
        
        output.positions.insert(child_index, final_pos);

        // 3. Advance the pen past the child's content size and its ending margin.
        main_pen += child_size.main(writing_mode);
        main_pen += child_margin.main_end(writing_mode);

        // 4. Track the maximum cross-axis size to determine the BFC's overflow size.
        let child_cross_extent =
            child_cross_pos + child_size.cross(writing_mode) + child_margin.cross_end(writing_mode);
        max_cross_size = max_cross_size.max(child_cross_extent);
    }

    // The final overflow size is determined by the final pen position and the max cross size.
    output.overflow_size = LogicalSize::from_main_cross(main_pen, max_cross_size, writing_mode);

    // Baseline calculation would happen here in a full implementation.
    output.baseline = None;

    if let Some(node_mut) = tree.get_mut(node_index) {
        node_mut.baseline = output.baseline;
    }

    Ok(output)
}

/// Lays out an Inline Formatting Context (IFC) by delegating to the `text3` engine.
///
/// This function acts as a bridge between the box-tree world of `solver3` and the
/// rich text layout world of `text3`. Its responsibilities are:
///
/// 1. **Collect Content**: Traverse the direct children of the IFC root and convert them into a
///    `Vec<InlineContent>`, the input format for `text3`. This involves:
///     - Recursively laying out `inline-block` children to determine their final size and baseline,
///       which are then passed to `text3` as opaque objects.
///     - Extracting raw text runs from inline text nodes.
///
/// 2. **Translate Constraints**: Convert the `LayoutConstraints` (available space, floats) from
///    `solver3` into the more detailed `UnifiedConstraints` that `text3` requires.
///
/// 3. **Invoke Text Layout**: Call the `text3` cache's `layout_flow` method to perform the complex
///    tasks of BIDI analysis, shaping, line breaking, justification, and vertical alignment.
///
/// 4. **Integrate Results**: Process the `UnifiedLayout` returned by `text3`:
///     - Store the rich layout result on the IFC root `LayoutNode` for the display list generation
///       pass.
///     - Update the `positions` map for all `inline-block` children based on the positions
///       calculated by `text3`.
///     - Extract the final overflow size and baseline for the IFC root itself.
fn layout_ifc<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &mut LayoutContext<T, Q>,
    text_cache: &mut text3::cache::LayoutCache<T>,
    tree: &mut LayoutTree<T>,
    node_index: usize,
    constraints: &LayoutConstraints,
) -> Result<LayoutOutput> {
    eprintln!("[layout_ifc] CALLED for node_index={}", node_index);

    let ifc_root_dom_id = tree
        .get(node_index)
        .and_then(|n| n.dom_node_id)
        .ok_or(LayoutError::InvalidTree)?;

    eprintln!("[layout_ifc] ifc_root_dom_id={:?}", ifc_root_dom_id);

    // Phase 1: Collect and measure all inline-level children.
    let (inline_content, child_map) =
        collect_and_measure_inline_content(ctx, text_cache, tree, node_index)?;

    eprintln!(
        "[layout_ifc] Collected {} inline content items",
        inline_content.len()
    );

    if inline_content.is_empty() {
        eprintln!("[layout_ifc] WARNING: inline_content is empty, returning default output!");
        return Ok(LayoutOutput::default());
    }

    // Phase 2: Translate constraints and define a single layout fragment for text3.
    let text3_constraints =
        translate_to_text3_constraints(constraints, ctx.styled_dom, ifc_root_dom_id);
    let fragments = vec![LayoutFragment {
        id: "main".to_string(),
        constraints: text3_constraints,
    }];

    // Phase 3: Invoke the text layout engine.
    let text_layout_result =
        match text_cache.layout_flow(&inline_content, &[], &fragments, ctx.font_manager) {
            Ok(result) => result,
            Err(e) => {
                // Font errors should not stop layout of other elements.
                // Log the error and return a zero-sized layout.
                eprintln!("[layout_ifc] ⚠️  WARNING: Text layout failed: {:?}", e);
                eprintln!(
                    "[layout_ifc] ⚠️  Continuing with zero-sized layout for node {}",
                    node_index
                );

                let mut output = LayoutOutput::default();
                output.overflow_size = LogicalSize::new(0.0, 0.0);
                return Ok(output);
            }
        };

    // Phase 4: Integrate results back into the solver3 layout tree.
    let mut output = LayoutOutput::default();
    let node = tree.get_mut(node_index).ok_or(LayoutError::InvalidTree)?;

    eprintln!(
        "[layout_ifc] text_layout_result has {} fragment_layouts",
        text_layout_result.fragment_layouts.len()
    );

    if let Some(main_frag) = text_layout_result.fragment_layouts.get("main") {
        let frag_bounds = main_frag.bounds();
        eprintln!(
            "[layout_ifc] ✓ Found 'main' fragment with {} items, bounds={}x{}",
            main_frag.items.len(),
            frag_bounds.width,
            frag_bounds.height
        );
        eprintln!(
            "[layout_ifc] ✓ Storing inline_layout_result on node {}",
            node_index
        );

        // Store the detailed result for the display list generator.
        node.inline_layout_result = Some(main_frag.clone());

        // Extract the overall size and baseline for the IFC root.
        output.overflow_size = LogicalSize::new(frag_bounds.width, frag_bounds.height);
        output.baseline = main_frag.last_baseline();
        node.baseline = output.baseline;

        // Position all the inline-block children based on text3's calculations.
        for positioned_item in &main_frag.items {
            if let ShapedItem::Object { source, .. } = &positioned_item.item {
                if let Some(&child_node_index) = child_map.get(source) {
                    let new_relative_pos = LogicalPosition {
                        x: positioned_item.position.x,
                        y: positioned_item.position.y,
                    };
                    output.positions.insert(child_node_index, new_relative_pos);
                }
            }
        }
    }

    Ok(output)
}

/// Translates solver3 layout constraints into the text3 engine's unified constraints.
fn translate_to_text3_constraints<'a>(
    constraints: &'a LayoutConstraints<'a>,
    styled_dom: &StyledDom,
    dom_id: NodeId,
) -> UnifiedConstraints {
    use crate::text3::cache::TextAlign as Text3TextAlign;

    // Convert floats into exclusion zones for text3 to flow around.
    let shape_exclusions = if let Some(ref bfc_state) = constraints.bfc_state {
        bfc_state
            .floats
            .floats
            .iter()
            .map(|float_box| {
                ShapeBoundary::Rectangle(crate::text3::cache::Rect {
                    x: float_box.rect.origin.x,
                    y: float_box.rect.origin.y,
                    width: float_box.rect.size.width,
                    height: float_box.rect.size.height,
                })
            })
            .collect()
    } else {
        Vec::new()
    };

    // Map text-align and justify-content from CSS to text3 enums.
    let id = dom_id;
    let node_data = &styled_dom.node_data.as_container()[id];
    let node_state = &styled_dom.styled_nodes.as_container()[id].state;

    // TODO: support shape-outside, shape boundaries, flow-from, flow-into

    let writing_mode = styled_dom
        .css_property_cache
        .ptr
        .get_writing_mode(node_data, &id, node_state)
        .and_then(|s| s.get_property().copied())
        .unwrap_or_default();

    let text_align = styled_dom
        .css_property_cache
        .ptr
        .get_text_align(node_data, &id, node_state)
        .and_then(|s| s.get_property().copied())
        .unwrap_or_default();

    let text_justify = styled_dom
        .css_property_cache
        .ptr
        .get_text_justify(node_data, &id, node_state)
        .and_then(|s| s.get_property().copied())
        .unwrap_or_default();

    let line_height = styled_dom
        .css_property_cache
        .ptr
        .get_line_height(node_data, &id, node_state)
        .and_then(|s| s.get_property().cloned())
        .unwrap_or_default();

    let hyphenation = styled_dom
        .css_property_cache
        .ptr
        .get_hyphens(node_data, &id, node_state)
        .and_then(|s| s.get_property().copied())
        .unwrap_or_default();

    let overflow_behaviour = styled_dom
        .css_property_cache
        .ptr
        .get_overflow_x(node_data, &id, node_state)
        .and_then(|s| s.get_property().copied())
        .unwrap_or_default();

    // Note: vertical_align and text_orientation property getters not yet available, using defaults
    let vertical_align = StyleVerticalAlign::default();
    let text_orientation = text3::cache::TextOrientation::default();

    // TODO: Once StyleDirection is properly defined in CSS module, read from property cache:
    // let direction = styled_dom
    //     .css_property_cache
    //     .ptr
    //     .get_direction(node_data, &id, node_state)
    //     .and_then(|s| s.get_property().copied());
    // For now, default to LTR which is the HTML/CSS standard and fixes Arabic text BiDi issues
    let direction = Some(text3::cache::Direction::Ltr);

    eprintln!(
        "[translate_to_text3_constraints] dom_id={:?}, available_size={}x{}, setting available_width={}",
        dom_id,
        constraints.available_size.width,
        constraints.available_size.height,
        constraints.available_size.width
    );

    UnifiedConstraints {
        exclusion_margin: 0.0,                   // TODO: support -azul-exclusion-margin
        hyphenation_language: None,              // TODO: support -azul-hyphenation-language
        text_indent: 0.0,                        // TODO: support text-indent
        initial_letter: None,                    // TODO: support initial-letter
        line_clamp: None,                        // TODO: support line-clamp proprty
        columns: 1,                              // TODO: support multi-column layout
        column_gap: 0.0,                         // TODO: support column-gap
        hanging_punctuation: false,              // TODO: support hanging-punctuation
        text_wrap: text3::cache::TextWrap::Wrap, // TODO: map from CSS property
        text_combine_upright: None,              // TODO: text-combine-upright
        segment_alignment: SegmentAlignment::Total,
        overflow: match overflow_behaviour {
            LayoutOverflow::Visible => text3::cache::OverflowBehavior::Visible,
            LayoutOverflow::Hidden | LayoutOverflow::Clip => text3::cache::OverflowBehavior::Hidden,
            LayoutOverflow::Scroll => text3::cache::OverflowBehavior::Scroll,
            LayoutOverflow::Auto => text3::cache::OverflowBehavior::Auto,
        },
        available_width: constraints.available_size.width,
        available_height: Some(constraints.available_size.height),
        shape_boundaries: Vec::new(), // TODO: support shape-outside
        shape_exclusions,
        writing_mode: Some(match writing_mode {
            LayoutWritingMode::HorizontalTb => text3::cache::WritingMode::HorizontalTb,
            LayoutWritingMode::VerticalRl => text3::cache::WritingMode::VerticalRl,
            LayoutWritingMode::VerticalLr => text3::cache::WritingMode::VerticalLr,
        }),
        direction, // Use the CSS direction property (currently defaulting to LTR)
        hyphenation: match hyphenation {
            StyleHyphens::None => false,
            StyleHyphens::Auto => true,
        },
        text_orientation,
        text_align: match text_align {
            StyleTextAlign::Start => text3::cache::TextAlign::Start,
            StyleTextAlign::End => text3::cache::TextAlign::End,
            StyleTextAlign::Left => text3::cache::TextAlign::Left,
            StyleTextAlign::Right => text3::cache::TextAlign::Right,
            StyleTextAlign::Center => text3::cache::TextAlign::Center,
            StyleTextAlign::Justify => text3::cache::TextAlign::Justify,
        },
        text_justify: match text_justify {
            LayoutTextJustify::None => text3::cache::JustifyContent::None,
            LayoutTextJustify::Auto => text3::cache::JustifyContent::None,
            LayoutTextJustify::InterWord => text3::cache::JustifyContent::InterWord,
            LayoutTextJustify::InterCharacter => text3::cache::JustifyContent::InterCharacter,
            LayoutTextJustify::Distribute => text3::cache::JustifyContent::Distribute,
        },
        line_height: 16.0, // TODO: properly handle line_height CssPropertyValue
        vertical_align: match vertical_align {
            StyleVerticalAlign::Top => text3::cache::VerticalAlign::Top,
            StyleVerticalAlign::Center => text3::cache::VerticalAlign::Middle,
            StyleVerticalAlign::Bottom => text3::cache::VerticalAlign::Bottom,
        },
    }
}

/// Lays out a Table Formatting Context.
fn layout_table_fc<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &mut LayoutContext<T, Q>,
    tree: &mut LayoutTree<T>,
    text_cache: &mut text3::cache::LayoutCache<T>,
    node_index: usize,
    constraints: &LayoutConstraints,
) -> Result<LayoutOutput> {
    ctx.debug_log("Laying out table (STUB)");
    // A real implementation would be a multi-pass algorithm:
    // 1. Determine number of columns and create a grid structure.
    // 2. Calculate min/max content width for each cell.
    // 3. Resolve column widths based on table width and cell constraints.
    // 4. Layout cells within their final column widths to determine row heights.
    // 5. Position cells within the final grid.

    // For now, we fall back to simple block stacking.
    layout_bfc(ctx, tree, text_cache, node_index, constraints)
}

/// Gathers all inline content for `text3`, recursively laying out `inline-block` children
/// to determine their size and baseline before passing them to the text engine.
fn collect_and_measure_inline_content<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &mut LayoutContext<T, Q>,
    text_cache: &mut TextLayoutCache<T>,
    tree: &mut LayoutTree<T>,
    ifc_root_index: usize,
) -> Result<(Vec<InlineContent>, HashMap<ContentIndex, usize>)> {
    eprintln!(
        "[collect_and_measure_inline_content] CALLED for node_index={}",
        ifc_root_index
    );

    let mut content = Vec::new();
    // Maps the `ContentIndex` used by text3 back to the `LayoutNode` index.
    let mut child_map = HashMap::new();
    let ifc_root_node = tree.get(ifc_root_index).ok_or(LayoutError::InvalidTree)?;

    // Get the DOM node ID of the IFC root
    let Some(ifc_root_dom_id) = ifc_root_node.dom_node_id else {
        eprintln!("[collect_and_measure_inline_content] WARNING: IFC root has no DOM ID");
        return Ok((content, child_map));
    };

    // Collect children to avoid holding an immutable borrow during iteration
    let children: Vec<_> = ifc_root_node.children.clone();
    drop(ifc_root_node);

    eprintln!(
        "[collect_and_measure_inline_content] Node {} has {} layout children",
        ifc_root_index,
        children.len()
    );
    
    // Check if this IFC root OR its parent is a list-item and needs a marker
    // Case 1: IFC root itself is list-item (e.g., <li> with display: list-item)
    // Case 2: IFC root's parent is list-item (e.g., <li><text>...</text></li>)
    let ifc_root_node = tree.get(ifc_root_index).ok_or(LayoutError::InvalidTree)?;
    let mut list_item_dom_id: Option<NodeId> = None;
    
    // Check IFC root itself
    if let Some(dom_id) = ifc_root_node.dom_node_id {
        let node_data = &ctx.styled_dom.node_data.as_container()[dom_id];
        let node_state = StyledNodeState::default();
        
        if let Some(display_value) = ctx.styled_dom.css_property_cache.ptr.get_display(node_data, &dom_id, &node_state) {
            if let Some(display) = display_value.get_property() {
                eprintln!("[collect_and_measure_inline_content] IFC root NodeId({:?}) has display: {:?}", dom_id, display);
                use azul_css::props::layout::LayoutDisplay;
                if *display == LayoutDisplay::ListItem {
                    list_item_dom_id = Some(dom_id);
                }
            }
        }
    }
    
    // Check IFC root's parent
    if list_item_dom_id.is_none() {
        if let Some(parent_idx) = ifc_root_node.parent {
            if let Some(parent_node) = tree.get(parent_idx) {
                if let Some(parent_dom_id) = parent_node.dom_node_id {
                    let parent_node_data = &ctx.styled_dom.node_data.as_container()[parent_dom_id];
                    let parent_node_state = StyledNodeState::default();
                    
                    if let Some(display_value) = ctx.styled_dom.css_property_cache.ptr.get_display(parent_node_data, &parent_dom_id, &parent_node_state) {
                        if let Some(display) = display_value.get_property() {
                            eprintln!("[collect_and_measure_inline_content] IFC root parent NodeId({:?}) has display: {:?}", parent_dom_id, display);
                            use azul_css::props::layout::LayoutDisplay;
                            if *display == LayoutDisplay::ListItem {
                                list_item_dom_id = Some(parent_dom_id);
                            }
                        }
                    }
                }
            }
        }
    }
    
    // If we found a list-item, generate markers
    if let Some(list_dom_id) = list_item_dom_id {
        eprintln!("[collect_and_measure_inline_content] ✓ Found list-item (NodeId({:?})), generating marker", list_dom_id);
        
        // Find the layout node index for the list-item DOM node
        let list_item_layout_idx = tree.nodes.iter().enumerate()
            .find(|(_, node)| node.dom_node_id == Some(list_dom_id))
            .map(|(idx, _)| idx);
        
        if let Some(list_idx) = list_item_layout_idx {
            // Generate marker text segments with proper Unicode font fallback
            let base_style = get_style_properties_with_context(tree, ctx.styled_dom, list_idx);
            let marker_segments = generate_list_marker_segments(
                tree,
                ctx.styled_dom,
                list_idx,
                ctx.counters,
                &ctx.font_manager.fc_cache,
                base_style,
            );
            
            for segment in marker_segments {
                eprintln!("[collect_and_measure_inline_content] ✓ Generated list marker segment: '{}'", segment.text);
                content.push(InlineContent::Text(segment));
            }
        }
    }
    
    drop(ifc_root_node);

    // IMPORTANT: We need to traverse the DOM, not just the layout tree!
    // According to CSS spec, a block container with inline-level children establishes
    // an IFC and should collect ALL inline content, including text nodes.
    // Text nodes exist in the DOM but might not have their own layout tree nodes.

    // Debug: Check what the node_hierarchy says about this node
    let node_hier_item = &ctx.styled_dom.node_hierarchy.as_container()[ifc_root_dom_id];
    eprintln!(
        "[collect_and_measure_inline_content] DEBUG: node_hier_item.first_child={:?}, \
         last_child={:?}",
        node_hier_item.first_child_id(ifc_root_dom_id),
        node_hier_item.last_child_id()
    );

    let dom_children: Vec<NodeId> = ifc_root_dom_id
        .az_children(&ctx.styled_dom.node_hierarchy.as_container())
        .collect();

    eprintln!(
        "[collect_and_measure_inline_content] IFC root has {} DOM children",
        dom_children.len()
    );

    for (item_idx, &dom_child_id) in dom_children.iter().enumerate() {
        let content_index = ContentIndex {
            run_index: ifc_root_index as u32,
            item_index: item_idx as u32,
        };

        let node_data = &ctx.styled_dom.node_data.as_container()[dom_child_id];

        // Check if this is a text node
        if let NodeType::Text(ref text_content) = node_data.get_node_type() {
            eprintln!(
                "[collect_and_measure_inline_content] ✓ Found text node (DOM child {:?}): '{}'",
                dom_child_id,
                text_content.as_str()
            );
            // For text nodes, inherit style from the IFC root (their parent in the layout tree)
            content.push(InlineContent::Text(StyledRun {
                text: text_content.to_string(),
                style: get_style_properties_with_context(tree, ctx.styled_dom, ifc_root_index),
                logical_start_byte: 0,
            }));
            // Text nodes don't have layout tree nodes, so we don't add them to child_map
            continue;
        }

        // For non-text nodes, find their corresponding layout tree node
        let child_index = children
            .iter()
            .find(|&&idx| {
                tree.get(idx)
                    .and_then(|n| n.dom_node_id)
                    .map(|id| id == dom_child_id)
                    .unwrap_or(false)
            })
            .copied();

        let Some(child_index) = child_index else {
            eprintln!(
                "[collect_and_measure_inline_content] WARNING: DOM child {:?} has no layout node",
                dom_child_id
            );
            continue;
        };

        let child_node = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
        // At this point we have a non-text DOM child with a layout node
        let dom_id = child_node.dom_node_id.unwrap();

        if get_display_property(ctx.styled_dom, Some(dom_id)) != LayoutDisplay::Inline {
            // This is an atomic inline-level box (e.g., inline-block, image).
            // We must determine its size and baseline before passing it to text3.

            // The intrinsic sizing pass has already calculated its preferred size.
            let intrinsic_size = child_node.intrinsic_sizes.clone().unwrap_or_default();
            // For an inline-block, its width is its max-content width.
            let width = intrinsic_size.max_content_width;

            // To find its height and baseline, we must lay out its contents.
            let styled_node_state = ctx
                .styled_dom
                .styled_nodes
                .as_container()
                .get(dom_id)
                .map(|n| n.state.clone())
                .unwrap_or_default();
            let writing_mode = get_writing_mode(ctx.styled_dom, dom_id, &styled_node_state);
            let child_constraints = LayoutConstraints {
                available_size: LogicalSize::new(width, f32::INFINITY),
                writing_mode,
                bfc_state: None, // Inline-blocks establish a new BFC, so no state is passed in.
                text_align: TextAlign::Start, // Does not affect size/baseline of the container.
            };

            // Drop the immutable borrow before calling layout_formatting_context
            drop(child_node);

            // Recursively lay out the inline-block to get its final height and baseline.
            // Note: This does not affect its final position, only its dimensions.
            let layout_output =
                layout_formatting_context(ctx, tree, text_cache, child_index, &child_constraints)?;

            let final_height = layout_output.overflow_size.height;
            let final_size = LogicalSize::new(width, final_height);

            // Update the node in the tree with its now-known used size.
            tree.get_mut(child_index).unwrap().used_size = Some(final_size);

            let baseline_offset = layout_output.baseline.unwrap_or(final_height);

            content.push(InlineContent::Shape(InlineShape {
                shape_def: ShapeDefinition::Rectangle {
                    size: crate::text3::cache::Size {
                        width,
                        height: final_height,
                    },
                    corner_radius: None,
                },
                fill: None,
                stroke: None,
                baseline_offset,
            }));
            child_map.insert(content_index, child_index);
        } else if let NodeType::Image(image_data) =
            ctx.styled_dom.node_data.as_container()[dom_id].get_node_type()
        {
            // Re-get child_node since we dropped it earlier for the inline-block case
            let child_node = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;

            // This is a simplified image handling. A real implementation would have more robust
            // intrinsic size resolution (e.g., from the image data itself).
            let intrinsic_size = child_node
                .intrinsic_sizes
                .clone()
                .unwrap_or(IntrinsicSizes {
                    max_content_width: 50.0,
                    max_content_height: 50.0,
                    ..Default::default()
                });
            content.push(InlineContent::Image(InlineImage {
                source: ImageSource::Url(String::new()), // Placeholder
                intrinsic_size: crate::text3::cache::Size {
                    width: intrinsic_size.max_content_width,
                    height: intrinsic_size.max_content_height,
                },
                display_size: None,
                baseline_offset: 0.0, // Images are bottom-aligned with the baseline by default
                alignment: crate::text3::cache::VerticalAlign::Baseline,
                object_fit: ObjectFit::Fill,
            }));
            child_map.insert(content_index, child_index);
        }
    }
    Ok((content, child_map))
}

/// Positions a floated child within the BFC and updates the floating context.
/// This function is fully writing-mode aware.
fn position_floated_child<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    _child_index: usize,
    child_margin_box_size: LogicalSize,
    float_type: LayoutFloat,
    constraints: &LayoutConstraints,
    _bfc_content_box: LogicalRect,
    current_main_offset: f32,
    floating_context: &mut FloatingContext,
) -> Result<LogicalPosition> {
    let wm = constraints.writing_mode;
    let child_main_size = child_margin_box_size.main(wm);
    let child_cross_size = child_margin_box_size.cross(wm);
    let bfc_cross_size = constraints.available_size.cross(wm);
    let mut placement_main_offset = current_main_offset;

    loop {
        // 1. Determine the available cross-axis space at the current `placement_main_offset`.
        let (available_cross_start, available_cross_end) = floating_context
            .available_line_box_space(
                placement_main_offset,
                placement_main_offset + child_main_size,
                bfc_cross_size,
                wm,
            );

        let available_cross_width = available_cross_end - available_cross_start;

        // 2. Check if the new float can fit in the available space.
        if child_cross_size <= available_cross_width {
            // It fits! Determine the final position and add it to the context.
            let final_cross_pos = match float_type {
                LayoutFloat::Left => available_cross_start,
                LayoutFloat::Right => available_cross_end - child_cross_size,
                LayoutFloat::None => unreachable!(),
            };
            let final_pos =
                LogicalPosition::from_main_cross(placement_main_offset, final_cross_pos, wm);

            let new_float_box = FloatBox {
                kind: float_type,
                rect: LogicalRect::new(final_pos, child_margin_box_size),
            };
            floating_context.floats.push(new_float_box);
            return Ok(final_pos);
        } else {
            // It doesn't fit. We must move the float down past an obstacle.
            // Find the lowest main-axis end of all floats that are blocking the current line.
            let mut next_main_offset = f32::INFINITY;
            for existing_float in &floating_context.floats {
                let float_main_start = existing_float.rect.origin.main(wm);
                let float_main_end = float_main_start + existing_float.rect.size.main(wm);

                // Consider only floats that are above or at the current placement line.
                if placement_main_offset < float_main_end {
                    next_main_offset = next_main_offset.min(float_main_end);
                }
            }

            if next_main_offset.is_infinite() {
                // This indicates an unrecoverable state, e.g., a float wider than the container.
                return Err(LayoutError::PositioningFailed);
            }
            placement_main_offset = next_main_offset;
        }
    }
}

// STUB: Functions to get CSS properties
fn get_float_property(styled_dom: &StyledDom, dom_id: Option<NodeId>) -> LayoutFloat {
    let Some(id) = dom_id else {
        return LayoutFloat::None;
    };
    let node_data = &styled_dom.node_data.as_container()[id];
    let node_state = &styled_dom.styled_nodes.as_container()[id].state;
    styled_dom
        .css_property_cache
        .ptr
        .get_float(node_data, &id, node_state)
        .and_then(|f| {
            f.get_property().map(|inner| match inner {
                azul_css::props::layout::LayoutFloat::Left => LayoutFloat::Left,
                azul_css::props::layout::LayoutFloat::Right => LayoutFloat::Right,
                azul_css::props::layout::LayoutFloat::None => LayoutFloat::None,
            })
        })
        .unwrap_or(LayoutFloat::None)
}

fn get_clear_property(styled_dom: &StyledDom, dom_id: Option<NodeId>) -> LayoutClear {
    let Some(id) = dom_id else {
        return LayoutClear::None;
    };
    let node_data = &styled_dom.node_data.as_container()[id];
    let node_state = &styled_dom.styled_nodes.as_container()[id].state;
    // There is no dedicated `get_clear` helper on the cache, so use the generic
    // get_property -> as_clear path and then extract the inner value.
    styled_dom
        .css_property_cache
        .ptr
        .get_property(
            node_data,
            &id,
            node_state,
            &azul_css::props::property::CssPropertyType::Clear,
        )
        .and_then(|p| p.as_clear())
        .and_then(|v| v.get_property())
        .map(|clear| match clear {
            azul_css::props::layout::LayoutClear::Left => LayoutClear::Left,
            azul_css::props::layout::LayoutClear::Right => LayoutClear::Right,
            azul_css::props::layout::LayoutClear::Both => LayoutClear::Both,
            azul_css::props::layout::LayoutClear::None => LayoutClear::None,
        })
        .unwrap_or(LayoutClear::None)
}

/// Helper to determine if scrollbars are needed
pub fn check_scrollbar_necessity(
    content_size: LogicalSize,
    container_size: LogicalSize,
    overflow_x: OverflowBehavior,
    overflow_y: OverflowBehavior,
) -> ScrollbarInfo {
    let mut needs_horizontal = match overflow_x {
        OverflowBehavior::Visible | OverflowBehavior::Hidden | OverflowBehavior::Clip => false,
        OverflowBehavior::Scroll => true,
        OverflowBehavior::Auto => content_size.width > container_size.width,
    };

    let mut needs_vertical = match overflow_y {
        OverflowBehavior::Visible | OverflowBehavior::Hidden | OverflowBehavior::Clip => false,
        OverflowBehavior::Scroll => true,
        OverflowBehavior::Auto => content_size.height > container_size.height,
    };

    // A classic layout problem: a vertical scrollbar can reduce horizontal space,
    // causing a horizontal scrollbar to appear, which can reduce vertical space...
    // A full solution involves a loop, but this two-pass check handles most cases.
    if needs_vertical && !needs_horizontal && overflow_x == OverflowBehavior::Auto {
        if content_size.width > (container_size.width - 16.0) {
            // Assuming 16px scrollbar
            needs_horizontal = true;
        }
    }
    if needs_horizontal && !needs_vertical && overflow_y == OverflowBehavior::Auto {
        if content_size.height > (container_size.height - 16.0) {
            needs_vertical = true;
        }
    }

    ScrollbarInfo {
        needs_horizontal,
        needs_vertical,
        scrollbar_width: if needs_vertical { 16.0 } else { 0.0 },
        scrollbar_height: if needs_horizontal { 16.0 } else { 0.0 },
    }
}

/// Calculates a single collapsed margin from two adjoining vertical margins.
///
/// Implements the rules from CSS 2.1 section 8.3.1:
/// - If both margins are positive, the result is the larger of the two.
/// - If both margins are negative, the result is the more negative of the two.
/// - If the margins have mixed signs, they are effectively summed.
fn collapse_margins(a: f32, b: f32) -> f32 {
    if a.is_sign_positive() && b.is_sign_positive() {
        a.max(b)
    } else if a.is_sign_negative() && b.is_sign_negative() {
        a.min(b)
    } else {
        a + b
    }
}

/// Generates marker text for a list item marker.
///
/// This function looks up the counter value from the cache and formats it
/// according to the list-style-type property.
fn generate_list_marker_text<T: ParsedFontTrait>(
    tree: &LayoutTree<T>,
    styled_dom: &StyledDom,
    marker_index: usize,
    counters: &BTreeMap<(usize, String), i32>,
) -> String {
    use crate::solver3::{counters::format_counter, getters::get_list_style_type};
    
    // Get the parent list-item node
    let marker_node = match tree.get(marker_index) {
        Some(n) => n,
        None => return String::new(),
    };
    
    let parent_index = match marker_node.parent {
        Some(p) => p,
        None => return String::new(),
    };
    
    let parent_node = match tree.get(parent_index) {
        Some(n) => n,
        None => return String::new(),
    };
    
    let parent_dom_id = match parent_node.dom_node_id {
        Some(id) => id,
        None => return String::new(),
    };
    
    // Get list-style-type from the list-item element
    let list_style_type = get_list_style_type(styled_dom, Some(parent_dom_id));
    
    // Get the counter value for "list-item" counter
    let counter_value = counters
        .get(&(parent_index, "list-item".to_string()))
        .copied()
        .unwrap_or(1);
    
    // Format the counter according to the list-style-type
    let marker_text = format_counter(counter_value, list_style_type);
    
    // For ordered lists (non-symbolic markers), add a period and space
    // For unordered lists (symbolic markers like •, ◦, ▪), just add a space
    if matches!(
        list_style_type,
        azul_css::props::style::lists::StyleListStyleType::Decimal
            | azul_css::props::style::lists::StyleListStyleType::DecimalLeadingZero
            | azul_css::props::style::lists::StyleListStyleType::LowerAlpha
            | azul_css::props::style::lists::StyleListStyleType::UpperAlpha
            | azul_css::props::style::lists::StyleListStyleType::LowerRoman
            | azul_css::props::style::lists::StyleListStyleType::UpperRoman
            | azul_css::props::style::lists::StyleListStyleType::LowerGreek
            | azul_css::props::style::lists::StyleListStyleType::UpperGreek
    ) {
        format!("{}. ", marker_text)
    } else {
        format!("{} ", marker_text)
    }
}

/// Generates marker text segments with appropriate font fallback for Unicode coverage.
///
/// Uses FcCache.query_for_text() to find fonts that can render all characters in the marker.
/// Returns multiple StyledRun segments if different fonts are needed for different parts.
///
/// This implements the architecture from fixscript.md:
/// 1. Pre-Query Fonts: Call query_for_text() to get font stack
/// 2. Load Font Metadata: Get character coverage info from fontconfig
/// 3. Iterate by Grapheme: Check each grapheme cluster
/// 4. Group and Shape: Group consecutive graphemes with same font
/// 5. Combine Results: Return multiple StyledRuns with appropriate font families
fn generate_list_marker_segments<T: ParsedFontTrait>(
    tree: &LayoutTree<T>,
    styled_dom: &StyledDom,
    marker_index: usize,
    counters: &BTreeMap<(usize, String), i32>,
    fc_cache: &rust_fontconfig::FcFontCache,
    base_style: Arc<StyleProperties>,
) -> Vec<StyledRun> {
    use unicode_segmentation::UnicodeSegmentation;
    use rust_fontconfig::{FcPattern, PatternMatch};
    
    // Generate the marker text
    let marker_text = generate_list_marker_text(tree, styled_dom, marker_index, counters);
    if marker_text.is_empty() {
        return Vec::new();
    }
    
    // Step 1: Pre-Query Fonts - get font stack from fontconfig
    let pattern = FcPattern {
        name: Some(base_style.font_selector.family.clone()),
        weight: base_style.font_selector.weight,
        italic: if base_style.font_selector.style == crate::text3::cache::FontStyle::Italic {
            PatternMatch::True
        } else {
            PatternMatch::DontCare
        },
        oblique: if base_style.font_selector.style == crate::text3::cache::FontStyle::Oblique {
            PatternMatch::True
        } else {
            PatternMatch::DontCare
        },
        ..Default::default()
    };
    
    let mut trace = Vec::new();
    let font_matches = fc_cache.query_for_text(&pattern, &marker_text, &mut trace);
    
    // If no fonts found, return a single segment with the original style
    if font_matches.is_empty() {
        eprintln!(
            "[generate_list_marker_segments] WARNING: No fonts found for marker text '{}', using base style",
            marker_text
        );
        return vec![StyledRun {
            text: marker_text,
            style: base_style,
            logical_start_byte: 0,
        }];
    }
    
    eprintln!(
        "[generate_list_marker_segments] Found {} font matches for marker '{}': {:?}",
        font_matches.len(),
        marker_text,
        font_matches.iter().map(|m| fc_cache.get_metadata_by_id(&m.id).and_then(|p| p.name.as_ref())).collect::<Vec<_>>()
    );
    
    // Step 2: Build font metadata list for glyph coverage checking
    let font_stack: Vec<(usize, Option<String>)> = font_matches
        .iter()
        .enumerate()
        .map(|(idx, fm)| {
            let name = fc_cache
                .get_metadata_by_id(&fm.id)
                .and_then(|p| p.name.clone());
            (idx, name)
        })
        .collect();
    
    // Step 3: Iterate by Grapheme Cluster
    // For each grapheme, find the first font in our stack that can render it
    let graphemes: Vec<(usize, &str)> = marker_text.grapheme_indices(true).collect();
    
    if graphemes.is_empty() {
        return vec![StyledRun {
            text: marker_text,
            style: base_style,
            logical_start_byte: 0,
        }];
    }
    
    let mut segments = Vec::new();
    let mut current_font_idx: Option<usize> = None;
    let mut segment_start_byte = 0;
    
    for (byte_idx, grapheme) in graphemes.iter() {
        let first_char = grapheme.chars().next().unwrap_or('\u{FFFD}');
        
        // Find the first font in our stack that can render this grapheme
        let best_font_idx = font_matches
            .iter()
            .enumerate()
            .find(|(_, fm)| {
                fc_cache
                    .get_metadata_by_id(&fm.id)
                    .map(|metadata| metadata.contains_char(first_char))
                    .unwrap_or(false)
            })
            .map(|(idx, _)| idx);
        
        // Check if we need to start a new segment (font change or first grapheme)
        let should_segment = current_font_idx.is_none() 
            || current_font_idx != best_font_idx;
        
        if should_segment {
            // Flush previous segment if exists
            if current_font_idx.is_some() && segment_start_byte < *byte_idx {
                let segment_text = &marker_text[segment_start_byte..*byte_idx];
                
                // Create a style with the appropriate font family
                let segment_style = if let Some(font_idx) = current_font_idx {
                    if let Some((_, Some(font_name))) = font_stack.get(font_idx) {
                        let mut new_style = (*base_style).clone();
                        new_style.font_selector.family = font_name.clone();
                        Arc::new(new_style)
                    } else {
                        base_style.clone()
                    }
                } else {
                    base_style.clone()
                };
                
                eprintln!(
                    "[generate_list_marker_segments] Segment: '{}' with font '{}'",
                    segment_text,
                    segment_style.font_selector.family
                );
                
                segments.push(StyledRun {
                    text: segment_text.to_string(),
                    style: segment_style,
                    logical_start_byte: segment_start_byte,
                });
            }
            
            // Start new segment
            segment_start_byte = *byte_idx;
            current_font_idx = best_font_idx;
        }
    }
    
    // Flush final segment
    if segment_start_byte < marker_text.len() {
        let segment_text = &marker_text[segment_start_byte..];
        
        let segment_style = if let Some(font_idx) = current_font_idx {
            if let Some((_, Some(font_name))) = font_stack.get(font_idx) {
                let mut new_style = (*base_style).clone();
                new_style.font_selector.family = font_name.clone();
                Arc::new(new_style)
            } else {
                base_style.clone()
            }
        } else {
            base_style.clone()
        };
        
        eprintln!(
            "[generate_list_marker_segments] Final segment: '{}' with font '{}'",
            segment_text,
            segment_style.font_selector.family
        );
        
        segments.push(StyledRun {
            text: segment_text.to_string(),
            style: segment_style,
            logical_start_byte: segment_start_byte,
        });
    }
    
    // If no segments were created (shouldn't happen), return single segment
    if segments.is_empty() {
        eprintln!(
            "[generate_list_marker_segments] WARNING: No segments created, returning full text"
        );
        segments.push(StyledRun {
            text: marker_text,
            style: base_style,
            logical_start_byte: 0,
        });
    }
    
    segments
}

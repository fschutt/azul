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

/// Result of BFC layout with margin escape information
#[derive(Debug, Clone)]
struct BfcLayoutResult {
    /// Standard layout output (positions, overflow size, baseline)
    output: LayoutOutput,
    
    /// Top margin that escaped the BFC (for parent-child collapse)
    /// If Some, this margin should be used by parent instead of positioning this BFC
    escaped_top_margin: Option<f32>,
    
    /// Bottom margin that escaped the BFC (for parent-child collapse)
    /// If Some, this margin should collapse with next sibling
    escaped_bottom_margin: Option<f32>,
}

impl BfcLayoutResult {
    fn from_output(output: LayoutOutput) -> Self {
        Self {
            output,
            escaped_top_margin: None,
            escaped_bottom_margin: None,
        }
    }
}

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
#[derive(Debug, Default, Clone)]
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
    /// Add a newly positioned float to the context
    pub fn add_float(&mut self, kind: LayoutFloat, rect: LogicalRect) {
        self.floats.push(FloatBox { kind, rect });
    }
    
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

/// Helper: Convert StyleFontStyle to text3::cache::FontStyle
pub(crate) fn convert_font_style(style: azul_css::props::basic::font::StyleFontStyle) -> crate::text3::cache::FontStyle {
    use azul_css::props::basic::font::StyleFontStyle;
    match style {
        StyleFontStyle::Normal => crate::text3::cache::FontStyle::Normal,
        StyleFontStyle::Italic => crate::text3::cache::FontStyle::Italic,
        StyleFontStyle::Oblique => crate::text3::cache::FontStyle::Oblique,
    }
}

/// Helper: Convert StyleFontWeight to rust_fontconfig::FcWeight
pub(crate) fn convert_font_weight(weight: azul_css::props::basic::font::StyleFontWeight) -> rust_fontconfig::FcWeight {
    use azul_css::props::basic::font::StyleFontWeight;
    match weight {
        StyleFontWeight::W100 => rust_fontconfig::FcWeight::Thin,
        StyleFontWeight::W200 => rust_fontconfig::FcWeight::ExtraLight,
        StyleFontWeight::W300 | StyleFontWeight::Lighter => rust_fontconfig::FcWeight::Light,
        StyleFontWeight::Normal => rust_fontconfig::FcWeight::Normal,
        StyleFontWeight::W500 => rust_fontconfig::FcWeight::Medium,
        StyleFontWeight::W600 => rust_fontconfig::FcWeight::SemiBold,
        StyleFontWeight::Bold => rust_fontconfig::FcWeight::Bold,
        StyleFontWeight::W800 => rust_fontconfig::FcWeight::ExtraBold,
        StyleFontWeight::W900 | StyleFontWeight::Bolder => rust_fontconfig::FcWeight::Black,
    }
}

/// Checks if a node establishes a new Block Formatting Context (BFC).
/// 
/// Per CSS 2.2 § 9.4.1, a BFC is established by:
/// - Floats (elements with float other than 'none')
/// - Absolutely positioned elements (position: absolute or fixed)
/// - Block containers that are not block boxes (e.g., inline-blocks, table-cells)
/// - Block boxes with 'overflow' other than 'visible' and 'clip'
/// - Elements with 'display: flow-root'
/// - Table cells, table captions, and inline-blocks
/// 
/// Normal flow block-level boxes do NOT establish a new BFC.
/// This is critical for correct float interaction: normal blocks should overlap floats
/// (not shrink around them), while their inline content wraps around floats.
fn establishes_new_bfc<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &LayoutContext<T, Q>,
    node: &LayoutNode<T>,
) -> bool {
    use crate::solver3::getters::{get_float, get_display_property, get_overflow_x, get_overflow_y};
    use azul_core::dom::FormattingContext;
    use azul_core::styled_dom::StyledNodeState;
    use azul_css::props::layout::{LayoutFloat, LayoutDisplay, LayoutOverflow, LayoutPosition};
    
    let Some(dom_id) = node.dom_node_id else {
        return false;
    };
    
    let node_state = &ctx.styled_dom.styled_nodes.as_container()[dom_id].state;
    
    // 1. Floats establish BFC
    let float_val = get_float(ctx.styled_dom, dom_id, node_state);
    if matches!(float_val, crate::solver3::getters::MultiValue::Exact(LayoutFloat::Left | LayoutFloat::Right)) {
        return true;
    }
    
    // 2. Absolutely positioned elements establish BFC
    let position = crate::solver3::positioning::get_position_type(ctx.styled_dom, Some(dom_id));
    if matches!(position, LayoutPosition::Absolute | LayoutPosition::Fixed) {
        return true;
    }
    
    // 3. Inline-blocks, table-cells, table-captions establish BFC
    let display = get_display_property(ctx.styled_dom, Some(dom_id));
    if matches!(display, 
        crate::solver3::getters::MultiValue::Exact(
            LayoutDisplay::InlineBlock | 
            LayoutDisplay::TableCell | 
            LayoutDisplay::TableCaption
        )
    ) {
        return true;
    }
    
    // 4. display: flow-root establishes BFC
    if matches!(display, crate::solver3::getters::MultiValue::Exact(LayoutDisplay::FlowRoot)) {
        return true;
    }
    
    // 5. Block boxes with overflow other than 'visible' or 'clip' establish BFC
    // Note: 'clip' does NOT establish BFC per CSS Overflow Module Level 3
    let overflow_x = get_overflow_x(ctx.styled_dom, dom_id, node_state);
    let overflow_y = get_overflow_y(ctx.styled_dom, dom_id, node_state);
    
    let creates_bfc_via_overflow = |ov: &crate::solver3::getters::MultiValue<LayoutOverflow>| {
        matches!(ov, 
            &crate::solver3::getters::MultiValue::Exact(
                LayoutOverflow::Hidden | 
                LayoutOverflow::Scroll | 
                LayoutOverflow::Auto
            )
        )
    };
    
    if creates_bfc_via_overflow(&overflow_x) || creates_bfc_via_overflow(&overflow_y) {
        return true;
    }
    
    // 6. Table, Flex, and Grid containers establish BFC (via FormattingContext)
    if matches!(node.formatting_context, 
        FormattingContext::Table | 
        FormattingContext::Flex | 
        FormattingContext::Grid
    ) {
        return true;
    }
    
    // Normal flow block boxes do NOT establish BFC
    false
}

/// Main dispatcher for formatting context layout.
pub fn layout_formatting_context<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &mut LayoutContext<T, Q>,
    tree: &mut LayoutTree<T>,
    text_cache: &mut text3::cache::LayoutCache<T>,
    node_index: usize,
    constraints: &LayoutConstraints,
    float_cache: &mut std::collections::BTreeMap<usize, FloatingContext>,
) -> Result<LayoutOutput> {
    let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;

    ctx.debug_info(format!("[layout_formatting_context] node_index={}, fc={:?}, available_size={:?}", 
        node_index, node.formatting_context, constraints.available_size));

    match node.formatting_context {
        FormattingContext::Block { .. } => {
            layout_bfc(ctx, tree, text_cache, node_index, constraints, float_cache).map(|r| r.output)
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
        _ => {
            let mut temp_float_cache = std::collections::BTreeMap::new();
            layout_bfc(ctx, tree, text_cache, node_index, constraints, &mut temp_float_cache).map(|r| r.output)
        }
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

/// Position a float within a BFC, considering existing floats.
/// Returns the LogicalRect (margin box) for the float.
fn position_float(
    float_ctx: &FloatingContext,
    float_type: LayoutFloat,
    size: LogicalSize,
    margin: &EdgeSizes,
    current_main_offset: f32,
    bfc_cross_size: f32,
    wm: LayoutWritingMode,
) -> LogicalRect {
    // Start at the current main-axis position (Y in horizontal-tb)
    let mut main_start = current_main_offset;
    
    // Calculate total size including margins
    let total_main = size.main(wm) + margin.main_start(wm) + margin.main_end(wm);
    let total_cross = size.cross(wm) + margin.cross_start(wm) + margin.cross_end(wm);
    
    // Find a position where the float fits
    let cross_start = loop {
        let (avail_start, avail_end) = float_ctx.available_line_box_space(
            main_start,
            main_start + total_main,
            bfc_cross_size,
            wm,
        );
        
        let available_width = avail_end - avail_start;
        
        if available_width >= total_cross {
            // Found space that fits
            if float_type == LayoutFloat::Left {
                // Position at line-left (avail_start)
                break avail_start + margin.cross_start(wm);
            } else {
                // Position at line-right (avail_end - size)
                break avail_end - total_cross + margin.cross_start(wm);
            }
        }
        
        // Not enough space at this Y, move down past the lowest overlapping float
        let next_main = float_ctx.floats.iter()
            .filter(|f| {
                let f_main_start = f.rect.origin.main(wm);
                let f_main_end = f_main_start + f.rect.size.main(wm);
                f_main_end > main_start && f_main_start < main_start + total_main
            })
            .map(|f| f.rect.origin.main(wm) + f.rect.size.main(wm))
            .max_by(|a, b| a.partial_cmp(b).unwrap());
        
        if let Some(next) = next_main {
            main_start = next;
        } else {
            // No overlapping floats found, use current position anyway
            if float_type == LayoutFloat::Left {
                break avail_start + margin.cross_start(wm);
            } else {
                break avail_end - total_cross + margin.cross_start(wm);
            }
        }
    };
    
    LogicalRect {
        origin: LogicalPosition::from_main_cross(
            main_start + margin.main_start(wm),
            cross_start,
            wm
        ),
        size,
    }
}

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
/// # Margin Collapsing Architecture
///
/// CSS 2.1 Section 8.3.1 compliant margin collapsing:
///
/// ```text
/// layout_bfc()
///   ├─ Check parent border/padding blockers
///   ├─ For each child:
///   │   ├─ Check child border/padding blockers
///   │   ├─ is_first_child?
///   │   │   └─ Check parent-child top collapse
///   │   ├─ Sibling collapse?
///   │   │   └─ advance_pen_with_margin_collapse()
///   │   │       └─ collapse_margins(prev_bottom, curr_top)
///   │   ├─ Position child
///   │   ├─ is_empty_block()?
///   │   │   └─ Collapse own top+bottom margins (collapse through)
///   │   └─ Save bottom margin for next sibling
///   └─ Check parent-child bottom collapse
/// ```
///
/// **Collapsing Rules:**
/// - Sibling margins: Adjacent vertical margins collapse to max (or sum if mixed signs)
/// - Parent-child: First child's top margin can escape parent (if no border/padding)
/// - Parent-child: Last child's bottom margin can escape parent (if no border/padding/height)
/// - Empty blocks: Top+bottom margins collapse with each other, then with siblings
/// - Blockers: Border, padding, inline content, or new BFC prevents collapsing
///
/// This approach is compliant with the CSS visual formatting model and works within
/// the constraints of the existing layout engine architecture.
fn layout_bfc<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &mut LayoutContext<T, Q>,
    tree: &mut LayoutTree<T>,
    text_cache: &mut text3::cache::LayoutCache<T>,
    node_index: usize,
    constraints: &LayoutConstraints,
    float_cache: &mut std::collections::BTreeMap<usize, FloatingContext>,
) -> Result<BfcLayoutResult> {
    let node = tree
        .get(node_index)
        .ok_or(LayoutError::InvalidTree)?
        .clone();
    let writing_mode = constraints.writing_mode;
    let mut output = LayoutOutput::default();
    
    eprintln!("\n[layout_bfc] ENTERED for node_index={}, children.len()={}, incoming_bfc_state={}", 
        node_index, node.children.len(), constraints.bfc_state.is_some());
    
    // Initialize FloatingContext for this BFC
    // We always recalculate float positions in this pass, but we'll store them in the cache
    // so that subsequent layout passes (for auto-sizing) have access to the positioned floats
    let mut float_context = FloatingContext::default();

    // --- Pass 1: Size all children (floats and normal flow) ---
    for &child_index in &node.children {
        let child_node = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
        let child_dom_id = child_node.dom_node_id;

        // Skip out-of-flow children (absolute/fixed)
        let position_type = get_position_type(ctx.styled_dom, child_dom_id);
        if position_type == LayoutPosition::Absolute || position_type == LayoutPosition::Fixed {
            continue;
        }

        // Size all children (floats and normal flow) - floats will be positioned later in Pass 2
        let mut temp_positions = BTreeMap::new();
        crate::solver3::cache::calculate_layout_for_subtree(
            ctx,
            tree,
            text_cache,
            child_index,
            LogicalPosition::zero(),
            constraints.available_size,
            &mut temp_positions,
            &mut bool::default(),
            float_cache,
        )?;
    }

    // --- Pass 2: Single-pass interleaved layout (position floats and normal flow in DOM order) ---
    
    let mut main_pen = 0.0f32;
    let mut max_cross_size = 0.0f32;
    
    // Margin collapsing state
    let mut last_margin_bottom = 0.0f32;
    let mut is_first_child = true;
    let mut first_child_index: Option<usize> = None;
    let mut last_child_index: Option<usize> = None;
    
    // Parent's own margins (for escape calculation)
    let parent_margin_top = node.box_props.margin.main_start(writing_mode);
    let parent_margin_bottom = node.box_props.margin.main_end(writing_mode);
    
    // Check if parent (this BFC root) has border/padding that prevents parent-child collapse
    let parent_has_top_blocker = has_margin_collapse_blocker(&node.box_props, writing_mode, true);
    let parent_has_bottom_blocker = has_margin_collapse_blocker(&node.box_props, writing_mode, false);
    
    // Track accumulated top margin for first-child escape
    let mut accumulated_top_margin = 0.0f32;
    let mut top_margin_resolved = false;
    
    // Track if we have any actual content (non-empty blocks)
    let mut has_content = false;



    for &child_index in &node.children {
        let child_node = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
        let child_dom_id = child_node.dom_node_id;

        let position_type = get_position_type(ctx.styled_dom, child_dom_id);
        if position_type == LayoutPosition::Absolute || position_type == LayoutPosition::Fixed {
            continue;
        }
        
        // Check if this child is a float - if so, position it at current main_pen
        use crate::solver3::getters::get_float;
        let is_float = if let Some(node_id) = child_dom_id {
            let styled_node = &ctx.styled_dom.styled_nodes.as_container()[node_id];
            let float_type = get_float(ctx.styled_dom, node_id, &styled_node.state);
            
            if !float_type.is_none() {
                // Extract the actual LayoutFloat value
                if let crate::solver3::getters::MultiValue::Exact(float_val) = float_type {
                    let float_size = child_node.used_size.unwrap_or_default();
                    let float_margin = &child_node.box_props.margin;
                    
                    eprintln!(
                        "[layout_bfc] Positioning float: index={}, type={:?}, size={:?}, at Y={}",
                        child_index, float_val, float_size, main_pen
                    );
                    
                    // Position the float at the CURRENT main_pen (respects DOM order!)
                    let float_rect = position_float(
                        &float_context,
                        float_val,
                        float_size,
                        float_margin,
                        main_pen,  // Use current Y position, not 0!
                        constraints.available_size.cross(writing_mode),
                        writing_mode,
                    );
                    
                    eprintln!("[layout_bfc] Float positioned at: {:?}", float_rect);
                    
                    // Add to float context BEFORE positioning next element
                    float_context.add_float(float_val, float_rect);
                    
                    // Store position in output
                    output.positions.insert(child_index, float_rect.origin);
                    
                    eprintln!("[layout_bfc] *** FLOAT POSITIONED: child={}, main_pen={} (unchanged - floats don't advance pen)", child_index, main_pen);
                    
                    // Floats are taken out of normal flow - DON'T advance main_pen
                    // Continue to next child
                    continue;
                }
            }
            false
        } else {
            false
        };

        // From here: normal flow (non-float) children only

        // Track first and last in-flow children for parent-child collapse
        if first_child_index.is_none() {
            first_child_index = Some(child_index);
        }
        last_child_index = Some(child_index);

        let child_size = child_node.used_size.unwrap_or_default();
        let child_margin = &child_node.box_props.margin;
        
        eprintln!("[layout_bfc] Child {} margin from box_props: top={}, right={}, bottom={}, left={}",
            child_index, child_margin.top, child_margin.right, child_margin.bottom, child_margin.left);
        
        // IMPORTANT: Use the ACTUAL margins from box_props, NOT escaped margins!
        // Escaped margins are only relevant for the parent-child relationship WITHIN a node's
        // own BFC layout. When positioning this child in ITS parent's BFC, we use its actual margins.
        // CSS 2.2 § 8.3.1: Margin collapsing happens between ADJACENT margins, which means:
        // - Parent's top and first child's top (if no blocker)
        // - Sibling's bottom and next sibling's top
        // - Parent's bottom and last child's bottom (if no blocker)
        // The escaped_top_margin stored in the child node is for its OWN children, not for itself!
        let child_margin_top = child_margin.main_start(writing_mode);
        let child_margin_bottom = child_margin.main_end(writing_mode);
        
        eprintln!("[layout_bfc] Child {} final margins: margin_top={}, margin_bottom={}",
            child_index, child_margin_top, child_margin_bottom);
        
        // Check if this child has border/padding that prevents margin collapsing
        let child_has_top_blocker = has_margin_collapse_blocker(&child_node.box_props, writing_mode, true);
        let child_has_bottom_blocker = has_margin_collapse_blocker(&child_node.box_props, writing_mode, false);
        
        // PHASE 1: Empty Block Detection & Self-Collapse
        let is_empty = is_empty_block(child_node);
        
        // Handle empty blocks FIRST (they collapse through and don't participate in layout)
        if is_empty && !child_has_top_blocker && !child_has_bottom_blocker {
            // Empty block: collapse its own top and bottom margins FIRST
            let self_collapsed = collapse_margins(child_margin_top, child_margin_bottom);
            
            // Then collapse with previous margin (sibling or parent)
            if is_first_child {
                is_first_child = false;
                // Empty first child: its collapsed margin can escape with parent's
                if !parent_has_top_blocker {
                    accumulated_top_margin = collapse_margins(parent_margin_top, self_collapsed);
                } else {
                    // Parent has blocker: add margins
                    if accumulated_top_margin == 0.0 {
                        accumulated_top_margin = parent_margin_top;
                    }
                    main_pen += accumulated_top_margin + self_collapsed;
                    top_margin_resolved = true;
                    accumulated_top_margin = 0.0;
                }
                last_margin_bottom = self_collapsed;
            } else {
                // Empty sibling: collapse with previous sibling's bottom margin
                last_margin_bottom = collapse_margins(last_margin_bottom, self_collapsed);
            }
            
            // Skip positioning and pen advance (empty has no visual presence)
            continue;
        }
        
        // From here on: non-empty blocks only
        
        // Check for clear property and advance pen if needed
        use crate::solver3::getters::get_clear;
        if let Some(node_id) = child_dom_id {
            let styled_node = &ctx.styled_dom.styled_nodes.as_container()[node_id];
            let child_clear = get_clear(ctx.styled_dom, node_id, &styled_node.state);
            match child_clear {
                crate::solver3::getters::MultiValue::Exact(clear_val) if clear_val != LayoutClear::None => {
                    let cleared_offset = float_context.clearance_offset(
                        clear_val,
                        main_pen,
                        writing_mode,
                    );
                    if cleared_offset > main_pen {
                        ctx.debug_info(format!(
                            "[layout_bfc] Applying clearance: child={}, clear={:?}, old_pen={}, new_pen={}",
                            child_index, clear_val, main_pen, cleared_offset
                        ));
                        main_pen = cleared_offset;
                        // Clearance breaks margin collapse
                        last_margin_bottom = 0.0;
                    }
                }
                _ => {}
            }
        }
        
        // PHASE 2: Parent-Child Top Margin Escape (First Child)
        // CSS 2.2 § 8.3.1: "The top margin of a box is adjacent to the top margin of its first 
        // in-flow child if the box has no top border, no top padding, and the child has no clearance."
        if is_first_child {
            is_first_child = false;
            
            if !parent_has_top_blocker && !child_has_top_blocker {
                // First child's top margin can escape parent
                // Accumulate it with parent's margin (will be returned to parent's parent)
                accumulated_top_margin = collapse_margins(parent_margin_top, child_margin_top);
                // Position child at pen (no margin applied - it escaped!)
                eprintln!("[layout_bfc] First child {} margin ESCAPES: parent_margin={}, child_margin={}, collapsed={}",
                    child_index, parent_margin_top, child_margin_top, accumulated_top_margin);
            } else {
                // Can't escape: parent has blocker (padding/border)
                // CSS 2.2 § 8.3.1 Rule 3.1: Blocker prevents collapse
                // Advance pen by parent's top margin (if any) + child's full top margin
                if !top_margin_resolved {
                    main_pen += parent_margin_top;
                    top_margin_resolved = true;
                }
                main_pen += child_margin_top;
                eprintln!("[layout_bfc] First child {} BLOCKED: parent_has_blocker={}, advanced by parent_margin={} + child_margin={}, main_pen={}",
                    child_index, parent_has_top_blocker, parent_margin_top, child_margin_top, main_pen);
            }
        } else {
            // Not first child: handle sibling collapse
            // CSS 2.2 § 8.3.1 Rule 1: "Vertical margins of adjacent block boxes in the normal flow collapse"
            
            // Resolve accumulated top margin if not yet done (for parent's first in-flow child)
            if !top_margin_resolved {
                main_pen += accumulated_top_margin;
                top_margin_resolved = true;
            }
            
            // Sibling margin collapse
            let collapsed = collapse_margins(last_margin_bottom, child_margin_top);
            main_pen += collapsed;
            
            eprintln!("[layout_bfc] Sibling collapse for child {}: last_margin_bottom={}, child_margin_top={}, collapsed={}, main_pen={}",
                child_index, last_margin_bottom, child_margin_top, collapsed, main_pen);
        }

        // Position child (non-empty blocks only reach here)
        // 
        // CSS 2.2 § 9.4.1: "In a block formatting context, each box's left outer edge touches 
        // the left edge of the containing block (for right-to-left formatting, right edges touch). 
        // This is true even in the presence of floats (although a box's line boxes may shrink 
        // due to the floats), unless the box establishes a new block formatting context 
        // (in which case the box itself may become narrower due to the floats)."
        //
        // CSS 2.2 § 9.5: "The border box of a table, a block-level replaced element, or an element 
        // in the normal flow that establishes a new block formatting context (such as an element 
        // with 'overflow' other than 'visible') must not overlap any floats in the same block 
        // formatting context as the element itself."
        
        let child_node = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
        let establishes_bfc = establishes_new_bfc(ctx, child_node);
        
        // Query available space considering floats ONLY if child establishes new BFC
        let (cross_start, cross_end, available_cross) = if establishes_bfc {
            // New BFC: Must shrink or move down to avoid overlapping floats
            let (start, end) = float_context.available_line_box_space(
                main_pen,
                main_pen + child_size.main(writing_mode),
                constraints.available_size.cross(writing_mode),
                writing_mode,
            );
            let available = end - start;
            
            ctx.debug_info(format!(
                "[layout_bfc] Child {} establishes BFC: shrinking to avoid floats, cross_range={}..{}, available_cross={}",
                child_index, start, end, available
            ));
            
            (start, end, available)
        } else {
            // Normal flow: Overlaps floats, positioned at full width
            // Only the child's INLINE CONTENT (if any) wraps around floats
            let start = 0.0;
            let end = constraints.available_size.cross(writing_mode);
            let available = end - start;
            
            ctx.debug_info(format!(
                "[layout_bfc] Child {} is normal flow: overlapping floats at full width, available_cross={}",
                child_index, available
            ));
            
            (start, end, available)
        };
        
        // Get child's margin and formatting context
        let (child_margin_cloned, is_inline_fc) = {
            let child_node = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
            (child_node.box_props.margin.clone(), 
             child_node.formatting_context == FormattingContext::Inline)
        };
        let child_margin = &child_margin_cloned;
        
        // Position child
        // For normal flow blocks (including IFCs): position at full width (cross_start = 0)
        // For BFC-establishing blocks: position in available space between floats
        let (child_cross_pos, child_main_pos) = if establishes_bfc {
            // BFC: Position in space between floats
            (cross_start + child_margin.cross_start(writing_mode), main_pen)
        } else {
            // Normal flow: Position at full width (floats don't affect box position)
            (child_margin.cross_start(writing_mode), main_pen)
        };

        let final_pos =
            LogicalPosition::from_main_cross(child_main_pos, child_cross_pos, writing_mode);
        
        eprintln!(
            "[layout_bfc] *** NORMAL FLOW BLOCK POSITIONED: child={}, final_pos={:?}, main_pen={}, establishes_bfc={}",
            child_index, final_pos, main_pen, establishes_bfc
        );
        
        // Re-layout IFC children with float context for correct text wrapping
        // Normal flow blocks WITH inline content need float context propagated
        if is_inline_fc && !establishes_bfc {
            // Use cached floats if available (from previous layout passes),
            // otherwise use the floats positioned in this pass
            let floats_for_ifc = float_cache.get(&node_index).unwrap_or(&float_context);
            
            eprintln!(
                "[layout_bfc] Re-layouting IFC child {} (normal flow) with parent's float context at Y={}, child_cross_pos={}",
                child_index, main_pen, child_cross_pos
            );
            eprintln!(
                "[layout_bfc]   Using {} floats (from cache: {})",
                floats_for_ifc.floats.len(),
                float_cache.contains_key(&node_index)
            );
            
            // Translate float coordinates from BFC-relative to IFC-relative
            // The IFC child is positioned at (child_cross_pos, main_pen) in BFC coordinates
            // Floats need to be relative to the IFC's CONTENT-BOX origin (inside padding/border)
            let child_node = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
            let padding_border_cross = child_node.box_props.padding.cross_start(writing_mode) 
                                     + child_node.box_props.border.cross_start(writing_mode);
            let padding_border_main = child_node.box_props.padding.main_start(writing_mode)
                                    + child_node.box_props.border.main_start(writing_mode);
            
            // Content-box origin in BFC coordinates
            let content_box_cross = child_cross_pos + padding_border_cross;
            let content_box_main = main_pen + padding_border_main;
            
            eprintln!(
                "[layout_bfc]   Border-box at ({}, {}), Content-box at ({}, {}), padding+border=({}, {})",
                child_cross_pos, main_pen, content_box_cross, content_box_main,
                padding_border_cross, padding_border_main
            );
            
            let mut ifc_floats = FloatingContext::default();
            for float_box in &floats_for_ifc.floats {
                // Convert float position from BFC coords to IFC CONTENT-BOX relative coords
                let float_rel_to_ifc = LogicalRect {
                    origin: LogicalPosition {
                        x: float_box.rect.origin.x - content_box_cross,
                        y: float_box.rect.origin.y - content_box_main,
                    },
                    size: float_box.rect.size,
                };
                
                eprintln!(
                    "[layout_bfc] Float {:?}: BFC coords = {:?}, IFC-content-relative = {:?}",
                    float_box.kind, float_box.rect, float_rel_to_ifc
                );
                
                ifc_floats.add_float(float_box.kind, float_rel_to_ifc);
            }
            
            // Create a BfcState with IFC-relative float coordinates
            let mut bfc_state = BfcState {
                pen: LogicalPosition::zero(),  // IFC starts at its own origin
                floats: ifc_floats.clone(),
                margins: MarginCollapseContext::default(),
            };
            
            eprintln!(
                "[layout_bfc]   Created IFC-relative FloatingContext with {} floats",
                ifc_floats.floats.len()
            );
            
            // Get the IFC child's content-box size (after padding/border)
            let child_node = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
            let child_content_size = child_node.box_props.inner_size(child_size, writing_mode);
            
            eprintln!(
                "[layout_bfc]   IFC child size: border-box={:?}, content-box={:?}",
                child_size, child_content_size
            );
            
            // Create new constraints with float context
            // IMPORTANT: Use the child's CONTENT-BOX width, not the BFC width!
            let ifc_constraints = LayoutConstraints {
                available_size: child_content_size,
                bfc_state: Some(&mut bfc_state),
                writing_mode,
                text_align: constraints.text_align,
            };
            
            // Re-layout the IFC with float awareness
            // This will pass floats as exclusion zones to text3 for line wrapping
            let ifc_output = layout_formatting_context(
                ctx,
                tree,
                text_cache,
                child_index,
                &ifc_constraints,
                float_cache,
            )?;
            
            // DON'T update used_size - the box keeps its full width!
            // Only the text layout inside changes to wrap around floats
            
            eprintln!(
                "[layout_bfc] IFC child {} re-layouted with float context (text will wrap, box stays full width)",
                child_index
            );
            
            // Merge positions from IFC output (inline-block children, etc.)
            for (idx, pos) in ifc_output.positions {
                output.positions.insert(idx, pos);
            }
        }

        
        output.positions.insert(child_index, final_pos);

        // Advance the pen past the child's content size (potentially updated)
        main_pen += child_size.main(writing_mode);
        has_content = true;
        
        // Update last margin for next sibling
        // CSS 2.2 § 8.3.1: The bottom margin of this box will collapse with the top margin
        // of the next sibling (if no clearance or blockers intervene)
        last_margin_bottom = child_margin_bottom;
        
        eprintln!("[layout_bfc] Child {} positioned at final_pos={:?}, size={:?}, advanced main_pen to {}, last_margin_bottom={}",
            child_index, final_pos, child_size, main_pen, last_margin_bottom);

        // Track the maximum cross-axis size to determine the BFC's overflow size.
        let child_cross_extent =
            child_cross_pos + child_size.cross(writing_mode) + child_margin.cross_end(writing_mode);
        max_cross_size = max_cross_size.max(child_cross_extent);
    }
    
    // Store the float context in cache for future layout passes
    // This happens after ALL children (floats and normal) have been positioned
    eprintln!("[layout_bfc] Storing {} floats in cache for node {}", 
        float_context.floats.len(), node_index);
    float_cache.insert(node_index, float_context.clone());
    
    // PHASE 3: Parent-Child Bottom Margin Escape
    let mut escaped_top_margin = None;
    let mut escaped_bottom_margin = None;
    
    // Handle top margin escape
    if !top_margin_resolved && accumulated_top_margin > 0.0 {
        // No content was positioned, all margins accumulated
        // This can happen with only empty children
        escaped_top_margin = Some(accumulated_top_margin);
    } else if !top_margin_resolved {
        // First child's margin escaped
        escaped_top_margin = Some(accumulated_top_margin);
    }
    
    // Handle bottom margin escape
    if let Some(last_idx) = last_child_index {
        let last_child = tree.get(last_idx).ok_or(LayoutError::InvalidTree)?;
        let last_has_bottom_blocker = has_margin_collapse_blocker(&last_child.box_props, writing_mode, false);
        
        if !parent_has_bottom_blocker && !last_has_bottom_blocker && has_content {
            // Last child's bottom margin can escape
            let collapsed_bottom = collapse_margins(parent_margin_bottom, last_margin_bottom);
            escaped_bottom_margin = Some(collapsed_bottom);
            // Don't add last_margin_bottom to pen (it escaped)
        } else {
            // Can't escape: add to pen
            main_pen += last_margin_bottom;
            // Also add parent's bottom margin if we have content
            if has_content {
                main_pen += parent_margin_bottom;
            }
        }
    } else {
        // No children: just use parent's margins
        if !top_margin_resolved {
            main_pen += parent_margin_top;
        }
        main_pen += parent_margin_bottom;
    }

    // CRITICAL: If this is a root node (no parent), apply escaped margins directly
    // instead of propagating them upward (since there's no parent to receive them)
    let is_root_node = node.parent.is_none();
    if is_root_node {
        if let Some(top) = escaped_top_margin {
            // Adjust all child positions downward by the escaped top margin
            for (_, pos) in output.positions.iter_mut() {
                let current_main = pos.main(writing_mode);
                *pos = LogicalPosition::from_main_cross(
                    current_main + top,
                    pos.cross(writing_mode),
                    writing_mode
                );
            }
            main_pen += top;

        }
        if let Some(bottom) = escaped_bottom_margin {
            main_pen += bottom;

        }
        // For root nodes, don't propagate margins further
        escaped_top_margin = None;
        escaped_bottom_margin = None;
    }

    // BROWSER COMPATIBILITY FIX (CSS 2.2 § 9.5 + implicit browser behavior):
    // Major browsers (Chrome, Firefox) implicitly expand a block container with height:auto
    // to contain its floated children, even though CSS 2.2 technically says floats don't
    // contribute to the height of their container with overflow:visible.
    // 
    // This is a de-facto standard behavior that ensures floated content doesn't escape
    // its container's background/border, which would otherwise create visual artifacts.
    //
    // We implement this by checking if any float extends beyond the current main_pen
    // (which represents the bottom of in-flow content), and if so, expanding main_pen
    // to encompass the floats.
    if !float_context.floats.is_empty() {
        let lowest_float_end = float_context.floats.iter()
            .map(|f| f.rect.origin.y + f.rect.size.height)
            .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .unwrap_or(0.0);
        
        if lowest_float_end > main_pen {
            eprintln!(
                "[layout_bfc] FLOAT CONTAINMENT: Container {} expanding from main_pen={} to {} to contain floats",
                node_index, main_pen, lowest_float_end
            );
            main_pen = lowest_float_end;
        }
    }

    // The final overflow size is determined by the final pen position and the max cross size.
    output.overflow_size = LogicalSize::from_main_cross(main_pen, max_cross_size, writing_mode);

    // Baseline calculation would happen here in a full implementation.
    output.baseline = None;

    // Store escaped margins in the LayoutNode for use by parent
    if let Some(node_mut) = tree.get_mut(node_index) {
        node_mut.escaped_top_margin = escaped_top_margin;
        node_mut.escaped_bottom_margin = escaped_bottom_margin;
        

    }

    if let Some(node_mut) = tree.get_mut(node_index) {
        node_mut.baseline = output.baseline;
    }

    Ok(BfcLayoutResult {
        output,
        escaped_top_margin,
        escaped_bottom_margin,
    })
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
    let float_count = constraints.bfc_state.as_ref().map(|s| s.floats.floats.len()).unwrap_or(0);
    eprintln!("[layout_ifc] ENTRY: node_index={}, has_bfc_state={}, float_count={}", 
              node_index, constraints.bfc_state.is_some(), float_count);
    ctx.debug_ifc_layout(format!("CALLED for node_index={}", node_index));

    let ifc_root_dom_id = tree
        .get(node_index)
        .and_then(|n| n.dom_node_id)
        .ok_or(LayoutError::InvalidTree)?;

    ctx.debug_ifc_layout(format!("ifc_root_dom_id={:?}", ifc_root_dom_id));

    // Phase 1: Collect and measure all inline-level children.
    let (inline_content, child_map) =
        collect_and_measure_inline_content(ctx, text_cache, tree, node_index)?;

    eprintln!("[layout_ifc] Collected {} inline content items for node {}", inline_content.len(), node_index);
    for (i, item) in inline_content.iter().enumerate() {
        match item {
            InlineContent::Text(run) => eprintln!("  [{}] Text: '{}'", i, run.text),
            InlineContent::Marker { run, position_outside } => eprintln!("  [{}] Marker: '{}' (outside={})", i, run.text, position_outside),
            InlineContent::Shape(_) => eprintln!("  [{}] Shape", i),
            InlineContent::Image(_) => eprintln!("  [{}] Image", i),
            _ => eprintln!("  [{}] Other", i),
        }
    }
    
    ctx.debug_ifc_layout(format!("Collected {} inline content items", inline_content.len()));

    if inline_content.is_empty() {
        ctx.debug_warning("inline_content is empty, returning default output!");
        return Ok(LayoutOutput::default());
    }

    // Phase 2: Translate constraints and define a single layout fragment for text3.
    let text3_constraints =
        translate_to_text3_constraints(ctx, constraints, ctx.styled_dom, ifc_root_dom_id);
    
    eprintln!("[layout_ifc] CALLING text_cache.layout_flow for node {} with {} exclusions", 
              node_index, text3_constraints.shape_exclusions.len());
    
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
                ctx.debug_warning(format!("Text layout failed: {:?}", e));
                ctx.debug_warning(format!("Continuing with zero-sized layout for node {}", node_index));

                let mut output = LayoutOutput::default();
                output.overflow_size = LogicalSize::new(0.0, 0.0);
                return Ok(output);
            }
        };

    // Phase 4: Integrate results back into the solver3 layout tree.
    let mut output = LayoutOutput::default();
    let node = tree.get_mut(node_index).ok_or(LayoutError::InvalidTree)?;

    ctx.debug_ifc_layout(format!(
        "text_layout_result has {} fragment_layouts",
        text_layout_result.fragment_layouts.len()
    ));

    if let Some(main_frag) = text_layout_result.fragment_layouts.get("main") {
        let frag_bounds = main_frag.bounds();
        ctx.debug_ifc_layout(format!(
            "Found 'main' fragment with {} items, bounds={}x{}",
            main_frag.items.len(),
            frag_bounds.width,
            frag_bounds.height
        ));
        ctx.debug_ifc_layout(format!(
            "Storing inline_layout_result on node {}",
            node_index
        ));

        // Store the detailed result for the display list generator.
        // IMPORTANT: Only overwrite the result if we have float exclusions OR if there's no result yet.
        // This ensures that the final, float-aware layout is preserved even if there are
        // subsequent layout passes without floats (e.g., for auto-sizing).
        let has_floats = constraints.bfc_state.as_ref().map(|s| !s.floats.floats.is_empty()).unwrap_or(false);
        let should_store = has_floats || node.inline_layout_result.is_none();
        
        if should_store {
            eprintln!("[layout_ifc] Storing inline_layout_result for node {} (has_floats={}, is_new={})",
                      node_index, has_floats, node.inline_layout_result.is_none());
            node.inline_layout_result = Some(main_frag.clone());
        } else {
            eprintln!("[layout_ifc] SKIPPING inline_layout_result for node {} (no floats, result exists)",
                      node_index);
        }

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
fn translate_to_text3_constraints<'a, T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &mut LayoutContext<T, Q>,
    constraints: &'a LayoutConstraints<'a>,
    styled_dom: &StyledDom,
    dom_id: NodeId,
) -> UnifiedConstraints {
    use crate::text3::cache::TextAlign as Text3TextAlign;

    // Convert floats into exclusion zones for text3 to flow around.
    let mut shape_exclusions = if let Some(ref bfc_state) = constraints.bfc_state {
        eprintln!(
            "[translate_to_text3] dom_id={:?}, converting {} floats to exclusions",
            dom_id, bfc_state.floats.floats.len()
        );
        bfc_state
            .floats
            .floats
            .iter()
            .enumerate()
            .map(|(i, float_box)| {
                let rect = crate::text3::cache::Rect {
                    x: float_box.rect.origin.x,
                    y: float_box.rect.origin.y,
                    width: float_box.rect.size.width,
                    height: float_box.rect.size.height,
                };
                eprintln!(
                    "[translate_to_text3]   Exclusion #{}: {:?} at ({}, {}) size {}x{}",
                    i, float_box.kind, rect.x, rect.y, rect.width, rect.height
                );
                ShapeBoundary::Rectangle(rect)
            })
            .collect()
    } else {
        eprintln!("[translate_to_text3] dom_id={:?}, NO bfc_state - no float exclusions", dom_id);
        Vec::new()
    };
    
    eprintln!(
        "[translate_to_text3] dom_id={:?}, available_size={}x{}, shape_exclusions.len()={}",
        dom_id, constraints.available_size.width, constraints.available_size.height, shape_exclusions.len()
    );

    // Map text-align and justify-content from CSS to text3 enums.
    let id = dom_id;
    let node_data = &styled_dom.node_data.as_container()[id];
    let node_state = &styled_dom.styled_nodes.as_container()[id].state;

    // Read CSS Shapes properties
    // For reference box, use the element's CSS height if available, otherwise available_size
    // This is important because available_size.height might be infinite during auto height calculation
    let ref_box_height = if constraints.available_size.height.is_finite() {
        constraints.available_size.height
    } else {
        // Try to get explicit CSS height
        // NOTE: If height is infinite, we can't properly resolve % heights
        // This is a limitation - shape-inside with % heights requires finite containing block
        styled_dom
            .css_property_cache
            .ptr
            .get_height(node_data, &id, node_state)
            .and_then(|v| v.get_property())
            .and_then(|h| match h {
                azul_css::props::layout::dimensions::LayoutHeight::Px(v) => {
                    // Only accept absolute units (px, pt, in, cm, mm) - no %, em, rem
                    // since we can't resolve relative units without proper context
                    use azul_css::props::basic::{SizeMetric, pixel::PT_TO_PX};
                    match v.metric {
                        SizeMetric::Px => Some(v.number.get()),
                        SizeMetric::Pt => Some(v.number.get() * PT_TO_PX),
                        SizeMetric::In => Some(v.number.get() * 96.0),
                        SizeMetric::Cm => Some(v.number.get() * 96.0 / 2.54),
                        SizeMetric::Mm => Some(v.number.get() * 96.0 / 25.4),
                        _ => None, // Ignore %, em, rem
                    }
                },
                _ => None,
            })
            .unwrap_or(constraints.available_size.width) // Fallback: use width as height (square)
    };
    
    let reference_box = crate::text3::cache::Rect {
        x: 0.0,
        y: 0.0,
        width: constraints.available_size.width,
        height: ref_box_height,
    };

    // shape-inside: Text flows within the shape boundary
    ctx.debug_info(format!("Checking shape-inside for node {:?}", id));
    ctx.debug_info(format!("Reference box: {:?} (available_size height was: {})", reference_box, constraints.available_size.height));
    
    let shape_boundaries = styled_dom
        .css_property_cache
        .ptr
        .get_shape_inside(node_data, &id, node_state)
        .and_then(|v| {
            ctx.debug_info(format!("Got shape-inside value: {:?}", v));
            v.get_property()
        })
        .and_then(|shape_inside| {
            ctx.debug_info(format!("shape-inside property: {:?}", shape_inside));
            if let azul_css::props::layout::ShapeInside::Shape(css_shape) = shape_inside {
                ctx.debug_info(format!("Converting CSS shape to ShapeBoundary: {:?}", css_shape));
                let boundary = ShapeBoundary::from_css_shape(css_shape, reference_box);
                ctx.debug_info(format!("Created ShapeBoundary: {:?}", boundary));
                Some(vec![boundary])
            } else {
                ctx.debug_info("shape-inside is None");
                None
            }
        })
        .unwrap_or_default();
    
    ctx.debug_info(format!("Final shape_boundaries count: {}", shape_boundaries.len()));

    // shape-outside: Text wraps around the shape (adds to exclusions)
    ctx.debug_info(format!("Checking shape-outside for node {:?}", id));
    if let Some(shape_outside_value) = styled_dom
        .css_property_cache
        .ptr
        .get_shape_outside(node_data, &id, node_state)
    {
        ctx.debug_info(format!("Got shape-outside value: {:?}", shape_outside_value));
        if let Some(shape_outside) = shape_outside_value.get_property() {
            ctx.debug_info(format!("shape-outside property: {:?}", shape_outside));
            if let azul_css::props::layout::ShapeOutside::Shape(css_shape) = shape_outside {
                ctx.debug_info(format!("Converting CSS shape-outside to ShapeBoundary: {:?}", css_shape));
                let boundary = ShapeBoundary::from_css_shape(css_shape, reference_box);
                ctx.debug_info(format!("Created ShapeBoundary (exclusion): {:?}", boundary));
                shape_exclusions.push(boundary);
            }
        }
    } else {
        ctx.debug_info("No shape-outside value found");
    }

    // TODO: clip-path will be used for rendering clipping (not text layout)

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

    // Get font-size for resolving line-height
    // Use helper function which checks dependency chain first
    use crate::solver3::getters::get_element_font_size;
    let font_size = get_element_font_size(styled_dom, id, node_state);

    let line_height_value = styled_dom
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

    // Get the direction property from the CSS cache (defaults to LTR if not set)
    let direction = styled_dom
        .css_property_cache
        .ptr
        .get_direction(node_data, &id, node_state)
        .and_then(|s| s.get_property().copied())
        .map(|d| match d {
            azul_css::props::style::StyleDirection::Ltr => text3::cache::Direction::Ltr,
            azul_css::props::style::StyleDirection::Rtl => text3::cache::Direction::Rtl,
        });

    ctx.debug_info(format!(
        "dom_id={:?}, available_size={}x{}, setting available_width={}",
        dom_id,
        constraints.available_size.width,
        constraints.available_size.height,
        constraints.available_size.width
    ));

    // Get text-indent
    let text_indent = styled_dom
        .css_property_cache
        .ptr
        .get_text_indent(node_data, &id, node_state)
        .and_then(|s| s.get_property())
        .map(|ti| {
            use azul_css::props::basic::{ResolutionContext, PropertyContext, PhysicalSize};
            use crate::solver3::getters::{get_element_font_size, get_parent_font_size, get_root_font_size};
            
            let context = ResolutionContext {
                element_font_size: get_element_font_size(styled_dom, id, node_state),
                parent_font_size: get_parent_font_size(styled_dom, id, node_state),
                root_font_size: get_root_font_size(styled_dom, node_state),
                containing_block_size: PhysicalSize::new(constraints.available_size.width, 0.0),
                element_size: None,
                viewport_size: PhysicalSize::new(0.0, 0.0),
            };
            ti.inner.resolve_with_context(&context, PropertyContext::Other)
        })
        .unwrap_or(0.0);

    // Get column-count for multi-column layout (default: 1 = no columns)
    let columns = styled_dom
        .css_property_cache
        .ptr
        .get_column_count(node_data, &id, node_state)
        .and_then(|s| s.get_property())
        .map(|cc| match cc {
            azul_css::props::layout::ColumnCount::Integer(n) => *n,
            azul_css::props::layout::ColumnCount::Auto => 1,
        })
        .unwrap_or(1);

    // Get column-gap for multi-column layout (default: normal = 1em)
    let column_gap = styled_dom
        .css_property_cache
        .ptr
        .get_column_gap(node_data, &id, node_state)
        .and_then(|s| s.get_property())
        .map(|cg| {
            use azul_css::props::basic::{ResolutionContext, PropertyContext, PhysicalSize};
            use crate::solver3::getters::{get_element_font_size, get_parent_font_size, get_root_font_size};
            
            let context = ResolutionContext {
                element_font_size: get_element_font_size(styled_dom, id, node_state),
                parent_font_size: get_parent_font_size(styled_dom, id, node_state),
                root_font_size: get_root_font_size(styled_dom, node_state),
                containing_block_size: PhysicalSize::new(0.0, 0.0),
                element_size: None,
                viewport_size: PhysicalSize::new(0.0, 0.0),
            };
            cg.inner.resolve_with_context(&context, PropertyContext::Other)
        })
        .unwrap_or_else(|| {
            // Default: 1em
            get_element_font_size(styled_dom, id, node_state)
        });

    // Map white-space CSS property to TextWrap
    let text_wrap = styled_dom
        .css_property_cache
        .ptr
        .get_white_space(node_data, &id, node_state)
        .and_then(|s| s.get_property())
        .map(|ws| match ws {
            azul_css::props::style::StyleWhiteSpace::Normal => text3::cache::TextWrap::Wrap,
            azul_css::props::style::StyleWhiteSpace::Nowrap => text3::cache::TextWrap::NoWrap,
            azul_css::props::style::StyleWhiteSpace::Pre => text3::cache::TextWrap::NoWrap,
        })
        .unwrap_or(text3::cache::TextWrap::Wrap);

    // Get initial-letter for drop caps
    let initial_letter = styled_dom
        .css_property_cache
        .ptr
        .get_initial_letter(node_data, &id, node_state)
        .and_then(|s| s.get_property())
        .map(|il| {
            use std::num::NonZeroUsize;
            text3::cache::InitialLetter {
                size: il.size as f32,
                sink: il.sink.unwrap_or(il.size) as u32,
                count: NonZeroUsize::new(1).unwrap(),
            }
        });

    // Get line-clamp for limiting visible lines
    let line_clamp = styled_dom
        .css_property_cache
        .ptr
        .get_line_clamp(node_data, &id, node_state)
        .and_then(|s| s.get_property())
        .and_then(|lc| std::num::NonZeroUsize::new(lc.max_lines));

    // Get hanging-punctuation for hanging punctuation marks
    let hanging_punctuation = styled_dom
        .css_property_cache
        .ptr
        .get_hanging_punctuation(node_data, &id, node_state)
        .and_then(|s| s.get_property())
        .map(|hp| hp.enabled)
        .unwrap_or(false);

    // Get text-combine-upright for vertical text combination
    let text_combine_upright = styled_dom
        .css_property_cache
        .ptr
        .get_text_combine_upright(node_data, &id, node_state)
        .and_then(|s| s.get_property())
        .map(|tcu| match tcu {
            azul_css::props::style::StyleTextCombineUpright::None => text3::cache::TextCombineUpright::None,
            azul_css::props::style::StyleTextCombineUpright::All => text3::cache::TextCombineUpright::All,
            azul_css::props::style::StyleTextCombineUpright::Digits(n) => text3::cache::TextCombineUpright::Digits(*n),
        });

    // Get exclusion-margin for shape exclusions
    let exclusion_margin = styled_dom
        .css_property_cache
        .ptr
        .get_exclusion_margin(node_data, &id, node_state)
        .and_then(|s| s.get_property())
        .map(|em| em.inner.get() as f32)
        .unwrap_or(0.0);

    // Get hyphenation-language for language-specific hyphenation
    let hyphenation_language = styled_dom
        .css_property_cache
        .ptr
        .get_hyphenation_language(node_data, &id, node_state)
        .and_then(|s| s.get_property())
        .and_then(|hl| {
            use hyphenation::{Language, Load};
            // Parse BCP 47 language code to hyphenation::Language
            match hl.inner.as_str() {
                "en-US" | "en" => Some(Language::EnglishUS),
                "de-DE" | "de" => Some(Language::German1996),
                "fr-FR" | "fr" => Some(Language::French),
                "es-ES" | "es" => Some(Language::Spanish),
                "it-IT" | "it" => Some(Language::Italian),
                "pt-PT" | "pt" => Some(Language::Portuguese),
                "nl-NL" | "nl" => Some(Language::Dutch),
                "pl-PL" | "pl" => Some(Language::Polish),
                "ru-RU" | "ru" => Some(Language::Russian),
                "zh-CN" | "zh" => Some(Language::Chinese),
                _ => None, // Unsupported language
            }
        });

    UnifiedConstraints {
        exclusion_margin,
        hyphenation_language,
        text_indent,
        initial_letter,
        line_clamp,
        columns,
        column_gap,
        hanging_punctuation,
        text_wrap,
        text_combine_upright,
        segment_alignment: SegmentAlignment::Total,
        overflow: match overflow_behaviour {
            LayoutOverflow::Visible => text3::cache::OverflowBehavior::Visible,
            LayoutOverflow::Hidden | LayoutOverflow::Clip => text3::cache::OverflowBehavior::Hidden,
            LayoutOverflow::Scroll => text3::cache::OverflowBehavior::Scroll,
            LayoutOverflow::Auto => text3::cache::OverflowBehavior::Auto,
        },
        available_width: constraints.available_size.width,
        available_height: Some(constraints.available_size.height),
        shape_boundaries, // CSS shape-inside: text flows within shape
        shape_exclusions, // CSS shape-outside + floats: text wraps around shapes
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
        line_height: line_height_value.inner.normalized() * font_size, // Resolve line-height relative to font-size
        vertical_align: match vertical_align {
            StyleVerticalAlign::Top => text3::cache::VerticalAlign::Top,
            StyleVerticalAlign::Center => text3::cache::VerticalAlign::Middle,
            StyleVerticalAlign::Bottom => text3::cache::VerticalAlign::Bottom,
        },
    }
}

/// Lays out a Table Formatting Context.
/// Table column information for layout calculations
#[derive(Debug, Clone)]
pub struct TableColumnInfo {
    /// Minimum width required for this column
    pub min_width: f32,
    /// Maximum width desired for this column
    pub max_width: f32,
    /// Computed final width for this column
    pub computed_width: Option<f32>,
}

/// Information about a table cell for layout
#[derive(Debug, Clone)]
pub struct TableCellInfo {
    /// Node index in the layout tree
    pub node_index: usize,
    /// Column index (0-based)
    pub column: usize,
    /// Number of columns this cell spans
    pub colspan: usize,
    /// Row index (0-based)
    pub row: usize,
    /// Number of rows this cell spans
    pub rowspan: usize,
}

/// Table layout context - holds all information needed for table layout
#[derive(Debug)]
struct TableLayoutContext {
    /// Information about each column
    columns: Vec<TableColumnInfo>,
    /// Information about each cell
    cells: Vec<TableCellInfo>,
    /// Number of rows in the table
    num_rows: usize,
    /// Whether to use fixed or auto layout algorithm
    use_fixed_layout: bool,
    /// Computed height for each row
    row_heights: Vec<f32>,
    /// Border collapse mode
    border_collapse: azul_css::props::layout::StyleBorderCollapse,
    /// Border spacing (only used when border_collapse is Separate)
    border_spacing: azul_css::props::layout::LayoutBorderSpacing,
    /// CSS 2.2 Section 17.4: Index of table-caption child, if any
    caption_index: Option<usize>,
    /// CSS 2.2 Section 17.6: Rows with visibility:collapse (dynamic effects)
    /// Set of row indices that have visibility:collapse
    collapsed_rows: std::collections::HashSet<usize>,
    /// CSS 2.2 Section 17.6: Columns with visibility:collapse (dynamic effects)
    /// Set of column indices that have visibility:collapse
    collapsed_columns: std::collections::HashSet<usize>,
}

impl TableLayoutContext {
    fn new() -> Self {
        Self {
            columns: Vec::new(),
            cells: Vec::new(),
            num_rows: 0,
            use_fixed_layout: false,
            row_heights: Vec::new(),
            border_collapse: azul_css::props::layout::StyleBorderCollapse::Separate,
            border_spacing: azul_css::props::layout::LayoutBorderSpacing::default(),
            caption_index: None,
            collapsed_rows: std::collections::HashSet::new(),
            collapsed_columns: std::collections::HashSet::new(),
        }
    }
}

/// Source of a border in the border conflict resolution algorithm
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BorderSource {
    Table = 0,
    ColumnGroup = 1,
    Column = 2,
    RowGroup = 3,
    Row = 4,
    Cell = 5,
}

/// Information about a border for conflict resolution
#[derive(Debug, Clone)]
pub struct BorderInfo {
    pub width: f32,
    pub style: azul_css::props::style::BorderStyle,
    pub color: azul_css::props::basic::ColorU,
    pub source: BorderSource,
}

impl BorderInfo {
    pub fn new(
        width: f32,
        style: azul_css::props::style::BorderStyle,
        color: azul_css::props::basic::ColorU,
        source: BorderSource,
    ) -> Self {
        Self {
            width,
            style,
            color,
            source,
        }
    }
    
    /// Get the priority of a border style for conflict resolution
    /// Higher number = higher priority
    pub fn style_priority(style: &azul_css::props::style::BorderStyle) -> u8 {
        use azul_css::props::style::BorderStyle;
        match style {
            BorderStyle::Hidden => 255, // Highest - suppresses all borders
            BorderStyle::None => 0,     // Lowest - loses to everything
            BorderStyle::Double => 8,
            BorderStyle::Solid => 7,
            BorderStyle::Dashed => 6,
            BorderStyle::Dotted => 5,
            BorderStyle::Ridge => 4,
            BorderStyle::Outset => 3,
            BorderStyle::Groove => 2,
            BorderStyle::Inset => 1,
        }
    }
    
    /// Compare two borders for conflict resolution per CSS 2.2 Section 17.6.2.1
    /// Returns the winning border
    pub fn resolve_conflict(a: &BorderInfo, b: &BorderInfo) -> Option<BorderInfo> {
        use azul_css::props::style::BorderStyle;
        
        // 1. 'hidden' wins and suppresses all borders
        if a.style == BorderStyle::Hidden || b.style == BorderStyle::Hidden {
            return None;
        }
        
        // 2. Filter out 'none' - if both are none, no border
        let a_is_none = a.style == BorderStyle::None;
        let b_is_none = b.style == BorderStyle::None;
        
        if a_is_none && b_is_none {
            return None;
        }
        if a_is_none {
            return Some(b.clone());
        }
        if b_is_none {
            return Some(a.clone());
        }
        
        // 3. Wider border wins
        if a.width > b.width {
            return Some(a.clone());
        }
        if b.width > a.width {
            return Some(b.clone());
        }
        
        // 4. If same width, compare style priority
        let a_priority = Self::style_priority(&a.style);
        let b_priority = Self::style_priority(&b.style);
        
        if a_priority > b_priority {
            return Some(a.clone());
        }
        if b_priority > a_priority {
            return Some(b.clone());
        }
        
        // 5. If same style, source priority: Cell > Row > RowGroup > Column > ColumnGroup > Table
        if a.source > b.source {
            return Some(a.clone());
        }
        if b.source > a.source {
            return Some(b.clone());
        }
        
        // 6. Same priority - prefer first one (left/top in LTR)
        Some(a.clone())
    }
}

/// Get border information for a node
fn get_border_info<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &LayoutContext<T, Q>,
    node: &LayoutNode<T>,
    source: BorderSource,
) -> (BorderInfo, BorderInfo, BorderInfo, BorderInfo) {
    use azul_core::styled_dom::StyledNodeState;
    use azul_css::props::style::BorderStyle;
    use azul_css::props::basic::ColorU;
    use azul_css::props::basic::pixel::{ResolutionContext, PropertyContext, PhysicalSize};
    use crate::solver3::getters::{get_element_font_size, get_parent_font_size, get_root_font_size};
    
    let default_border = BorderInfo::new(
        0.0,
        BorderStyle::None,
        ColorU { r: 0, g: 0, b: 0, a: 0 },
        source,
    );
    
    if let Some(dom_id) = node.dom_node_id {
        let node_data = &ctx.styled_dom.node_data.as_container()[dom_id];
        let node_state = StyledNodeState::default();
        
        // Create resolution context for border-width (em/rem support, no % support)
        let element_font_size = get_element_font_size(ctx.styled_dom, dom_id, &node_state);
        let parent_font_size = get_parent_font_size(ctx.styled_dom, dom_id, &node_state);
        let root_font_size = get_root_font_size(ctx.styled_dom, &node_state);
        
        let resolution_context = ResolutionContext {
            element_font_size,
            parent_font_size,
            root_font_size,
            containing_block_size: PhysicalSize::new(0.0, 0.0), // Not used for border-width
            element_size: None, // Not used for border-width
            viewport_size: PhysicalSize::new(0.0, 0.0),
        };
        
        // Top border
        let top = if let Some(style) = ctx.styled_dom.css_property_cache.ptr.get_border_top_style(node_data, &dom_id, &node_state) {
            if let Some(style_val) = style.get_property() {
                let width = ctx.styled_dom.css_property_cache.ptr.get_border_top_width(node_data, &dom_id, &node_state)
                    .and_then(|w| w.get_property())
                    .map(|w| w.inner.resolve_with_context(&resolution_context, PropertyContext::BorderWidth))
                    .unwrap_or(0.0);
                let color = ctx.styled_dom.css_property_cache.ptr.get_border_top_color(node_data, &dom_id, &node_state)
                    .and_then(|c| c.get_property())
                    .map(|c| c.inner)
                    .unwrap_or(ColorU { r: 0, g: 0, b: 0, a: 255 });
                BorderInfo::new(width, style_val.inner, color, source)
            } else {
                default_border.clone()
            }
        } else {
            default_border.clone()
        };
        
        // Right border
        let right = if let Some(style) = ctx.styled_dom.css_property_cache.ptr.get_border_right_style(node_data, &dom_id, &node_state) {
            if let Some(style_val) = style.get_property() {
                let width = ctx.styled_dom.css_property_cache.ptr.get_border_right_width(node_data, &dom_id, &node_state)
                    .and_then(|w| w.get_property())
                    .map(|w| w.inner.resolve_with_context(&resolution_context, PropertyContext::BorderWidth))
                    .unwrap_or(0.0);
                let color = ctx.styled_dom.css_property_cache.ptr.get_border_right_color(node_data, &dom_id, &node_state)
                    .and_then(|c| c.get_property())
                    .map(|c| c.inner)
                    .unwrap_or(ColorU { r: 0, g: 0, b: 0, a: 255 });
                BorderInfo::new(width, style_val.inner, color, source)
            } else {
                default_border.clone()
            }
        } else {
            default_border.clone()
        };
        
        // Bottom border
        let bottom = if let Some(style) = ctx.styled_dom.css_property_cache.ptr.get_border_bottom_style(node_data, &dom_id, &node_state) {
            if let Some(style_val) = style.get_property() {
                let width = ctx.styled_dom.css_property_cache.ptr.get_border_bottom_width(node_data, &dom_id, &node_state)
                    .and_then(|w| w.get_property())
                    .map(|w| w.inner.resolve_with_context(&resolution_context, PropertyContext::BorderWidth))
                    .unwrap_or(0.0);
                let color = ctx.styled_dom.css_property_cache.ptr.get_border_bottom_color(node_data, &dom_id, &node_state)
                    .and_then(|c| c.get_property())
                    .map(|c| c.inner)
                    .unwrap_or(ColorU { r: 0, g: 0, b: 0, a: 255 });
                BorderInfo::new(width, style_val.inner, color, source)
            } else {
                default_border.clone()
            }
        } else {
            default_border.clone()
        };
        
        // Left border
        let left = if let Some(style) = ctx.styled_dom.css_property_cache.ptr.get_border_left_style(node_data, &dom_id, &node_state) {
            if let Some(style_val) = style.get_property() {
                let width = ctx.styled_dom.css_property_cache.ptr.get_border_left_width(node_data, &dom_id, &node_state)
                    .and_then(|w| w.get_property())
                    .map(|w| w.inner.resolve_with_context(&resolution_context, PropertyContext::BorderWidth))
                    .unwrap_or(0.0);
                let color = ctx.styled_dom.css_property_cache.ptr.get_border_left_color(node_data, &dom_id, &node_state)
                    .and_then(|c| c.get_property())
                    .map(|c| c.inner)
                    .unwrap_or(ColorU { r: 0, g: 0, b: 0, a: 255 });
                BorderInfo::new(width, style_val.inner, color, source)
            } else {
                default_border.clone()
            }
        } else {
            default_border.clone()
        };
        
        (top, right, bottom, left)
    } else {
        (default_border.clone(), default_border.clone(), default_border.clone(), default_border)
    }
}

/// Get the table-layout property for a table node
fn get_table_layout_property<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &LayoutContext<T, Q>,
    node: &LayoutNode<T>,
) -> azul_css::props::layout::LayoutTableLayout {
    use azul_css::props::layout::LayoutTableLayout;
    use azul_core::styled_dom::StyledNodeState;
    
    if let Some(dom_id) = node.dom_node_id {
        let node_data = &ctx.styled_dom.node_data.as_container()[dom_id];
        let node_state = StyledNodeState::default();
        
        if let Some(prop) = ctx.styled_dom.css_property_cache.ptr.get_table_layout(node_data, &dom_id, &node_state) {
            if let Some(value) = prop.get_property() {
                return *value;
            }
        }
    }
    
    LayoutTableLayout::Auto // Default
}

/// Get the border-collapse property for a table node
fn get_border_collapse_property<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &LayoutContext<T, Q>,
    node: &LayoutNode<T>,
) -> azul_css::props::layout::StyleBorderCollapse {
    use azul_css::props::layout::StyleBorderCollapse;
    use azul_core::styled_dom::StyledNodeState;
    
    if let Some(dom_id) = node.dom_node_id {
        let node_data = &ctx.styled_dom.node_data.as_container()[dom_id];
        let node_state = StyledNodeState::default();
        
        if let Some(prop) = ctx.styled_dom.css_property_cache.ptr.get_border_collapse(node_data, &dom_id, &node_state) {
            if let Some(value) = prop.get_property() {
                return *value;
            }
        }
    }
    
    StyleBorderCollapse::Separate // Default
}

/// Get the border-spacing property for a table node
fn get_border_spacing_property<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &LayoutContext<T, Q>,
    node: &LayoutNode<T>,
) -> azul_css::props::layout::LayoutBorderSpacing {
    use azul_css::props::layout::LayoutBorderSpacing;
    use azul_core::styled_dom::StyledNodeState;
    
    if let Some(dom_id) = node.dom_node_id {
        let node_data = &ctx.styled_dom.node_data.as_container()[dom_id];
        let node_state = StyledNodeState::default();
        
        if let Some(prop) = ctx.styled_dom.css_property_cache.ptr.get_border_spacing(node_data, &dom_id, &node_state) {
            if let Some(value) = prop.get_property() {
                return *value;
            }
        }
    }
    
    LayoutBorderSpacing::default() // Default: 0
}

/// CSS 2.2 Section 17.4 - Tables in the visual formatting model:
/// "The caption box is a block box that retains its own content, padding, border, and margin areas.
/// The caption-side property specifies the position of the caption box with respect to the table box."
///
/// Get the caption-side property for a table node.
/// Returns Top (default) or Bottom.
fn get_caption_side_property<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &LayoutContext<T, Q>,
    node: &LayoutNode<T>,
) -> azul_css::props::layout::StyleCaptionSide {
    use azul_css::props::layout::StyleCaptionSide;
    use azul_core::styled_dom::StyledNodeState;
    
    if let Some(dom_id) = node.dom_node_id {
        let node_data = &ctx.styled_dom.node_data.as_container()[dom_id];
        let node_state = StyledNodeState::default();
        
        if let Some(prop) = ctx.styled_dom.css_property_cache.ptr.get_caption_side(node_data, &dom_id, &node_state) {
            if let Some(value) = prop.get_property() {
                return *value;
            }
        }
    }
    
    StyleCaptionSide::Top // Default per CSS 2.2
}

/// CSS 2.2 Section 17.6 - Dynamic row and column effects:
/// "The 'visibility' value 'collapse' removes a row or column from display, but it has a different 
/// effect than 'visibility: hidden' on other elements. When a row or column is collapsed, the space 
/// normally occupied by the row or column is removed."
///
/// Check if a node has visibility:collapse set.
/// This is used for table rows and columns to optimize dynamic hiding.
fn is_visibility_collapsed<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &LayoutContext<T, Q>,
    node: &LayoutNode<T>,
) -> bool {
    use azul_css::props::style::StyleVisibility;
    use azul_core::styled_dom::StyledNodeState;
    
    if let Some(dom_id) = node.dom_node_id {
        let node_data = &ctx.styled_dom.node_data.as_container()[dom_id];
        let node_state = StyledNodeState::default();
        
        if let Some(prop) = ctx.styled_dom.css_property_cache.ptr.get_visibility(node_data, &dom_id, &node_state) {
            if let Some(value) = prop.get_property() {
                return matches!(value, StyleVisibility::Collapse);
            }
        }
    }
    
    false
}

/// CSS 2.2 Section 17.6.1.1 - Borders and Backgrounds around empty cells: the 'empty-cells' property
/// "In the separated borders model, the 'empty-cells' property controls the rendering of borders 
/// and backgrounds around cells that have no visible content. Empty means it has no children, or 
/// has children that are only collapsed whitespace."
///
/// Check if a table cell is empty (has no visible content).
/// This is used by the rendering pipeline to decide whether to paint borders/backgrounds
/// when empty-cells: hide is set in separated border model.
///
/// A cell is considered empty if:
/// - It has no children, OR
/// - It has children but no inline_layout_result (no rendered content)
///
/// Note: Full whitespace detection would require checking text content during rendering.
/// This function provides a basic check suitable for layout phase.
fn is_cell_empty<T: ParsedFontTrait>(
    tree: &LayoutTree<T>,
    cell_index: usize,
) -> bool {
    let cell_node = match tree.get(cell_index) {
        Some(node) => node,
        None => return true, // Invalid cell is considered empty
    };
    
    // No children = empty
    if cell_node.children.is_empty() {
        return true;
    }
    
    // If cell has an inline layout result, check if it's empty
    if let Some(ref inline_result) = cell_node.inline_layout_result {
        // Check if inline layout has any rendered content
        // Empty inline layouts have no items (glyphs/fragments)
        // Note: This is a heuristic - full detection requires text content analysis
        return inline_result.items.is_empty();
    }
    
    // Check if all children have no content
    // A more thorough check would recursively examine all descendants
    // For now, we use a simple heuristic: if there are children, assume not empty
    // unless proven otherwise by inline_layout_result
    
    // Cell with children but no inline layout = likely has block-level content = not empty
    false
}

fn layout_table_fc<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &mut LayoutContext<T, Q>,
    tree: &mut LayoutTree<T>,
    text_cache: &mut text3::cache::LayoutCache<T>,
    node_index: usize,
    constraints: &LayoutConstraints,
) -> Result<LayoutOutput> {
    ctx.debug_log("Laying out table");
    
    ctx.debug_table_layout(format!("node_index={}, available_size={:?}, writing_mode={:?}", 
        node_index, constraints.available_size, constraints.writing_mode));
    
    // Multi-pass table layout algorithm:
    // 1. Analyze table structure - identify rows, cells, columns
    // 2. Determine table-layout property (fixed vs auto)
    // 3. Calculate column widths
    // 4. Layout cells and calculate row heights
    // 5. Position cells in final grid
    
    // Get the table node to read CSS properties
    let table_node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?.clone();
    
    // Calculate the table's border-box width for column distribution
    // This accounts for the table's own width property (e.g., width: 100%)
    let table_border_box_width = if let Some(dom_id) = table_node.dom_node_id {
        // Use calculate_used_size_for_node to resolve table width (respects width:100%)
        let intrinsic = table_node.intrinsic_sizes.clone().unwrap_or_default();
        let containing_block_size = LogicalSize {
            width: constraints.available_size.width,
            height: constraints.available_size.height,
        };
        
        let table_size = crate::solver3::sizing::calculate_used_size_for_node(
            ctx.styled_dom,
            Some(dom_id),
            containing_block_size,
            intrinsic,
            &table_node.box_props,
        )?;
        
        table_size.width
    } else {
        constraints.available_size.width
    };
    
    // Subtract padding and border to get content-box width for column distribution
    let table_content_box_width = {
        let padding_width = table_node.box_props.padding.left + table_node.box_props.padding.right;
        let border_width = table_node.box_props.border.left + table_node.box_props.border.right;
        (table_border_box_width - padding_width - border_width).max(0.0)
    };
    
    ctx.debug_table_layout("========== TABLE LAYOUT DEBUG ==========");
    ctx.debug_table_layout(format!("Node index: {}", node_index));
    ctx.debug_table_layout(format!("Available size from parent: {:.2} x {:.2}", 
        constraints.available_size.width, constraints.available_size.height));
    ctx.debug_table_layout(format!("Table border-box width: {:.2}", table_border_box_width));
    ctx.debug_table_layout(format!("Table content-box width: {:.2}", table_content_box_width));
    ctx.debug_table_layout(format!("Table padding: L={:.2} R={:.2}", 
        table_node.box_props.padding.left, table_node.box_props.padding.right));
    ctx.debug_table_layout(format!("Table border: L={:.2} R={:.2}", 
        table_node.box_props.border.left, table_node.box_props.border.right));
    ctx.debug_table_layout("=======================================");
    
    // Phase 1: Analyze table structure
    let mut table_ctx = analyze_table_structure(tree, node_index, ctx)?;
    
    // Phase 2: Read CSS properties and determine layout algorithm
    use azul_css::props::layout::LayoutTableLayout;
    let table_layout = get_table_layout_property(ctx, &table_node);
    table_ctx.use_fixed_layout = matches!(table_layout, LayoutTableLayout::Fixed);
    
    // Read border properties
    table_ctx.border_collapse = get_border_collapse_property(ctx, &table_node);
    table_ctx.border_spacing = get_border_spacing_property(ctx, &table_node);
    
    ctx.debug_log(&format!(
        "Table layout: {:?}, border-collapse: {:?}, border-spacing: {:?}",
        table_layout, table_ctx.border_collapse, table_ctx.border_spacing
    ));
    
    // Phase 3: Calculate column widths
    if table_ctx.use_fixed_layout {
        // DEBUG: Log available width passed into fixed column calculation
        ctx.debug_table_layout(format!("FIXED layout: table_content_box_width={:.2}",
            table_content_box_width));
        calculate_column_widths_fixed(ctx, &mut table_ctx, table_content_box_width);
    } else {
        // Pass table_content_box_width for column distribution in auto layout
        calculate_column_widths_auto_with_width(&mut table_ctx, tree, text_cache, ctx, constraints, table_content_box_width)?;
    }
    
    ctx.debug_table_layout("After column width calculation:");
    ctx.debug_table_layout(format!("  Number of columns: {}", table_ctx.columns.len()));
    for (i, col) in table_ctx.columns.iter().enumerate() {
        ctx.debug_table_layout(format!("  Column {}: width={:.2}", i, col.computed_width.unwrap_or(0.0)));
    }
    let total_col_width: f32 = table_ctx.columns.iter().filter_map(|c| c.computed_width).sum();
    ctx.debug_table_layout(format!("  Total column width: {:.2}", total_col_width));
    
    // Phase 4: Calculate row heights based on cell content
    calculate_row_heights(&mut table_ctx, tree, text_cache, ctx, constraints)?;
    
    // Phase 5: Position cells in final grid and collect positions
    let mut cell_positions = position_table_cells(&mut table_ctx, tree, ctx, node_index, constraints)?;
    
    // Calculate final table size including border-spacing
    let mut table_width: f32 = table_ctx.columns.iter()
        .filter_map(|col| col.computed_width)
        .sum();
    let mut table_height: f32 = table_ctx.row_heights.iter().sum();
    
    ctx.debug_table_layout(format!("After calculate_row_heights: table_height={:.2}, row_heights={:?}", 
        table_height, table_ctx.row_heights));
    
    // Add border-spacing to table size if border-collapse is separate
    use azul_css::props::layout::StyleBorderCollapse;
    if table_ctx.border_collapse == StyleBorderCollapse::Separate {
        use azul_css::props::basic::{ResolutionContext, PropertyContext, PhysicalSize};
        use crate::solver3::getters::{get_element_font_size, get_parent_font_size, get_root_font_size};
        
        let styled_dom = ctx.styled_dom;
        let table_id = tree.nodes[node_index].dom_node_id.unwrap();
        let table_state = &styled_dom.styled_nodes.as_container()[table_id].state;
        
        let spacing_context = ResolutionContext {
            element_font_size: get_element_font_size(styled_dom, table_id, table_state),
            parent_font_size: get_parent_font_size(styled_dom, table_id, table_state),
            root_font_size: get_root_font_size(styled_dom, table_state),
            containing_block_size: PhysicalSize::new(0.0, 0.0),
            element_size: None,
            viewport_size: PhysicalSize::new(0.0, 0.0), // TODO: Get actual DPI scale from ctx
        };
        
        let h_spacing = table_ctx.border_spacing.horizontal.resolve_with_context(&spacing_context, PropertyContext::Other);
        let v_spacing = table_ctx.border_spacing.vertical.resolve_with_context(&spacing_context, PropertyContext::Other);
        
        // Add spacing: left + (n-1 between columns) + right = n+1 spacings
        let num_cols = table_ctx.columns.len();
        if num_cols > 0 {
            table_width += h_spacing * (num_cols + 1) as f32;
        }
        
        // Add spacing: top + (n-1 between rows) + bottom = n+1 spacings
        if table_ctx.num_rows > 0 {
            table_height += v_spacing * (table_ctx.num_rows + 1) as f32;
        }
    }
    
    // CSS 2.2 Section 17.4: Layout and position the caption if present
    // "The caption box is a block box that retains its own content, padding, border, and margin areas."
    let caption_side = get_caption_side_property(ctx, &table_node);
    let mut caption_height = 0.0;
    let mut table_y_offset = 0.0;
    
    if let Some(caption_idx) = table_ctx.caption_index {
        ctx.debug_log(&format!("Laying out caption with caption-side: {:?}", caption_side));
        
        // Layout caption as a block with the table's width as available width
        let caption_constraints = LayoutConstraints {
            available_size: LogicalSize {
                width: table_width,
                height: constraints.available_size.height,
            },
            writing_mode: constraints.writing_mode,
            bfc_state: None, // Caption creates its own BFC
            text_align: constraints.text_align,
        };
        
        // Layout the caption node
        let mut empty_float_cache = std::collections::BTreeMap::new();
        let caption_output = layout_formatting_context(ctx, tree, text_cache, caption_idx, &caption_constraints, &mut empty_float_cache)?;
        caption_height = caption_output.overflow_size.height;
        
        // Position caption based on caption-side property
        use azul_css::props::layout::StyleCaptionSide;
        let caption_position = match caption_side {
            StyleCaptionSide::Top => {
                // Caption on top: position at y=0, table starts below caption
                table_y_offset = caption_height;
                LogicalPosition { x: 0.0, y: 0.0 }
            }
            StyleCaptionSide::Bottom => {
                // Caption on bottom: table starts at y=0, caption below table
                LogicalPosition { x: 0.0, y: table_height }
            }
        };
        
        // Add caption position to the positions map
        cell_positions.insert(caption_idx, caption_position);
        
        ctx.debug_log(&format!("Caption positioned at x={:.2}, y={:.2}, height={:.2}", 
                             caption_position.x, caption_position.y, caption_height));
    }
    
    // Adjust all table cell positions if caption is on top
    if table_y_offset > 0.0 {
        ctx.debug_log(&format!("Adjusting table cells by y offset: {:.2}", table_y_offset));
        
        // Adjust cell positions in the map
        for cell_info in &table_ctx.cells {
            if let Some(pos) = cell_positions.get_mut(&cell_info.node_index) {
                pos.y += table_y_offset;
            }
        }
    }
    
    // Total table height includes caption
    let total_height = table_height + caption_height;
    
    ctx.debug_table_layout("Final table dimensions:");
    ctx.debug_table_layout(format!("  Content width (columns): {:.2}", table_width));
    ctx.debug_table_layout(format!("  Content height (rows): {:.2}", table_height));
    ctx.debug_table_layout(format!("  Caption height: {:.2}", caption_height));
    ctx.debug_table_layout(format!("  Total height: {:.2}", total_height));
    ctx.debug_table_layout("========== END TABLE DEBUG ==========");
    
    // Create output with the table's final size and cell positions
    let output = LayoutOutput {
        overflow_size: LogicalSize {
            width: table_width,
            height: total_height,
        },
        positions: cell_positions, // Cell positions calculated in position_table_cells
        baseline: None, // Tables don't have a baseline
    };
    
    Ok(output)
}

/// Analyze the table structure to identify rows, cells, and columns
fn analyze_table_structure<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    tree: &LayoutTree<T>,
    table_index: usize,
    ctx: &mut LayoutContext<T, Q>,
) -> Result<TableLayoutContext> {
    let mut table_ctx = TableLayoutContext::new();
    
    let table_node = tree.get(table_index).ok_or(LayoutError::InvalidTree)?;
    
    // CSS 2.2 Section 17.4: A table may have one table-caption child.
    // Traverse children to find caption, columns/colgroups, rows, and row groups
    for &child_idx in &table_node.children {
        if let Some(child) = tree.get(child_idx) {
            // Check if this is a table caption
            if matches!(child.formatting_context, FormattingContext::TableCaption) {
                ctx.debug_log(&format!("Found table caption at index {}", child_idx));
                table_ctx.caption_index = Some(child_idx);
                continue;
            }
            
            // CSS 2.2 Section 17.2: Check for column groups
            if matches!(child.formatting_context, FormattingContext::TableColumnGroup) {
                analyze_table_colgroup(tree, child_idx, &mut table_ctx, ctx)?;
                continue;
            }
            
            // Check if this is a table row or row group
            match child.formatting_context {
                FormattingContext::TableRow => {
                    analyze_table_row(tree, child_idx, &mut table_ctx, ctx)?;
                }
                FormattingContext::TableRowGroup => {
                    // Process rows within the row group
                    for &row_idx in &child.children {
                        if let Some(row) = tree.get(row_idx) {
                            if matches!(row.formatting_context, FormattingContext::TableRow) {
                                analyze_table_row(tree, row_idx, &mut table_ctx, ctx)?;
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    
    ctx.debug_log(&format!("Table structure: {} rows, {} columns, {} cells{}", 
                          table_ctx.num_rows, 
                          table_ctx.columns.len(), 
                          table_ctx.cells.len(),
                          if table_ctx.caption_index.is_some() { ", has caption" } else { "" }));
    
    Ok(table_ctx)
}

/// Analyze a table column group to identify columns and track collapsed columns
/// CSS 2.2 Section 17.2: Column groups contain columns
/// CSS 2.2 Section 17.6: Columns can have visibility:collapse
fn analyze_table_colgroup<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    tree: &LayoutTree<T>,
    colgroup_index: usize,
    table_ctx: &mut TableLayoutContext,
    ctx: &mut LayoutContext<T, Q>,
) -> Result<()> {
    let colgroup_node = tree.get(colgroup_index).ok_or(LayoutError::InvalidTree)?;
    
    // Check if the colgroup itself has visibility:collapse
    if is_visibility_collapsed(ctx, colgroup_node) {
        ctx.debug_log(&format!("Column group at index {} has visibility:collapse", colgroup_index));
        // All columns in this group should be collapsed
        // For now, just mark the group (actual column indices will be determined later)
    }
    
    // Check for individual column elements within the group
    for &col_idx in &colgroup_node.children {
        if let Some(col_node) = tree.get(col_idx) {
            // Note: Individual columns don't have a FormattingContext::TableColumn
            // They are represented as children of TableColumnGroup
            // Check visibility:collapse on each column
            if is_visibility_collapsed(ctx, col_node) {
                // We need to determine the actual column index this represents
                // For now, we'll track it during cell analysis
                ctx.debug_log(&format!("Column at index {} has visibility:collapse", col_idx));
            }
        }
    }
    
    Ok(())
}

/// Analyze a table row to identify cells and update column count
fn analyze_table_row<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    tree: &LayoutTree<T>,
    row_index: usize,
    table_ctx: &mut TableLayoutContext,
    ctx: &mut LayoutContext<T, Q>,
) -> Result<()> {
    let row_node = tree.get(row_index).ok_or(LayoutError::InvalidTree)?;
    let row_num = table_ctx.num_rows;
    table_ctx.num_rows += 1;
    
    // CSS 2.2 Section 17.6: Check if this row has visibility:collapse
    if is_visibility_collapsed(ctx, row_node) {
        ctx.debug_log(&format!("Row {} has visibility:collapse", row_num));
        table_ctx.collapsed_rows.insert(row_num);
    }
    
    let mut col_index = 0;
    
    for &cell_idx in &row_node.children {
        if let Some(cell) = tree.get(cell_idx) {
            if matches!(cell.formatting_context, FormattingContext::TableCell) {
                // Get colspan and rowspan (TODO: from CSS properties)
                let colspan = 1; // TODO: Get from CSS
                let rowspan = 1; // TODO: Get from CSS
                
                let cell_info = TableCellInfo {
                    node_index: cell_idx,
                    column: col_index,
                    colspan,
                    row: row_num,
                    rowspan,
                };
                                
                table_ctx.cells.push(cell_info);
                
                // Update column count
                let max_col = col_index + colspan;
                while table_ctx.columns.len() < max_col {
                    table_ctx.columns.push(TableColumnInfo {
                        min_width: 0.0,
                        max_width: 0.0,
                        computed_width: None,
                    });
                }
                
                col_index += colspan;
            }
        }
    }
    
    Ok(())
}

/// Calculate column widths using the fixed table layout algorithm
/// CSS 2.2 Section 17.5.2.1: In fixed table layout, the table width is not dependent on cell contents
/// CSS 2.2 Section 17.6: Columns with visibility:collapse are excluded from width calculations
fn calculate_column_widths_fixed<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &mut LayoutContext<T, Q>,
    table_ctx: &mut TableLayoutContext,
    available_width: f32,
) {
    ctx.debug_table_layout(format!("calculate_column_widths_fixed: num_cols={}, available_width={:.2}",
        table_ctx.columns.len(), available_width));
    
    // Fixed layout: distribute width equally among non-collapsed columns
    // TODO: Respect column width properties and first-row cell widths
    let num_cols = table_ctx.columns.len();
    if num_cols == 0 {
        return;
    }
    
    // Count non-collapsed columns
    let num_visible_cols = num_cols - table_ctx.collapsed_columns.len();
    if num_visible_cols == 0 {
        // All columns collapsed - set all to zero width
        for col in &mut table_ctx.columns {
            col.computed_width = Some(0.0);
        }
        return;
    }
    
    // Distribute width only among visible columns
    let col_width = available_width / num_visible_cols as f32;
    for (col_idx, col) in table_ctx.columns.iter_mut().enumerate() {
        if table_ctx.collapsed_columns.contains(&col_idx) {
            col.computed_width = Some(0.0);
        } else {
            col.computed_width = Some(col_width);
        }
    }
}

/// Measure a cell's minimum content width (with maximum wrapping)
fn measure_cell_min_content_width<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &mut LayoutContext<T, Q>,
    tree: &mut LayoutTree<T>,
    text_cache: &mut text3::cache::LayoutCache<T>,
    cell_index: usize,
    constraints: &LayoutConstraints,
) -> Result<f32> {
    // For intrinsic sizing in auto table layout, we need to measure content width,
    // not percentage-based widths. Use table's available width as the containing block.
    // CSS 2.2 Section 17.5.2.2: "Calculate the minimum content width (MCW) of each cell"
    let min_constraints = LayoutConstraints {
        available_size: LogicalSize {
            width: constraints.available_size.width, // Use table's available width as containing block
            height: f32::INFINITY,
        },
        writing_mode: constraints.writing_mode,
        bfc_state: None, // Don't propagate BFC state for measurement
        text_align: constraints.text_align,
    };
    
    let mut temp_positions = BTreeMap::new();
    let mut temp_scrollbar_reflow = false;
    let mut temp_float_cache = std::collections::BTreeMap::new();
    
    crate::solver3::cache::calculate_layout_for_subtree(
        ctx,
        tree,
        text_cache,
        cell_index,
        LogicalPosition::zero(),
        min_constraints.available_size,
        &mut temp_positions,
        &mut temp_scrollbar_reflow,
        &mut temp_float_cache,
    )?;
    
    let cell_node = tree.get(cell_index).ok_or(LayoutError::InvalidTree)?;
    let size = cell_node.used_size.unwrap_or_default();
    
    // Add padding and border to get the total minimum width
    let padding = &cell_node.box_props.padding;
    let border = &cell_node.box_props.border;
    let writing_mode = constraints.writing_mode;
    
    let min_width = size.width
        + padding.cross_start(writing_mode)
        + padding.cross_end(writing_mode)
        + border.cross_start(writing_mode)
        + border.cross_end(writing_mode);
    
    Ok(min_width)
}

/// Measure a cell's maximum content width (without wrapping)
fn measure_cell_max_content_width<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &mut LayoutContext<T, Q>,
    tree: &mut LayoutTree<T>,
    text_cache: &mut text3::cache::LayoutCache<T>,
    cell_index: usize,
    constraints: &LayoutConstraints,
) -> Result<f32> {
    // For intrinsic sizing in auto table layout, use the table's available width as containing block.
    // CSS 2.2 Section 17.5.2.2: "Calculate the maximum content width (MCW) of each cell"
    let max_constraints = LayoutConstraints {
        available_size: LogicalSize {
            width: constraints.available_size.width, // Use table's available width as containing block
            height: f32::INFINITY,
        },
        writing_mode: constraints.writing_mode,
        bfc_state: None, // Don't propagate BFC state for measurement
        text_align: constraints.text_align,
    };
    
    let mut temp_positions = BTreeMap::new();
    let mut temp_scrollbar_reflow = false;
    let mut temp_float_cache = std::collections::BTreeMap::new();
    
    crate::solver3::cache::calculate_layout_for_subtree(
        ctx,
        tree,
        text_cache,
        cell_index,
        LogicalPosition::zero(),
        max_constraints.available_size,
        &mut temp_positions,
        &mut temp_scrollbar_reflow,
        &mut temp_float_cache,
    )?;
    
    let cell_node = tree.get(cell_index).ok_or(LayoutError::InvalidTree)?;
    let size = cell_node.used_size.unwrap_or_default();
    
    // Add padding and border to get the total maximum width
    let padding = &cell_node.box_props.padding;
    let border = &cell_node.box_props.border;
    let writing_mode = constraints.writing_mode;
    
    let max_width = size.width
        + padding.cross_start(writing_mode)
        + padding.cross_end(writing_mode)
        + border.cross_start(writing_mode)
        + border.cross_end(writing_mode);
    
    Ok(max_width)
}

/// Calculate column widths using the auto table layout algorithm
fn calculate_column_widths_auto<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    table_ctx: &mut TableLayoutContext,
    tree: &mut LayoutTree<T>,
    text_cache: &mut text3::cache::LayoutCache<T>,
    ctx: &mut LayoutContext<T, Q>,
    constraints: &LayoutConstraints,
) -> Result<()> {
    calculate_column_widths_auto_with_width(table_ctx, tree, text_cache, ctx, constraints, constraints.available_size.width)
}

/// Calculate column widths using the auto table layout algorithm with explicit table width
fn calculate_column_widths_auto_with_width<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    table_ctx: &mut TableLayoutContext,
    tree: &mut LayoutTree<T>,
    text_cache: &mut text3::cache::LayoutCache<T>,
    ctx: &mut LayoutContext<T, Q>,
    constraints: &LayoutConstraints,
    table_width: f32,
) -> Result<()> {
    // Auto layout: calculate min/max content width for each cell
    let num_cols = table_ctx.columns.len();
    if num_cols == 0 {
        return Ok(());
    }
    
    // Step 1: Measure all cells to determine column min/max widths
    // CSS 2.2 Section 17.6: Skip cells in collapsed columns
    for cell_info in &table_ctx.cells {
        // Skip cells in collapsed columns
        if table_ctx.collapsed_columns.contains(&cell_info.column) {
            continue;
        }
        
        // Skip cells that span into collapsed columns
        let mut spans_collapsed = false;
        for col_offset in 0..cell_info.colspan {
            if table_ctx.collapsed_columns.contains(&(cell_info.column + col_offset)) {
                spans_collapsed = true;
                break;
            }
        }
        if spans_collapsed {
            continue;
        }
        
        let min_width = measure_cell_min_content_width(
            ctx,
            tree,
            text_cache,
            cell_info.node_index,
            constraints,
        )?;
        
        let max_width = measure_cell_max_content_width(
            ctx,
            tree,
            text_cache,
            cell_info.node_index,
            constraints,
        )?;
        
        // Handle single-column cells
        if cell_info.colspan == 1 {
            let col = &mut table_ctx.columns[cell_info.column];
            col.min_width = col.min_width.max(min_width);
            col.max_width = col.max_width.max(max_width);
        } else {
            // Handle multi-column cells (colspan > 1)
            // Distribute the cell's min/max width across the spanned columns
            distribute_cell_width_across_columns(
                &mut table_ctx.columns,
                cell_info.column,
                cell_info.colspan,
                min_width,
                max_width,
                &table_ctx.collapsed_columns,
            );
        }
    }
    
    // Step 2: Calculate final column widths based on available space
    // Exclude collapsed columns from total width calculations
    let total_min_width: f32 = table_ctx.columns.iter()
        .enumerate()
        .filter(|(idx, _)| !table_ctx.collapsed_columns.contains(idx))
        .map(|(_, c)| c.min_width)
        .sum();
    let total_max_width: f32 = table_ctx.columns.iter()
        .enumerate()
        .filter(|(idx, _)| !table_ctx.collapsed_columns.contains(idx))
        .map(|(_, c)| c.max_width)
        .sum();
    let available_width = table_width; // Use table's content-box width, not constraints
    
    ctx.debug_table_layout(format!("calculate_column_widths_auto: min={:.2}, max={:.2}, table_width={:.2}", 
        total_min_width, total_max_width, table_width));
    
    // Handle infinity and NaN cases
    if !total_max_width.is_finite() || !available_width.is_finite() {
        // If max_width is infinite or unavailable, distribute available width equally
        let num_non_collapsed = table_ctx.columns.len() - table_ctx.collapsed_columns.len();
        let width_per_column = if num_non_collapsed > 0 {
            available_width / num_non_collapsed as f32
        } else {
            0.0
        };
        
        for (col_idx, col) in table_ctx.columns.iter_mut().enumerate() {
            if table_ctx.collapsed_columns.contains(&col_idx) {
                col.computed_width = Some(0.0);
            } else {
                // Use the larger of min_width and equal distribution
                col.computed_width = Some(col.min_width.max(width_per_column));
            }
        }
    } else if available_width >= total_max_width {
        // Case 1: More space than max-content - distribute excess proportionally
        // CSS 2.1 Section 17.5.2.2: Distribute extra space proportionally to max-content widths
        let excess_width = available_width - total_max_width;
        
        // First pass: collect column info (max_width) to avoid borrowing issues
        let column_info: Vec<(usize, f32, bool)> = table_ctx.columns.iter()
            .enumerate()
            .map(|(idx, c)| (idx, c.max_width, table_ctx.collapsed_columns.contains(&idx)))
            .collect();
        
        // Calculate total weight for proportional distribution (use max_width as weight)
        let total_weight: f32 = column_info.iter()
            .filter(|(_, _, is_collapsed)| !is_collapsed)
            .map(|(_, max_w, _)| max_w.max(1.0)) // Avoid division by zero
            .sum();
        
        let num_non_collapsed = column_info.iter().filter(|(_, _, is_collapsed)| !is_collapsed).count();
        
        // Second pass: set computed widths
        for (col_idx, max_width, is_collapsed) in column_info {
            let col = &mut table_ctx.columns[col_idx];
            if is_collapsed {
                col.computed_width = Some(0.0);
            } else {
                // Start with max-content width, then add proportional share of excess
                let weight_factor = if total_weight > 0.0 {
                    max_width.max(1.0) / total_weight
                } else {
                    // If all columns have 0 max_width, distribute equally
                    1.0 / num_non_collapsed.max(1) as f32
                };
                
                let final_width = max_width + (excess_width * weight_factor);
                col.computed_width = Some(final_width);
            }
        }
    } else if available_width >= total_min_width {
        // Case 2: Between min and max - interpolate proportionally
        // Avoid division by zero if min == max
        let scale = if total_max_width > total_min_width {
            (available_width - total_min_width) / (total_max_width - total_min_width)
        } else {
            0.0 // If min == max, just use min width
        };
        for (col_idx, col) in table_ctx.columns.iter_mut().enumerate() {
            if table_ctx.collapsed_columns.contains(&col_idx) {
                col.computed_width = Some(0.0);
            } else {
                let interpolated = col.min_width + (col.max_width - col.min_width) * scale;
                col.computed_width = Some(interpolated);
            }
        }
    } else {
        // Case 3: Not enough space - scale down from min widths
        let scale = available_width / total_min_width;
        for (col_idx, col) in table_ctx.columns.iter_mut().enumerate() {
            if table_ctx.collapsed_columns.contains(&col_idx) {
                col.computed_width = Some(0.0);
            } else {
                col.computed_width = Some(col.min_width * scale);
            }
        }
    }
    
    Ok(())
}

/// Distribute a multi-column cell's width across the columns it spans
fn distribute_cell_width_across_columns(
    columns: &mut [TableColumnInfo],
    start_col: usize,
    colspan: usize,
    cell_min_width: f32,
    cell_max_width: f32,
    collapsed_columns: &std::collections::HashSet<usize>,
) {
    let end_col = start_col + colspan;
    if end_col > columns.len() {
        return;
    }
    
    // Calculate current total of spanned non-collapsed columns
    let current_min_total: f32 = columns[start_col..end_col].iter()
        .enumerate()
        .filter(|(idx, _)| !collapsed_columns.contains(&(start_col + idx)))
        .map(|(_, c)| c.min_width)
        .sum();
    let current_max_total: f32 = columns[start_col..end_col].iter()
        .enumerate()
        .filter(|(idx, _)| !collapsed_columns.contains(&(start_col + idx)))
        .map(|(_, c)| c.max_width)
        .sum();
    
    // Count non-collapsed columns in the span
    let num_visible_cols = (start_col..end_col)
        .filter(|idx| !collapsed_columns.contains(idx))
        .count();
    
    if num_visible_cols == 0 {
        return; // All spanned columns are collapsed
    }
    
    // Only distribute if the cell needs more space than currently available
    if cell_min_width > current_min_total {
        let extra_min = cell_min_width - current_min_total;
        let per_col = extra_min / num_visible_cols as f32;
        for (idx, col) in columns[start_col..end_col].iter_mut().enumerate() {
            if !collapsed_columns.contains(&(start_col + idx)) {
                col.min_width += per_col;
            }
        }
    }
    
    if cell_max_width > current_max_total {
        let extra_max = cell_max_width - current_max_total;
        let per_col = extra_max / num_visible_cols as f32;
        for (idx, col) in columns[start_col..end_col].iter_mut().enumerate() {
            if !collapsed_columns.contains(&(start_col + idx)) {
                col.max_width += per_col;
            }
        }
    }
}

/// Layout a cell with its computed column width to determine its content height
fn layout_cell_for_height<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &mut LayoutContext<T, Q>,
    tree: &mut LayoutTree<T>,
    text_cache: &mut text3::cache::LayoutCache<T>,
    cell_index: usize,
    cell_width: f32,
    constraints: &LayoutConstraints,
) -> Result<f32> {
    let cell_node = tree.get(cell_index).ok_or(LayoutError::InvalidTree)?;
    let cell_dom_id = cell_node.dom_node_id.ok_or(LayoutError::InvalidTree)?;
    
    // Check if cell has text content directly in DOM (not in LayoutTree)
    // Text nodes are intentionally not included in LayoutTree per CSS spec,
    // but we need to measure them for table cell height calculation.
    let has_text_children = cell_dom_id
        .az_children(&ctx.styled_dom.node_hierarchy.as_container())
        .any(|child_id| {
            let node_data = &ctx.styled_dom.node_data.as_container()[child_id];
            matches!(node_data.get_node_type(), NodeType::Text(_))
        });
    
    ctx.debug_table_layout(format!("layout_cell_for_height: cell_index={}, has_text_children={}", 
        cell_index, has_text_children));
    
    // Get padding and border to calculate content width
    let cell_node = tree.get(cell_index).ok_or(LayoutError::InvalidTree)?;
    let padding = &cell_node.box_props.padding;
    let border = &cell_node.box_props.border;
    let writing_mode = constraints.writing_mode;
    
    // cell_width is the border-box width (includes padding/border from column width calculation)
    // but layout functions need content-box width
    let content_width = cell_width
        - padding.cross_start(writing_mode)
        - padding.cross_end(writing_mode)
        - border.cross_start(writing_mode)
        - border.cross_end(writing_mode);
    
    ctx.debug_table_layout(format!("Cell width: border_box={:.2}, content_box={:.2}", 
        cell_width, content_width));
    
    let content_height = if has_text_children {
        // Cell contains text - use IFC to measure it
        // This is the key fix: IFC traverses DOM to find text nodes
        ctx.debug_table_layout("Using IFC to measure text content");
        
        let cell_constraints = LayoutConstraints {
            available_size: LogicalSize {
                width: content_width,  // Use content width, not border-box width
                height: f32::INFINITY,
            },
            writing_mode: constraints.writing_mode,
            bfc_state: None,
            text_align: constraints.text_align,
        };
        
        let output = layout_ifc(ctx, text_cache, tree, cell_index, &cell_constraints)?;
        
        ctx.debug_table_layout(format!("IFC returned height={:.2}", output.overflow_size.height));
        
        output.overflow_size.height
    } else {
        // Cell contains block-level children or is empty - use regular layout
        ctx.debug_table_layout("Using regular layout for block children");
        
        let cell_constraints = LayoutConstraints {
            available_size: LogicalSize {
                width: content_width,  // Use content width, not border-box width
                height: f32::INFINITY,
            },
            writing_mode: constraints.writing_mode,
            bfc_state: None,
            text_align: constraints.text_align,
        };
        
        let mut temp_positions = BTreeMap::new();
        let mut temp_scrollbar_reflow = false;
        let mut temp_float_cache = std::collections::BTreeMap::new();
        
        crate::solver3::cache::calculate_layout_for_subtree(
            ctx,
            tree,
            text_cache,
            cell_index,
            LogicalPosition::zero(),
            cell_constraints.available_size,
            &mut temp_positions,
            &mut temp_scrollbar_reflow,
            &mut temp_float_cache,
        )?;
        
        let cell_node = tree.get(cell_index).ok_or(LayoutError::InvalidTree)?;
        cell_node.used_size.unwrap_or_default().height
    };
    
    // Add padding and border to get the total height
    let cell_node = tree.get(cell_index).ok_or(LayoutError::InvalidTree)?;
    let padding = &cell_node.box_props.padding;
    let border = &cell_node.box_props.border;
    let writing_mode = constraints.writing_mode;
    
    let total_height = content_height
        + padding.main_start(writing_mode)
        + padding.main_end(writing_mode)
        + border.main_start(writing_mode)
        + border.main_end(writing_mode);
    
    ctx.debug_table_layout(format!("Cell total height: cell_index={}, content={:.2}, padding/border={:.2}, total={:.2}", 
        cell_index, content_height,
        padding.main_start(writing_mode) + padding.main_end(writing_mode) + border.main_start(writing_mode) + border.main_end(writing_mode),
        total_height));
    
    Ok(total_height)
}

/// Calculate row heights based on cell content after column widths are determined
fn calculate_row_heights<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    table_ctx: &mut TableLayoutContext,
    tree: &mut LayoutTree<T>,
    text_cache: &mut text3::cache::LayoutCache<T>,
    ctx: &mut LayoutContext<T, Q>,
    constraints: &LayoutConstraints,
) -> Result<()> {
    ctx.debug_table_layout(format!("calculate_row_heights: num_rows={}, available_size={:?}", 
        table_ctx.num_rows, constraints.available_size));
    
    // Initialize row heights
    table_ctx.row_heights = vec![0.0; table_ctx.num_rows];
    
    // CSS 2.2 Section 17.6: Set collapsed rows to height 0
    for &row_idx in &table_ctx.collapsed_rows {
        if row_idx < table_ctx.row_heights.len() {
            table_ctx.row_heights[row_idx] = 0.0;
        }
    }
    
    // First pass: Calculate heights for cells that don't span multiple rows
    for cell_info in &table_ctx.cells {
        // Skip cells in collapsed rows
        if table_ctx.collapsed_rows.contains(&cell_info.row) {
            continue;
        }
        
        // Get the cell's width (sum of column widths if colspan > 1)
        let mut cell_width = 0.0;
        for col_idx in cell_info.column..(cell_info.column + cell_info.colspan) {
            if let Some(col) = table_ctx.columns.get(col_idx) {
                if let Some(width) = col.computed_width {
                    cell_width += width;
                }
            }
        }
        
        ctx.debug_table_layout(format!("Cell layout: node_index={}, row={}, col={}, width={:.2}", 
            cell_info.node_index, cell_info.row, cell_info.column, cell_width));
        
        // Layout the cell to get its height
        let cell_height = layout_cell_for_height(
            ctx,
            tree,
            text_cache,
            cell_info.node_index,
            cell_width,
            constraints,
        )?;
        
        ctx.debug_table_layout(format!("Cell height calculated: node_index={}, height={:.2}", 
            cell_info.node_index, cell_height));
        
        // For single-row cells, update the row height
        if cell_info.rowspan == 1 {
            let current_height = table_ctx.row_heights[cell_info.row];
            table_ctx.row_heights[cell_info.row] = current_height.max(cell_height);
        }
    }
    
    // Second pass: Handle cells that span multiple rows (rowspan > 1)
    for cell_info in &table_ctx.cells {
        // Skip cells that start in collapsed rows
        if table_ctx.collapsed_rows.contains(&cell_info.row) {
            continue;
        }
        
        if cell_info.rowspan > 1 {
            // Get the cell's width
            let mut cell_width = 0.0;
            for col_idx in cell_info.column..(cell_info.column + cell_info.colspan) {
                if let Some(col) = table_ctx.columns.get(col_idx) {
                    if let Some(width) = col.computed_width {
                        cell_width += width;
                    }
                }
            }
            
            // Layout the cell to get its height
            let cell_height = layout_cell_for_height(
                ctx,
                tree,
                text_cache,
                cell_info.node_index,
                cell_width,
                constraints,
            )?;
            
            // Calculate the current total height of spanned rows (excluding collapsed rows)
            let end_row = cell_info.row + cell_info.rowspan;
            let current_total: f32 = table_ctx.row_heights[cell_info.row..end_row]
                .iter()
                .enumerate()
                .filter(|(idx, _)| !table_ctx.collapsed_rows.contains(&(cell_info.row + idx)))
                .map(|(_, height)| height)
                .sum();
            
            // If the cell needs more height, distribute extra height across non-collapsed spanned rows
            if cell_height > current_total {
                let extra_height = cell_height - current_total;
                
                // Count non-collapsed rows in span
                let non_collapsed_rows = (cell_info.row..end_row)
                    .filter(|row_idx| !table_ctx.collapsed_rows.contains(row_idx))
                    .count();
                
                if non_collapsed_rows > 0 {
                    let per_row = extra_height / non_collapsed_rows as f32;
                    
                    for row_idx in cell_info.row..end_row {
                        if !table_ctx.collapsed_rows.contains(&row_idx) {
                            table_ctx.row_heights[row_idx] += per_row;
                        }
                    }
                }
            }
        }
    }
    
    // CSS 2.2 Section 17.6: Final pass - ensure collapsed rows have height 0
    for &row_idx in &table_ctx.collapsed_rows {
        if row_idx < table_ctx.row_heights.len() {
            table_ctx.row_heights[row_idx] = 0.0;
        }
    }
    
    Ok(())
}

/// Position all cells in the table grid with calculated widths and heights
fn position_table_cells<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    table_ctx: &mut TableLayoutContext,
    tree: &mut LayoutTree<T>,
    ctx: &mut LayoutContext<T, Q>,
    table_index: usize,
    constraints: &LayoutConstraints,
) -> Result<BTreeMap<usize, LogicalPosition>> {
    ctx.debug_log("Positioning table cells in grid");
    
    let mut positions = BTreeMap::new();
    
    // Get border spacing values if border-collapse is separate
    use azul_css::props::layout::StyleBorderCollapse;
    let (h_spacing, v_spacing) = if table_ctx.border_collapse == StyleBorderCollapse::Separate {
        use azul_css::props::basic::{ResolutionContext, PropertyContext, PhysicalSize};
        use crate::solver3::getters::{get_element_font_size, get_parent_font_size, get_root_font_size};
        
        let styled_dom = ctx.styled_dom;
        let table_id = tree.nodes[table_index].dom_node_id.unwrap();
        let table_state = &styled_dom.styled_nodes.as_container()[table_id].state;
        
        let spacing_context = ResolutionContext {
            element_font_size: get_element_font_size(styled_dom, table_id, table_state),
            parent_font_size: get_parent_font_size(styled_dom, table_id, table_state),
            root_font_size: get_root_font_size(styled_dom, table_state),
            containing_block_size: PhysicalSize::new(0.0, 0.0),
            element_size: None,
            viewport_size: PhysicalSize::new(0.0, 0.0), // TODO: Get actual DPI scale from ctx
        };
        
        let h = table_ctx.border_spacing.horizontal.resolve_with_context(&spacing_context, PropertyContext::Other);
        let v = table_ctx.border_spacing.vertical.resolve_with_context(&spacing_context, PropertyContext::Other);
        (h, v)
    } else {
        (0.0, 0.0)
    };
    
    ctx.debug_log(&format!(
        "Border spacing: h={:.2}, v={:.2}",
        h_spacing, v_spacing
    ));
    
    // Calculate cumulative column positions (x-offsets) with spacing
    let mut col_positions = vec![0.0; table_ctx.columns.len()];
    let mut x_offset = h_spacing; // Start with spacing on the left
    for (i, col) in table_ctx.columns.iter().enumerate() {
        col_positions[i] = x_offset;
        if let Some(width) = col.computed_width {
            x_offset += width + h_spacing; // Add spacing between columns
        }
    }
    
    // Calculate cumulative row positions (y-offsets) with spacing
    let mut row_positions = vec![0.0; table_ctx.num_rows];
    let mut y_offset = v_spacing; // Start with spacing on the top
    for (i, &height) in table_ctx.row_heights.iter().enumerate() {
        row_positions[i] = y_offset;
        y_offset += height + v_spacing; // Add spacing between rows
    }
    
    // Position each cell
    for cell_info in &table_ctx.cells {
        let cell_node = tree.get_mut(cell_info.node_index)
            .ok_or(LayoutError::InvalidTree)?;
        
        // Calculate cell position
        let x = col_positions.get(cell_info.column).copied().unwrap_or(0.0);
        let y = row_positions.get(cell_info.row).copied().unwrap_or(0.0);
        
        // Calculate cell size (sum of spanned columns/rows)
        let mut width = 0.0;
        println!("[position_table_cells] Cell {}: calculating width from cols {}..{}", 
            cell_info.node_index, cell_info.column, cell_info.column + cell_info.colspan);
        for col_idx in cell_info.column..(cell_info.column + cell_info.colspan) {
            if let Some(col) = table_ctx.columns.get(col_idx) {
                println!("[position_table_cells]   Col {}: computed_width={:?}", col_idx, col.computed_width);
                if let Some(col_width) = col.computed_width {
                    width += col_width;
                    // Add spacing between spanned columns (but not after the last one)
                    if col_idx < cell_info.column + cell_info.colspan - 1 {
                        width += h_spacing;
                    }
                } else {
                    println!("[position_table_cells]   ⚠️  Col {} has NO computed_width!", col_idx);
                }
            } else {
                println!("[position_table_cells]   ⚠️  Col {} not found in table_ctx.columns!", col_idx);
            }
        }
        
        let mut height = 0.0;
        let end_row = cell_info.row + cell_info.rowspan;
        for row_idx in cell_info.row..end_row {
            if let Some(&row_height) = table_ctx.row_heights.get(row_idx) {
                height += row_height;
                // Add spacing between spanned rows (but not after the last one)
                if row_idx < end_row - 1 {
                    height += v_spacing;
                }
            }
        }
        
        // Update cell's used size and position
        let writing_mode = constraints.writing_mode;
        // Table layout works in main/cross axes, must convert back to logical width/height
        
        println!("[position_table_cells] Cell {}: BEFORE from_main_cross: width={}, height={}, writing_mode={:?}", 
            cell_info.node_index, width, height, writing_mode);
        
        cell_node.used_size = Some(LogicalSize::from_main_cross(height, width, writing_mode));
        
        println!("[position_table_cells] Cell {}: AFTER from_main_cross: used_size={:?}", 
            cell_info.node_index, cell_node.used_size);
        
        println!("[position_table_cells] Cell {}: setting used_size to {}x{} (row_heights={:?})", 
            cell_info.node_index, width, height, table_ctx.row_heights);
        
        // Apply vertical-align to cell content if it has inline layout
        if let Some(ref inline_result) = cell_node.inline_layout_result {
            use azul_css::props::style::StyleVerticalAlign;
            use azul_core::styled_dom::StyledNodeState;
            
            // Get vertical-align property from styled_dom
            let vertical_align = if let Some(dom_id) = cell_node.dom_node_id {
                let node_data = &ctx.styled_dom.node_data.as_container()[dom_id];
                let node_state = StyledNodeState::default();
                
                ctx.styled_dom
                    .css_property_cache
                    .ptr
                    .get_vertical_align(node_data, &dom_id, &node_state)
                    .and_then(|v| v.get_property().copied())
                    .unwrap_or(StyleVerticalAlign::Top)
            } else {
                StyleVerticalAlign::Top
            };
            
            // Calculate content height from inline layout bounds
            let content_bounds = inline_result.bounds();
            let content_height = content_bounds.height;
            
            // Get padding and border to calculate content-box height
            // height is border-box, but vertical alignment should be within content-box
            let padding = &cell_node.box_props.padding;
            let border = &cell_node.box_props.border;
            let content_box_height = height 
                - padding.main_start(writing_mode) 
                - padding.main_end(writing_mode)
                - border.main_start(writing_mode)
                - border.main_end(writing_mode);
            
            // Calculate vertical offset based on alignment within content-box
            let align_factor = match vertical_align {
                StyleVerticalAlign::Top => 0.0,
                StyleVerticalAlign::Center => 0.5,
                StyleVerticalAlign::Bottom => 1.0,
            };
            let y_offset = (content_box_height - content_height) * align_factor;
            
            println!("[position_table_cells] Cell {}: vertical-align={:?}, border_box_height={}, content_box_height={}, content_height={}, y_offset={}", 
                cell_info.node_index, vertical_align, height, content_box_height, content_height, y_offset);
            
            // Create new layout with adjusted positions
            if y_offset.abs() > 0.01 { // Only adjust if offset is significant
                use crate::text3::cache::{UnifiedLayout, PositionedItem};
                use std::sync::Arc;
                
                let adjusted_items: Vec<PositionedItem<T>> = inline_result.items.iter().map(|item| {
                    PositionedItem {
                        item: item.item.clone(),
                        position: crate::text3::cache::Point {
                            x: item.position.x,
                            y: item.position.y + y_offset,
                        },
                        line_index: item.line_index,
                    }
                }).collect();
                
                let adjusted_layout = UnifiedLayout {
                    items: adjusted_items,
                    overflow: inline_result.overflow.clone(),
                    used_fonts: inline_result.used_fonts.clone(),
                };
                
                cell_node.inline_layout_result = Some(Arc::new(adjusted_layout));
            }
        }
        
        // Store position relative to table origin
        let position = LogicalPosition::from_main_cross(y, x, writing_mode);
        
        // Insert position into map so cache module can position the cell
        positions.insert(cell_info.node_index, position);
        
        ctx.debug_log(&format!(
            "Cell at row={}, col={}: pos=({:.2}, {:.2}), size=({:.2}x{:.2})",
            cell_info.row, cell_info.column, x, y, width, height
        ));
    }
    
    Ok(positions)
}

/// Gathers all inline content for `text3`, recursively laying out `inline-block` children
/// to determine their size and baseline before passing them to the text engine.
fn collect_and_measure_inline_content<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &mut LayoutContext<T, Q>,
    text_cache: &mut TextLayoutCache<T>,
    tree: &mut LayoutTree<T>,
    ifc_root_index: usize,
) -> Result<(Vec<InlineContent>, HashMap<ContentIndex, usize>)> {
    ctx.debug_ifc_layout(format!("collect_and_measure_inline_content: node_index={}", ifc_root_index));

    let mut content = Vec::new();
    // Maps the `ContentIndex` used by text3 back to the `LayoutNode` index.
    let mut child_map = HashMap::new();
    let ifc_root_node = tree.get(ifc_root_index).ok_or(LayoutError::InvalidTree)?;

    // Get the DOM node ID of the IFC root
    let Some(ifc_root_dom_id) = ifc_root_node.dom_node_id else {
        ctx.debug_warning("IFC root has no DOM ID");
        return Ok((content, child_map));
    };

    // Collect children to avoid holding an immutable borrow during iteration
    let children: Vec<_> = ifc_root_node.children.clone();
    drop(ifc_root_node);

    ctx.debug_ifc_layout(format!("Node {} has {} layout children", ifc_root_index, children.len()));
    
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
                use azul_css::props::layout::LayoutDisplay;
                if *display == LayoutDisplay::ListItem {
                    ctx.debug_ifc_layout(format!("IFC root NodeId({:?}) is list-item", dom_id));
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
                            use azul_css::props::layout::LayoutDisplay;
                            if *display == LayoutDisplay::ListItem {
                                ctx.debug_ifc_layout(format!("IFC root parent NodeId({:?}) is list-item", parent_dom_id));
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
        ctx.debug_ifc_layout(format!("Found list-item (NodeId({:?})), generating marker", list_dom_id));
        
        // Find the layout node index for the list-item DOM node
        let list_item_layout_idx = tree.nodes.iter().enumerate()
            .find(|(_, node)| node.dom_node_id == Some(list_dom_id) && node.pseudo_element.is_none())
            .map(|(idx, _)| idx);
        
        if let Some(list_idx) = list_item_layout_idx {
            // Per CSS spec, the ::marker pseudo-element is the first child of the list-item
            // Find the ::marker pseudo-element in the list-item's children
            let list_item_node = tree.get(list_idx).ok_or(LayoutError::InvalidTree)?;
            let marker_idx = list_item_node.children.iter().find(|&&child_idx| {
                tree.get(child_idx)
                    .map(|child| child.pseudo_element == Some(crate::solver3::layout_tree::PseudoElement::Marker))
                    .unwrap_or(false)
            }).copied();
            
            if let Some(marker_idx) = marker_idx {
                ctx.debug_ifc_layout(format!("Found ::marker pseudo-element at index {}", marker_idx));
                
                // Get the DOM ID for style resolution (marker references the same DOM node as list-item)
                let list_dom_id_for_style = tree.get(marker_idx)
                    .and_then(|n| n.dom_node_id)
                    .unwrap_or(list_dom_id);
                
                // Get list-style-position to determine marker positioning
                // Default is 'outside' per CSS Lists Module Level 3
                use crate::solver3::getters::get_list_style_position;
                use azul_css::props::style::lists::StyleListStylePosition;
                
                let list_style_position = get_list_style_position(ctx.styled_dom, Some(list_dom_id));
                let position_outside = matches!(list_style_position, StyleListStylePosition::Outside);
                
                ctx.debug_ifc_layout(format!("List marker list-style-position: {:?} (outside={})", 
                                             list_style_position, position_outside));
                
                // Generate marker text segments with proper Unicode font fallback
                let base_style = Arc::new(get_style_properties(ctx.styled_dom, list_dom_id_for_style));
                let marker_segments = generate_list_marker_segments(
                    tree,
                    ctx.styled_dom,
                    marker_idx,  // Pass the marker index, not the list-item index
                    ctx.counters,
                    &ctx.font_manager.fc_cache,
                    base_style,
                );
                
                ctx.debug_ifc_layout(format!("Generated {} list marker segments", marker_segments.len()));
                
                // Add markers as InlineContent::Marker with position information
                // Outside markers will be positioned in the padding gutter by the layout engine
                for segment in marker_segments {
                    content.push(InlineContent::Marker {
                        run: segment,
                        position_outside,
                    });
                }
            } else {
                ctx.debug_ifc_layout(format!("WARNING: List-item at index {} has no ::marker pseudo-element", list_idx));
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
                style: Arc::new(get_style_properties(ctx.styled_dom, ifc_root_dom_id)),
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

        if get_display_property(ctx.styled_dom, Some(dom_id)).unwrap_or_default() != LayoutDisplay::Inline {
            // This is an atomic inline-level box (e.g., inline-block, image).
            // We must determine its size and baseline before passing it to text3.

            // The intrinsic sizing pass has already calculated its preferred size.
            let intrinsic_size = child_node.intrinsic_sizes.clone().unwrap_or_default();
            
            // CRITICAL FIX: For inline-blocks, check if explicit CSS width/height are specified
            // If so, use those instead of intrinsic sizes (which may be 0 for empty inline-blocks)
            let styled_node_state = ctx
                .styled_dom
                .styled_nodes
                .as_container()
                .get(dom_id)
                .map(|n| n.state.clone())
                .unwrap_or_default();
            
            let css_width = crate::solver3::getters::get_css_width(ctx.styled_dom, dom_id, &styled_node_state);
            let css_height = crate::solver3::getters::get_css_height(ctx.styled_dom, dom_id, &styled_node_state);
            
            // Resolve explicit width if specified, otherwise use max-content width
            let width = match css_width.unwrap_or_default() {
                azul_css::props::layout::LayoutWidth::Px(px) => {
                    // Convert pixel value to actual pixels
                    use azul_css::props::basic::SizeMetric;
                    match px.metric {
                        SizeMetric::Px => px.number.get(),
                        SizeMetric::Pt => px.number.get() * 1.33333, // PT_TO_PX
                        SizeMetric::In => px.number.get() * 96.0,
                        SizeMetric::Cm => px.number.get() * 96.0 / 2.54,
                        SizeMetric::Mm => px.number.get() * 96.0 / 25.4,
                        SizeMetric::Em | SizeMetric::Rem => {
                            // TODO: Resolve em/rem properly with font-size
                            px.number.get() * 16.0 // DEFAULT_FONT_SIZE
                        }
                        _ => intrinsic_size.max_content_width, // Percent/viewport - use intrinsic
                    }
                }
                _ => intrinsic_size.max_content_width, // Auto or other - use intrinsic
            };
            
            eprintln!("[collect_and_measure_inline_content] Inline-block NodeId({:?}): intrinsic_width={}, css_width={:?}, final_width={}", 
                dom_id, intrinsic_size.max_content_width, css_width, width);

            // To find its height and baseline, we must lay out its contents.
            let writing_mode = get_writing_mode(ctx.styled_dom, dom_id, &styled_node_state).unwrap_or_default();
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
            let mut empty_float_cache = std::collections::BTreeMap::new();
            let layout_output =
                layout_formatting_context(ctx, tree, text_cache, child_index, &child_constraints, &mut empty_float_cache)?;

            let mut final_height = layout_output.overflow_size.height;
            
            // CRITICAL FIX: Check for explicit CSS height
            if let azul_css::props::layout::LayoutHeight::Px(px) = css_height.unwrap_or_default() {
                use azul_css::props::basic::SizeMetric;
                final_height = match px.metric {
                    SizeMetric::Px => px.number.get(),
                    SizeMetric::Pt => px.number.get() * 1.33333,
                    SizeMetric::In => px.number.get() * 96.0,
                    SizeMetric::Cm => px.number.get() * 96.0 / 2.54,
                    SizeMetric::Mm => px.number.get() * 96.0 / 25.4,
                    SizeMetric::Em | SizeMetric::Rem => px.number.get() * 16.0,
                    _ => final_height, // Percent/viewport - use layout result
                };
            }
            
            eprintln!("[collect_and_measure_inline_content] Inline-block NodeId({:?}): layout_height={}, css_height={:?}, final_height={}", 
                dom_id, layout_output.overflow_size.height, css_height, final_height);
            
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
                source_node_id: Some(dom_id),
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
        } else {
            // This is a regular inline box (display: inline) - e.g., <span>, <em>, <strong>
            // According to CSS Inline-3 spec §2, inline boxes are "transparent" wrappers
            // We must recursively collect their text children with inherited style
            eprintln!(
                "[collect_and_measure_inline_content] Found inline span (DOM {:?}), recursing",
                dom_id
            );
            
            let span_style = get_style_properties(ctx.styled_dom, dom_id);
            collect_inline_span_recursive(
                ctx,
                tree,
                dom_id,
                span_style,
                &mut content,
                &mut child_map,
                &children,
            )?;
        }
    }
    Ok((content, child_map))
}

/// Recursively collects inline content from an inline span (display: inline) element.
/// 
/// According to CSS Inline Layout Module Level 3 §2:
/// "Inline boxes are transparent wrappers that wrap their content."
/// They don't create a new formatting context - their children participate in the
/// same IFC as the parent.
///
/// This function processes:
/// - Text nodes: collected with the span's inherited style
/// - Nested inline spans: recursively descended
/// - Inline-blocks, images: measured and added as shapes
fn collect_inline_span_recursive<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &mut LayoutContext<T, Q>,
    tree: &mut LayoutTree<T>,
    span_dom_id: NodeId,
    span_style: StyleProperties,
    content: &mut Vec<InlineContent>,
    child_map: &mut HashMap<ContentIndex, usize>,
    parent_children: &[usize], // Layout tree children of parent IFC
) -> Result<()> {
    eprintln!("[collect_inline_span_recursive] Processing inline span {:?}", span_dom_id);
    
    // Get DOM children of this span
    let span_dom_children: Vec<NodeId> = span_dom_id
        .az_children(&ctx.styled_dom.node_hierarchy.as_container())
        .collect();
    
    eprintln!("[collect_inline_span_recursive] Span has {} DOM children", span_dom_children.len());
    
    for &child_dom_id in &span_dom_children {
        let node_data = &ctx.styled_dom.node_data.as_container()[child_dom_id];
        
        // CASE 1: Text node - collect with span's style
        if let NodeType::Text(ref text_content) = node_data.get_node_type() {
            eprintln!(
                "[collect_inline_span_recursive] ✓ Found text in span: '{}'",
                text_content.as_str()
            );
            content.push(InlineContent::Text(StyledRun {
                text: text_content.to_string(),
                style: Arc::new(span_style.clone()),
                logical_start_byte: 0,
            }));
            continue;
        }
        
        // CASE 2: Element node - check its display type
        let child_display = get_display_property(ctx.styled_dom, Some(child_dom_id))
            .unwrap_or_default();
        
        // Find the corresponding layout tree node
        let child_index = parent_children
            .iter()
            .find(|&&idx| {
                tree.get(idx)
                    .and_then(|n| n.dom_node_id)
                    .map(|id| id == child_dom_id)
                    .unwrap_or(false)
            })
            .copied();
        
        match child_display {
            LayoutDisplay::Inline => {
                // Nested inline span - recurse with child's style
                eprintln!("[collect_inline_span_recursive] Found nested inline span {:?}", child_dom_id);
                let child_style = get_style_properties(ctx.styled_dom, child_dom_id);
                collect_inline_span_recursive(
                    ctx,
                    tree,
                    child_dom_id,
                    child_style,
                    content,
                    child_map,
                    parent_children,
                )?;
            }
            LayoutDisplay::InlineBlock => {
                // Inline-block inside span - measure and add as shape
                let Some(child_index) = child_index else {
                    eprintln!("[collect_inline_span_recursive] WARNING: inline-block {:?} has no layout node", child_dom_id);
                    continue;
                };
                
                let child_node = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
                let intrinsic_size = child_node.intrinsic_sizes.clone().unwrap_or_default();
                let width = intrinsic_size.max_content_width;
                
                let styled_node_state = ctx
                    .styled_dom
                    .styled_nodes
                    .as_container()
                    .get(child_dom_id)
                    .map(|n| n.state.clone())
                    .unwrap_or_default();
                let writing_mode = get_writing_mode(ctx.styled_dom, child_dom_id, &styled_node_state).unwrap_or_default();
                let child_constraints = LayoutConstraints {
                    available_size: LogicalSize::new(width, f32::INFINITY),
                    writing_mode,
                    bfc_state: None,
                    text_align: TextAlign::Start,
                };
                
                drop(child_node);
                
                let mut empty_float_cache = std::collections::BTreeMap::new();
                let layout_output = layout_formatting_context(ctx, tree, &mut TextLayoutCache::default(), child_index, &child_constraints, &mut empty_float_cache)?;
                let final_height = layout_output.overflow_size.height;
                let final_size = LogicalSize::new(width, final_height);
                
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
                    source_node_id: Some(child_dom_id),
                }));
                
                // Note: We don't add to child_map here because this is inside a span
                eprintln!("[collect_inline_span_recursive] Added inline-block shape {}x{}", width, final_height);
            }
            _ => {
                // Other display types inside span (shouldn't normally happen)
                eprintln!("[collect_inline_span_recursive] WARNING: Unsupported display type {:?} inside inline span", child_display);
            }
        }
    }
    
    Ok(())
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
///
/// # Examples
///
/// ```ignore
/// // Positive margins: larger wins
/// assert_eq!(collapse_margins(20.0, 10.0), 20.0);
/// 
/// // Negative margins: more negative wins
/// assert_eq!(collapse_margins(-20.0, -10.0), -20.0);
/// 
/// // Mixed signs: sum
/// assert_eq!(collapse_margins(20.0, -10.0), 10.0);
/// ```
pub(crate) fn collapse_margins(a: f32, b: f32) -> f32 {
    if a.is_sign_positive() && b.is_sign_positive() {
        a.max(b)
    } else if a.is_sign_negative() && b.is_sign_negative() {
        a.min(b)
    } else {
        a + b
    }
}

/// Helper function to advance the pen position with margin collapsing.
///
/// This implements CSS 2.1 margin collapsing for adjacent block-level boxes in a BFC.
/// 
/// # Arguments
/// * `pen` - Current main-axis position (will be modified)
/// * `last_margin_bottom` - The bottom margin of the previous in-flow element
/// * `current_margin_top` - The top margin of the current element
/// 
/// # Returns
/// The new `last_margin_bottom` value (the bottom margin of the current element)
///
/// # CSS Spec Compliance
/// Per CSS 2.1 Section 8.3.1 "Collapsing margins":
/// - Adjacent vertical margins of block boxes collapse
/// - The resulting margin width is the maximum of the adjoining margins (if both positive)
/// - Or the sum of the most positive and most negative (if signs differ)
///
/// # Example Flow
/// ```ignore
/// // Element 1: margin-bottom = 20px
/// // Element 2: margin-top = 30px, margin-bottom = 15px
/// 
/// let mut pen = 0.0;
/// let mut last_margin = 0.0;
/// 
/// // After element 1:
/// pen += element1_height;
/// last_margin = 20.0;
/// 
/// // Before element 2:
/// let collapsed = collapse_margins(20.0, 30.0); // = 30.0 (larger wins)
/// pen += collapsed; // Advance by 30px, not 50px
/// // Position element 2 at pen
/// pen += element2_height;
/// last_margin = 15.0; // Save for next element
/// ```
fn advance_pen_with_margin_collapse(
    pen: &mut f32,
    last_margin_bottom: f32,
    current_margin_top: f32,
) -> f32 {
    // Collapse the previous element's bottom margin with current element's top margin
    let collapsed_margin = collapse_margins(last_margin_bottom, current_margin_top);
    
    // Advance pen by the collapsed margin
    *pen += collapsed_margin;
    
    // Return collapsed_margin so caller knows how much space was actually added
    collapsed_margin
}

/// Checks if an element's border or padding prevents margin collapsing.
///
/// Per CSS 2.1 Section 8.3.1:
/// - Border between margins prevents collapsing
/// - Padding between margins prevents collapsing
///
/// # Arguments
/// * `box_props` - The box properties containing border and padding
/// * `writing_mode` - The writing mode to determine main axis
/// * `check_start` - If true, check main-start (top); if false, check main-end (bottom)
///
/// # Returns
/// `true` if border or padding exists and prevents collapsing, `false` otherwise
fn has_margin_collapse_blocker(
    box_props: &crate::solver3::geometry::BoxProps,
    writing_mode: LayoutWritingMode,
    check_start: bool, // true = check top/start, false = check bottom/end
) -> bool {
    if check_start {
        // Check if there's border-top or padding-top
        let border_start = box_props.border.main_start(writing_mode);
        let padding_start = box_props.padding.main_start(writing_mode);
        border_start > 0.0 || padding_start > 0.0
    } else {
        // Check if there's border-bottom or padding-bottom
        let border_end = box_props.border.main_end(writing_mode);
        let padding_end = box_props.padding.main_end(writing_mode);
        border_end > 0.0 || padding_end > 0.0
    }
}

/// Checks if an element is empty (has no content).
///
/// Per CSS 2.1 Section 8.3.1:
/// If a block element has no border, padding, inline content, height, or min-height,
/// then its top and bottom margins collapse with each other.
///
/// # Arguments
/// * `node` - The layout node to check
/// 
/// # Returns
/// `true` if the element is empty and its margins can collapse internally
fn is_empty_block<T: ParsedFontTrait>(node: &crate::solver3::layout_tree::LayoutNode<T>) -> bool {
    // Per CSS 2.1 spec: An empty block is one with:
    // - No in-flow children
    // - No inline content (text)
    // - No border or padding (checked elsewhere via has_margin_collapse_blocker)
    // - No explicit height (height: auto or height: 0)
    // - No min-height that would force a height
    
    // Check if node has children
    if !node.children.is_empty() {
        if std::env::var("DEBUG_MARGIN_COLLAPSE").is_ok() {
            println!("[is_empty_block] Node {:?} has {} children → NOT EMPTY", 
                     node.dom_node_id, node.children.len());
        }
        return false;
    }
    
    // Check if node has inline content (text)
    if node.inline_layout_result.is_some() {
        if std::env::var("DEBUG_MARGIN_COLLAPSE").is_ok() {
            println!("[is_empty_block] Node {:?} has inline content → NOT EMPTY", 
                     node.dom_node_id);
        }
        return false;
    }
    
    // DON'T check used_size.height here! 
    // During the sizing pass, empty blocks may get a default height (e.g., line-height).
    // We need to check if there's an EXPLICIT height that forces the block to have size.
    // Since we don't have access to CSS properties here, and the box_props don't include
    // explicit height info, we use a heuristic: if used_size exists and is exactly 0,
    // it's explicitly empty. Otherwise, we can't determine it from used_size alone.
    // 
    // The proper solution would be to check the CSS property directly, but that requires
    // StyledDom access. For now, we assume empty blocks have NO content, so they're empty.
    
    // Empty block: no children, no inline content
    if std::env::var("DEBUG_MARGIN_COLLAPSE").is_ok() {
        println!("[is_empty_block] Node {:?} IS EMPTY ✓", node.dom_node_id);
    }
    true
}

/// Generates marker text for a list item marker.
///
/// This function looks up the counter value from the cache and formats it
/// according to the list-style-type property.
/// 
/// Per CSS Lists Module Level 3, the ::marker pseudo-element is the first child
/// of the list-item, and references the same DOM node. Counter resolution happens
/// on the list-item (parent) node.
fn generate_list_marker_text<T: ParsedFontTrait>(
    tree: &LayoutTree<T>,
    styled_dom: &StyledDom,
    marker_index: usize,
    counters: &BTreeMap<(usize, String), i32>,
) -> String {
    use crate::solver3::{counters::format_counter, getters::get_list_style_type};
    use azul_css::props::style::lists::StyleListStyleType;
    
    // Get the marker node
    let marker_node = match tree.get(marker_index) {
        Some(n) => n,
        None => return String::new(),
    };
    
    // Verify this is actually a ::marker pseudo-element
    // Per spec, markers must be pseudo-elements, not anonymous boxes
    if marker_node.pseudo_element != Some(crate::solver3::layout_tree::PseudoElement::Marker) {
        eprintln!("[generate_list_marker_text] WARNING: Node {} is not a ::marker pseudo-element (pseudo={:?}, anonymous_type={:?})", 
                 marker_index, marker_node.pseudo_element, marker_node.anonymous_type);
        // Fallback for old-style anonymous markers during transition
        if marker_node.anonymous_type != Some(crate::solver3::layout_tree::AnonymousBoxType::ListItemMarker) {
            return String::new();
        }
    }
    
    // Get the parent list-item node (::marker is first child of list-item)
    let list_item_index = match marker_node.parent {
        Some(p) => p,
        None => {
            eprintln!("[generate_list_marker_text] ERROR: Marker has no parent");
            return String::new();
        }
    };
    
    let list_item_node = match tree.get(list_item_index) {
        Some(n) => n,
        None => return String::new(),
    };
    
    let list_item_dom_id = match list_item_node.dom_node_id {
        Some(id) => id,
        None => {
            eprintln!("[generate_list_marker_text] ERROR: List-item has no DOM ID");
            return String::new();
        }
    };
    
    eprintln!("[generate_list_marker_text] marker_index={}, list_item_index={}, list_item_dom_id={:?}", 
             marker_index, list_item_index, list_item_dom_id);
    
    // Get list-style-type from the list-item or its container
    let list_container_dom_id = if let Some(grandparent_index) = list_item_node.parent {
        if let Some(grandparent) = tree.get(grandparent_index) {
            grandparent.dom_node_id
        } else {
            None
        }
    } else {
        None
    };
    
    // Try to get list-style-type from the list container first, then fall back to the list-item
    let list_style_type = if let Some(container_id) = list_container_dom_id {
        let container_type = get_list_style_type(styled_dom, Some(container_id));
        if container_type != StyleListStyleType::default() {
            container_type
        } else {
            get_list_style_type(styled_dom, Some(list_item_dom_id))
        }
    } else {
        get_list_style_type(styled_dom, Some(list_item_dom_id))
    };
    
    // Get the counter value for "list-item" counter from the LIST-ITEM node
    // Per CSS spec, counters are scoped to elements, and the list-item counter
    // is incremented at the list-item element, not the marker pseudo-element
    let counter_value = counters
        .get(&(list_item_index, "list-item".to_string()))
        .copied()
        .unwrap_or_else(|| {
            eprintln!("[generate_list_marker_text] WARNING: No counter found for list-item at index {}, defaulting to 1", list_item_index);
            1
        });
    
    eprintln!("[generate_list_marker_text] counter_value={} for list_item_index={}", counter_value, list_item_index);
    
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

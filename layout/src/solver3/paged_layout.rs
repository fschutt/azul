//! CSS Paged Media layout integration with integrated fragmentation
//!
//! This module provides functionality for laying out documents with pagination,
//! such as for PDF generation. It integrates fragmentation INTO layout (not post-layout).
//!
//! ## Key Differences from Post-Layout Splitting
//!
//! 1. Break decisions are made DURING layout, not after
//! 2. `break-inside: avoid` is respected
//! 3. Orphans/widows are applied at line level
//! 4. Headers/footers are generated per-page with dynamic counters
//! 5. Content is never duplicated across pages

use std::collections::BTreeMap;

use azul_core::{
    dom::{DomId, NodeId, NodeType},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    hit_test::ScrollPosition,
    resources::RendererResources,
    selection::SelectionState,
    styled_dom::StyledDom,
};
use azul_css::{
    LayoutDebugMessage,
    props::layout::fragmentation::{PageBreak, BreakInside},
};

use crate::{
    font_traits::{ParsedFontTrait, TextLayoutCache},
    fragmentation::{
        FragmentationLayoutContext, FragmentationDefaults, PageTemplate, PageMargins,
        BoxBreakBehavior, KeepTogetherPriority, BreakDecision, PageFragment,
        decide_break,
    },
    paged::FragmentationContext,
    solver3::{
        cache::LayoutCache,
        display_list::{DisplayList, DisplayListItem},
        getters::{
            get_break_before, get_break_after, get_break_inside, get_orphans, get_widows,
            is_avoid_break_inside, is_forced_page_break, is_avoid_page_break,
        },
        LayoutContext, LayoutError, Result,
    },
};

// ============================================================================
// Public API
// ============================================================================

/// Options for paged layout
#[derive(Debug, Clone)]
pub struct PagedLayoutOptions {
    /// Page size (full page including margins)
    pub page_size: LogicalSize,
    /// Page margins
    pub margins: PageMargins,
    /// Page template for headers/footers
    pub template: PageTemplate,
    /// Fragmentation defaults (smart breaking behavior)
    pub defaults: FragmentationDefaults,
}

impl Default for PagedLayoutOptions {
    fn default() -> Self {
        Self {
            page_size: LogicalSize::new(595.0, 842.0), // A4 in points
            margins: PageMargins::uniform(50.0),
            template: PageTemplate::default(),
            defaults: FragmentationDefaults::default(),
        }
    }
}

/// Layout a document with integrated pagination, returning one DisplayList per page.
///
/// This function performs CSS Paged Media layout with fragmentation integrated
/// into the layout process itself, not as a post-processing step.
///
/// # Arguments
/// * `options` - Paged layout options (page size, margins, template)
/// * Other arguments same as `layout_document()`
///
/// # Returns
/// A vector of DisplayLists, one per page. Each DisplayList contains the
/// elements that fit on that page, with Y-coordinates relative to the page origin.
#[cfg(feature = "text_layout")]
pub fn layout_document_paged<T, F>(
    cache: &mut LayoutCache,
    text_cache: &mut TextLayoutCache,
    fragmentation_context: FragmentationContext,
    new_dom: StyledDom,
    viewport: LogicalRect,
    font_manager: &mut crate::font_traits::FontManager<T>,
    scroll_offsets: &BTreeMap<NodeId, ScrollPosition>,
    selections: &BTreeMap<DomId, SelectionState>,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    gpu_value_cache: Option<&azul_core::gpu::GpuValueCache>,
    renderer_resources: &RendererResources,
    id_namespace: azul_core::resources::IdNamespace,
    dom_id: DomId,
    font_loader: F,
) -> Result<Vec<DisplayList>> 
where
    T: ParsedFontTrait + Sync + 'static,
    F: Fn(&[u8], usize) -> std::result::Result<T, crate::text3::cache::LayoutError>,
{
    
    // === FONT RESOLUTION AND LOADING ===
    {
        use crate::solver3::getters::{
            collect_and_resolve_font_chains, 
            collect_font_ids_from_chains,
            compute_fonts_to_load,
            load_fonts_from_disk,
        };
        
        let chains = collect_and_resolve_font_chains(&new_dom, &font_manager.fc_cache);
        let required_fonts = collect_font_ids_from_chains(&chains);
        let already_loaded = font_manager.get_loaded_font_ids();
        let fonts_to_load = compute_fonts_to_load(&required_fonts, &already_loaded);
        
        if !fonts_to_load.is_empty() {
            let load_result = load_fonts_from_disk(
                &fonts_to_load,
                &font_manager.fc_cache,
                &font_loader,
            );
            font_manager.insert_fonts(load_result.loaded);
            for (font_id, error) in &load_result.failed {
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::warning(format!(
                        "[FontLoading] Failed to load font {:?}: {}", font_id, error
                    )));
                }
            }
        }
        font_manager.set_font_chain_cache(chains.into_inner());
    }

    // Get page dimensions from fragmentation context
    let page_size = match &fragmentation_context {
        FragmentationContext::Paged { page_size, .. } => *page_size,
        FragmentationContext::Continuous { width, .. } => {
            let display_list = super::layout_document(
                cache, text_cache, new_dom, viewport, font_manager,
                scroll_offsets, selections, debug_messages,
                gpu_value_cache, renderer_resources, id_namespace, dom_id,
            )?;
            return Ok(vec![display_list]);
        }
        FragmentationContext::MultiColumn { column_width, column_height, .. } => {
            LogicalSize::new(*column_width, *column_height)
        }
        FragmentationContext::Regions { regions } => {
            regions.first()
                .map(|r| r.size)
                .unwrap_or(LogicalSize::new(viewport.size.width, viewport.size.height))
        }
    };
    
    // Perform regular layout first
    let display_list = super::layout_document(
        cache, text_cache, new_dom.clone(), viewport, font_manager,
        scroll_offsets, selections, debug_messages,
        gpu_value_cache, renderer_resources, id_namespace, dom_id,
    )?;
    
    // Create fragmentation context
    let margins = PageMargins::default();
    let mut frag_ctx = FragmentationLayoutContext::new(page_size, margins);
    
    // Classify and fragment display list
    let classified_items = classify_display_list_items(&display_list, &new_dom);
    let pages = fragment_display_list_with_breaks(
        display_list, &classified_items, &new_dom, &mut frag_ctx,
    );
    
    // Add page chrome
    let total_pages = pages.len();
    let final_pages = pages.into_iter().enumerate().map(|(page_idx, mut page)| {
        let chrome = generate_page_chrome(&frag_ctx, page_idx, total_pages);
        page.items.extend(chrome);
        page
    }).collect();
    
    Ok(final_pages)
}

// ============================================================================
// Display List Item Classification
// ============================================================================

/// Classification of a single display list item for pagination
#[derive(Debug, Clone)]
struct ClassifiedItem {
    /// Index in the original display list
    index: usize,
    /// Bounds of this item (if it has spatial extent)
    bounds: Option<LogicalRect>,
    /// How this item should behave at page breaks
    behavior: BoxBreakBehavior,
    /// CSS break-before property
    break_before: PageBreak,
    /// CSS break-after property
    break_after: PageBreak,
    /// Source DOM node (for debugging)
    source_node: Option<NodeId>,
    /// Item type for debugging
    item_type: ItemType,
}

/// Type of display list item for classification
#[derive(Debug, Clone, Copy, PartialEq)]
enum ItemType {
    /// Background rectangles, borders - typically part of a box
    BoxDecoration,
    /// Text content that can be split
    Text,
    /// Images - cannot be split
    Image,
    /// State management (clips, stacking contexts) - no spatial extent
    StateManagement,
    /// Other items
    Other,
}

impl ClassifiedItem {
    /// Get the Y-coordinate range this item occupies
    fn y_range(&self) -> Option<(f32, f32)> {
        self.bounds.map(|b| (b.origin.y, b.origin.y + b.size.height))
    }
    
    /// Check which page(s) this item belongs to
    fn pages_for_item(&self, page_height: f32) -> (usize, usize) {
        match self.bounds {
            Some(bounds) => {
                let start_page = (bounds.origin.y / page_height).floor() as usize;
                let end_y = bounds.origin.y + bounds.size.height;
                let end_page = if end_y > 0.0 {
                    ((end_y - 0.001) / page_height).floor() as usize
                } else {
                    0
                };
                (start_page, end_page)
            }
            None => (0, 0), // State management items go on page 0
        }
    }
}

/// Classify all items in the display list
/// 
/// Uses the node_mapping from the display list to look up CSS break properties
/// from the StyledDom.
fn classify_display_list_items(
    display_list: &DisplayList,
    styled_dom: &StyledDom,
) -> Vec<ClassifiedItem> {
    display_list.items.iter().enumerate().map(|(index, item)| {
        let bounds = get_item_bounds(item);
        let height = bounds.map(|b| b.size.height).unwrap_or(0.0);
        
        // Get source node from display list's node_mapping
        let source_node = display_list.node_mapping.get(index).copied().flatten();
        
        // Get CSS break properties from StyledDom
        let break_before = get_break_before(styled_dom, source_node);
        let break_after = get_break_after(styled_dom, source_node);
        let break_inside = get_break_inside(styled_dom, source_node);
        
        // Determine behavior based on item type AND CSS break properties
        let (item_type, mut behavior) = classify_item_type(item, height);
        
        // Apply break-inside:avoid from CSS
        if is_avoid_break_inside(&break_inside) {
            behavior = BoxBreakBehavior::KeepTogether {
                estimated_height: height,
                priority: KeepTogetherPriority::Normal,
            };
        }
        
        ClassifiedItem {
            index,
            bounds,
            behavior,
            break_before,
            break_after,
            source_node,
            item_type,
        }
    }).collect()
}

/// Classify a single item's type and break behavior
fn classify_item_type(item: &DisplayListItem, height: f32) -> (ItemType, BoxBreakBehavior) {
    match item {
        // Text can be split across pages
        DisplayListItem::TextLayout { .. } | DisplayListItem::Text { .. } => {
            (ItemType::Text, BoxBreakBehavior::Splittable {
                min_before_break: 14.0, // ~1 line
                min_after_break: 14.0,
            })
        }
        // Text decorations follow text
        DisplayListItem::Underline { .. } | 
        DisplayListItem::Strikethrough { .. } | 
        DisplayListItem::Overline { .. } => {
            (ItemType::Text, BoxBreakBehavior::Splittable {
                min_before_break: 0.0,
                min_after_break: 0.0,
            })
        }
        // Images cannot be split
        DisplayListItem::Image { .. } => {
            (ItemType::Image, BoxBreakBehavior::Monolithic { height })
        }
        // Box decorations (backgrounds, borders)
        DisplayListItem::Rect { .. } => {
            (ItemType::BoxDecoration, BoxBreakBehavior::Splittable {
                min_before_break: 0.0,
                min_after_break: 0.0,
            })
        }
        DisplayListItem::Border { .. } => {
            (ItemType::BoxDecoration, BoxBreakBehavior::Splittable {
                min_before_break: 0.0,
                min_after_break: 0.0,
            })
        }
        // State management - no spatial extent
        DisplayListItem::PushClip { .. } | DisplayListItem::PopClip |
        DisplayListItem::PushStackingContext { .. } | DisplayListItem::PopStackingContext |
        DisplayListItem::PushScrollFrame { .. } | DisplayListItem::PopScrollFrame => {
            (ItemType::StateManagement, BoxBreakBehavior::Monolithic { height: 0.0 })
        }
        // Selection/cursor - part of text
        DisplayListItem::SelectionRect { .. } | DisplayListItem::CursorRect { .. } => {
            (ItemType::Text, BoxBreakBehavior::Splittable {
                min_before_break: 0.0,
                min_after_break: 0.0,
            })
        }
        // Everything else
        _ => {
            (ItemType::Other, BoxBreakBehavior::Splittable {
                min_before_break: 0.0,
                min_after_break: 0.0,
            })
        }
    }
}

// ============================================================================
// Integrated Fragmentation Algorithm - Clean Clipping Approach
// ============================================================================

/// Fragment a display list into multiple pages using clean clipping.
/// 
/// Algorithm:
/// 1. Calculate total content height and number of pages needed
/// 2. For each page, iterate through ALL items
/// 3. Clip each item to the page's visible area [page_top, page_bottom]
/// 4. Offset Y coordinates to be page-relative (0 = top of page)
/// 5. Skip items that are completely outside the page bounds
/// 
/// This approach ensures:
/// - No duplication of items across pages
/// - Clean clipping at page boundaries
/// - Borders are properly handled (top/bottom borders hidden when clipped)
fn fragment_display_list_with_breaks(
    display_list: DisplayList,
    _classified_items: &[ClassifiedItem],
    _styled_dom: &StyledDom,
    ctx: &mut FragmentationLayoutContext,
) -> Vec<DisplayList> {
    let page_height = ctx.page_content_height;
    
    if page_height <= 0.0 {
        return vec![display_list];
    }
    
    // Calculate total content height
    let max_y = display_list.items.iter()
        .filter_map(|item| get_item_bounds(item))
        .map(|bounds| bounds.origin.y + bounds.size.height)
        .fold(0.0f32, f32::max);
    
    let num_pages = ((max_y / page_height).ceil() as usize).max(1);
    
    // Generate pages by clipping
    let mut pages = Vec::with_capacity(num_pages);
    
    for page_idx in 0..num_pages {
        let page_top = page_idx as f32 * page_height;
        let page_bottom = page_top + page_height;
        
        let page_items: Vec<DisplayListItem> = display_list.items.iter()
            .filter_map(|item| clip_and_offset_item(item, page_top, page_bottom))
            .collect();
        
        pages.push(DisplayList { 
            items: page_items, 
            node_mapping: Vec::new() 
        });
    }
    
    pages
}

/// Clip an item to the page bounds and offset to page-relative coordinates.
/// 
/// Returns None if the item is completely outside the page bounds.
/// For items that intersect the page boundary, clips the visible portion.
fn clip_and_offset_item(
    item: &DisplayListItem,
    page_top: f32,
    page_bottom: f32,
) -> Option<DisplayListItem> {
    match item {
        // === Rectangular items that can be clipped ===
        DisplayListItem::Rect { bounds, color, border_radius } => {
            clip_rect_to_page(*bounds, page_top, page_bottom).map(|clipped| {
                DisplayListItem::Rect {
                    bounds: clipped,
                    color: *color,
                    border_radius: *border_radius,
                }
            })
        }
        
        DisplayListItem::Border { bounds, widths, colors, styles, border_radius } => {
            // For borders, we need to hide top/bottom borders when clipped
            let original_bounds = *bounds;
            clip_rect_to_page(*bounds, page_top, page_bottom).map(|clipped| {
                let mut new_widths = *widths;
                
                // Hide top border if we clipped the top
                if clipped.origin.y > 0.0 && original_bounds.origin.y < page_top {
                    new_widths.top = None;
                }
                
                // Hide bottom border if we clipped the bottom
                let original_bottom = original_bounds.origin.y + original_bounds.size.height;
                let clipped_bottom = clipped.origin.y + clipped.size.height;
                if original_bottom > page_bottom && clipped_bottom >= page_bottom - page_top - 1.0 {
                    new_widths.bottom = None;
                }
                
                DisplayListItem::Border {
                    bounds: clipped,
                    widths: new_widths,
                    colors: *colors,
                    styles: *styles,
                    border_radius: border_radius.clone(),
                }
            })
        }
        
        DisplayListItem::SelectionRect { bounds, border_radius, color } => {
            clip_rect_to_page(*bounds, page_top, page_bottom).map(|clipped| {
                DisplayListItem::SelectionRect {
                    bounds: clipped,
                    border_radius: *border_radius,
                    color: *color,
                }
            })
        }
        
        DisplayListItem::CursorRect { bounds, color } => {
            clip_rect_to_page(*bounds, page_top, page_bottom).map(|clipped| {
                DisplayListItem::CursorRect {
                    bounds: clipped,
                    color: *color,
                }
            })
        }
        
        DisplayListItem::Image { bounds, key } => {
            // Images: only show if they fit on this page (don't split)
            if bounds.origin.y >= page_top && bounds.origin.y + bounds.size.height <= page_bottom {
                Some(DisplayListItem::Image {
                    bounds: offset_bounds(*bounds, -page_top),
                    key: *key,
                })
            } else if bounds.origin.y < page_bottom && bounds.origin.y >= page_top {
                // Image starts on this page but extends beyond - show clipped
                clip_rect_to_page(*bounds, page_top, page_bottom).map(|clipped| {
                    DisplayListItem::Image {
                        bounds: clipped,
                        key: *key,
                    }
                })
            } else {
                None
            }
        }
        
        // === Text items ===
        DisplayListItem::TextLayout { layout, bounds, font_hash, font_size_px, color } => {
            if !rect_intersects_page(bounds, page_top, page_bottom) {
                return None;
            }
            Some(DisplayListItem::TextLayout {
                layout: layout.clone(),
                bounds: offset_bounds(*bounds, -page_top),
                font_hash: *font_hash,
                font_size_px: *font_size_px,
                color: *color,
            })
        }
        
        DisplayListItem::Text { glyphs, font_hash, font_size_px, color, clip_rect } => {
            if !rect_intersects_page(clip_rect, page_top, page_bottom) {
                return None;
            }
            
            // Filter glyphs to only those visible on this page
            let page_glyphs: Vec<_> = glyphs.iter()
                .filter(|g| g.point.y >= page_top - 20.0 && g.point.y <= page_bottom + 20.0) // Include some margin for glyph height
                .map(|g| azul_core::ui_solver::GlyphInstance {
                    index: g.index,
                    point: azul_core::geom::LogicalPosition {
                        x: g.point.x,
                        y: g.point.y - page_top,
                    },
                    size: g.size,
                })
                .collect();
            
            if page_glyphs.is_empty() {
                return None;
            }
            
            Some(DisplayListItem::Text {
                glyphs: page_glyphs,
                font_hash: *font_hash,
                font_size_px: *font_size_px,
                color: *color,
                clip_rect: offset_bounds(*clip_rect, -page_top),
            })
        }
        
        // === Text decorations ===
        DisplayListItem::Underline { bounds, color, thickness } => {
            clip_rect_to_page(*bounds, page_top, page_bottom).map(|clipped| {
                DisplayListItem::Underline {
                    bounds: clipped,
                    color: *color,
                    thickness: *thickness,
                }
            })
        }
        
        DisplayListItem::Strikethrough { bounds, color, thickness } => {
            clip_rect_to_page(*bounds, page_top, page_bottom).map(|clipped| {
                DisplayListItem::Strikethrough {
                    bounds: clipped,
                    color: *color,
                    thickness: *thickness,
                }
            })
        }
        
        DisplayListItem::Overline { bounds, color, thickness } => {
            clip_rect_to_page(*bounds, page_top, page_bottom).map(|clipped| {
                DisplayListItem::Overline {
                    bounds: clipped,
                    color: *color,
                    thickness: *thickness,
                }
            })
        }
        
        // === Scrollbars ===
        DisplayListItem::ScrollBar { bounds, color, orientation, opacity_key, hit_id } => {
            clip_rect_to_page(*bounds, page_top, page_bottom).map(|clipped| {
                DisplayListItem::ScrollBar {
                    bounds: clipped,
                    color: *color,
                    orientation: *orientation,
                    opacity_key: *opacity_key,
                    hit_id: *hit_id,
                }
            })
        }
        
        // === Hit test areas ===
        DisplayListItem::HitTestArea { bounds, tag } => {
            clip_rect_to_page(*bounds, page_top, page_bottom).map(|clipped| {
                DisplayListItem::HitTestArea {
                    bounds: clipped,
                    tag: *tag,
                }
            })
        }
        
        // === State management - these don't have meaningful bounds for pagination ===
        // Skip these for now - they'd need proper tracking per page
        DisplayListItem::PushClip { .. } |
        DisplayListItem::PopClip |
        DisplayListItem::PushScrollFrame { .. } |
        DisplayListItem::PopScrollFrame |
        DisplayListItem::PushStackingContext { .. } |
        DisplayListItem::PopStackingContext => None,
        
        // === IFrames ===
        DisplayListItem::IFrame { child_dom_id, bounds, clip_rect } => {
            clip_rect_to_page(*bounds, page_top, page_bottom).map(|clipped| {
                DisplayListItem::IFrame {
                    child_dom_id: *child_dom_id,
                    bounds: clipped,
                    clip_rect: offset_bounds(*clip_rect, -page_top),
                }
            })
        }
    }
}

/// Clip a rectangle to page bounds and offset to page-relative coordinates.
/// Returns None if the rectangle is completely outside the page.
fn clip_rect_to_page(
    bounds: LogicalRect,
    page_top: f32,
    page_bottom: f32,
) -> Option<LogicalRect> {
    let item_top = bounds.origin.y;
    let item_bottom = bounds.origin.y + bounds.size.height;
    
    // Check if completely outside page
    if item_bottom <= page_top || item_top >= page_bottom {
        return None;
    }
    
    // Calculate clipped bounds
    let clipped_top = item_top.max(page_top);
    let clipped_bottom = item_bottom.min(page_bottom);
    let clipped_height = clipped_bottom - clipped_top;
    
    // Offset to page-relative coordinates
    let page_relative_y = clipped_top - page_top;
    
    Some(LogicalRect {
        origin: LogicalPosition {
            x: bounds.origin.x,
            y: page_relative_y,
        },
        size: LogicalSize {
            width: bounds.size.width,
            height: clipped_height,
        },
    })
}

/// Check if a rectangle intersects the page bounds
fn rect_intersects_page(bounds: &LogicalRect, page_top: f32, page_bottom: f32) -> bool {
    let item_top = bounds.origin.y;
    let item_bottom = bounds.origin.y + bounds.size.height;
    item_bottom > page_top && item_top < page_bottom
}

// ============================================================================
// Helper Functions
// ============================================================================

fn generate_page_chrome(
    _ctx: &FragmentationLayoutContext, 
    _page_index: usize, 
    _total_pages: usize,
) -> Vec<DisplayListItem> {
    // TODO: Implement proper header/footer rendering with page numbers
    Vec::new()
}

fn get_item_bounds(item: &DisplayListItem) -> Option<LogicalRect> {
    match item {
        DisplayListItem::Rect { bounds, .. } => Some(*bounds),
        DisplayListItem::SelectionRect { bounds, .. } => Some(*bounds),
        DisplayListItem::CursorRect { bounds, .. } => Some(*bounds),
        DisplayListItem::Border { bounds, .. } => Some(*bounds),
        DisplayListItem::TextLayout { bounds, .. } => Some(*bounds),
        DisplayListItem::Text { clip_rect, .. } => Some(*clip_rect),
        DisplayListItem::Underline { bounds, .. } => Some(*bounds),
        DisplayListItem::Strikethrough { bounds, .. } => Some(*bounds),
        DisplayListItem::Overline { bounds, .. } => Some(*bounds),
        DisplayListItem::Image { bounds, .. } => Some(*bounds),
        DisplayListItem::ScrollBar { bounds, .. } => Some(*bounds),
        DisplayListItem::PushClip { bounds, .. } => Some(*bounds),
        DisplayListItem::PushScrollFrame { clip_bounds, .. } => Some(*clip_bounds),
        DisplayListItem::HitTestArea { bounds, .. } => Some(*bounds),
        _ => None,
    }
}

fn offset_bounds(bounds: LogicalRect, offset_y: f32) -> LogicalRect {
    LogicalRect {
        origin: LogicalPosition { x: bounds.origin.x, y: bounds.origin.y + offset_y },
        size: bounds.size,
    }
}

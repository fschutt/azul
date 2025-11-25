//! CSS Paged Media layout integration
//!
//! This module provides functionality for laying out documents with pagination,
//! such as for PDF generation. It extends the regular layout engine with
//! page-aware layout that respects CSS break properties.

use std::collections::BTreeMap;

use azul_core::{
    dom::{DomId, NodeId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    hit_test::ScrollPosition,
    resources::RendererResources,
    selection::SelectionState,
    styled_dom::StyledDom,
};
use azul_css::LayoutDebugMessage;

use crate::{
    font_traits::{FontLoaderTrait, ParsedFontTrait, TextLayoutCache},
    paged::FragmentationContext,
    solver3::{
        cache::LayoutCache,
        display_list::{DisplayList, DisplayListItem},
        LayoutContext, LayoutError, Result,
    },
};

/// Layout a document with pagination, returning one DisplayList per page.
///
/// This function performs CSS Paged Media layout, splitting content across
/// multiple pages according to the provided FragmentationContext.
///
/// # Arguments
/// * `fragmentation_context` - Defines page size and fragmentainer properties
/// * Other arguments same as `layout_document()`
///
/// # Returns
/// A vector of DisplayLists, one per page. Each DisplayList contains the
/// elements that fit on that page, with Y-coordinates relative to the page origin.
#[cfg(feature = "text_layout")]
pub fn layout_document_paged<T: ParsedFontTrait + Sync + 'static, Q: FontLoaderTrait<T>>(
    cache: &mut LayoutCache<T>,
    text_cache: &mut TextLayoutCache<T>,
    fragmentation_context: FragmentationContext,
    new_dom: StyledDom,
    viewport: LogicalRect,
    font_manager: &crate::font_traits::FontManager<T, Q>,
    scroll_offsets: &BTreeMap<NodeId, ScrollPosition>,
    selections: &BTreeMap<DomId, SelectionState>,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    gpu_value_cache: Option<&azul_core::gpu::GpuValueCache>,
    renderer_resources: &RendererResources,
    id_namespace: azul_core::resources::IdNamespace,
    dom_id: DomId,
) -> Result<Vec<DisplayList>> {
    // Perform regular layout first (with infinite height to get full content)
    let display_list = super::layout_document(
        cache,
        text_cache,
        new_dom,
        viewport,
        font_manager,
        scroll_offsets,
        selections,
        debug_messages,
        gpu_value_cache,
        renderer_resources,
        id_namespace,
        dom_id,
    )?;
    
    // Split the display list into pages
    let page_height = match &fragmentation_context {
        FragmentationContext::Paged { page_size, .. } => page_size.height,
        FragmentationContext::Continuous { .. } => return Ok(vec![display_list]),
        FragmentationContext::MultiColumn { column_height, .. } => *column_height,
        FragmentationContext::Regions { regions } => {
            regions.first().map(|r| r.size.height).unwrap_or(display_list.items.iter()
                .filter_map(|item| get_item_max_y(item))
                .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                .unwrap_or(0.0))
        }
    };
    let pages = split_display_list_into_pages(display_list, page_height);
    
    Ok(pages)
}

/// Split a DisplayList into multiple pages based on Y coordinates.
/// 
/// This is a simple post-layout fragmentation that:
/// 1. Groups items by which page they belong to (based on Y coordinate)
/// 2. Adjusts Y coordinates to be page-relative
/// 3. Handles items that span page boundaries by duplicating/clipping them
fn split_display_list_into_pages(display_list: DisplayList, page_height: f32) -> Vec<DisplayList> {
    if page_height <= 0.0 {
        return vec![display_list];
    }
    
    // Calculate total content height to determine number of pages
    let max_y = display_list.items.iter()
        .filter_map(|item| get_item_max_y(item))
        .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .unwrap_or(0.0);
    
    let num_pages = ((max_y / page_height).ceil() as usize).max(1);
    
    // Create empty pages
    let mut pages: Vec<DisplayList> = (0..num_pages)
        .map(|_| DisplayList { items: Vec::new() })
        .collect();
    
    // Distribute items to pages
    for item in display_list.items {
        let item_bounds = get_item_bounds(&item);
        
        if let Some(bounds) = item_bounds {
            let start_page = (bounds.origin.y / page_height).floor() as usize;
            let end_page = ((bounds.origin.y + bounds.size.height) / page_height).ceil() as usize;
            
            // Item spans one or more pages
            for page_idx in start_page..end_page.min(num_pages) {
                let page_top = page_idx as f32 * page_height;
                let page_bottom = page_top + page_height;
                
                // Calculate the portion of the item visible on this page
                let visible_top = bounds.origin.y.max(page_top);
                let visible_bottom = (bounds.origin.y + bounds.size.height).min(page_bottom);
                
                if visible_bottom > visible_top {
                    // Adjust item for this page
                    if let Some(adjusted_item) = adjust_item_for_page(&item, page_top, page_height) {
                        pages[page_idx].items.push(adjusted_item);
                    }
                }
            }
        } else {
            // Items without bounds (like PushClip, PopClip) go to all pages
            // This is a simplification - proper handling would track stacking context
            for page in &mut pages {
                page.items.push(clone_display_item(&item));
            }
        }
    }
    
    pages
}

/// Get the bounds of a display list item, if it has spatial extent
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

/// Get the maximum Y coordinate of an item
fn get_item_max_y(item: &DisplayListItem) -> Option<f32> {
    get_item_bounds(item).map(|b| b.origin.y + b.size.height)
}

/// Adjust a display list item for a specific page by offsetting Y coordinates
fn adjust_item_for_page(item: &DisplayListItem, page_top: f32, _page_height: f32) -> Option<DisplayListItem> {
    let offset_y = -page_top;
    
    match item {
        DisplayListItem::Rect { bounds, color, border_radius } => {
            Some(DisplayListItem::Rect {
                bounds: offset_bounds(*bounds, offset_y),
                color: *color,
                border_radius: *border_radius,
            })
        }
        DisplayListItem::SelectionRect { bounds, border_radius, color } => {
            Some(DisplayListItem::SelectionRect {
                bounds: offset_bounds(*bounds, offset_y),
                border_radius: *border_radius,
                color: *color,
            })
        }
        DisplayListItem::CursorRect { bounds, color } => {
            Some(DisplayListItem::CursorRect {
                bounds: offset_bounds(*bounds, offset_y),
                color: *color,
            })
        }
        DisplayListItem::Border { bounds, widths, colors, styles, border_radius } => {
            Some(DisplayListItem::Border {
                bounds: offset_bounds(*bounds, offset_y),
                widths: *widths,
                colors: *colors,
                styles: *styles,
                border_radius: border_radius.clone(),
            })
        }
        DisplayListItem::TextLayout { layout, bounds, font_hash, font_size_px, color } => {
            Some(DisplayListItem::TextLayout {
                layout: layout.clone(),
                bounds: offset_bounds(*bounds, offset_y),
                font_hash: *font_hash,
                font_size_px: *font_size_px,
                color: *color,
            })
        }
        DisplayListItem::Text { glyphs, font_hash, font_size_px, color, clip_rect } => {
            // Offset each glyph's position
            let offset_glyphs: Vec<_> = glyphs.iter().map(|g| {
                azul_core::ui_solver::GlyphInstance {
                    index: g.index,
                    point: azul_core::geom::LogicalPosition {
                        x: g.point.x,
                        y: g.point.y + offset_y,
                    },
                    size: g.size,
                }
            }).collect();
            
            Some(DisplayListItem::Text {
                glyphs: offset_glyphs,
                font_hash: *font_hash,
                font_size_px: *font_size_px,
                color: *color,
                clip_rect: offset_bounds(*clip_rect, offset_y),
            })
        }
        DisplayListItem::Underline { bounds, color, thickness } => {
            Some(DisplayListItem::Underline {
                bounds: offset_bounds(*bounds, offset_y),
                color: *color,
                thickness: *thickness,
            })
        }
        DisplayListItem::Strikethrough { bounds, color, thickness } => {
            Some(DisplayListItem::Strikethrough {
                bounds: offset_bounds(*bounds, offset_y),
                color: *color,
                thickness: *thickness,
            })
        }
        DisplayListItem::Overline { bounds, color, thickness } => {
            Some(DisplayListItem::Overline {
                bounds: offset_bounds(*bounds, offset_y),
                color: *color,
                thickness: *thickness,
            })
        }
        DisplayListItem::Image { bounds, key } => {
            Some(DisplayListItem::Image {
                bounds: offset_bounds(*bounds, offset_y),
                key: *key,
            })
        }
        DisplayListItem::ScrollBar { bounds, color, orientation, opacity_key, hit_id } => {
            Some(DisplayListItem::ScrollBar {
                bounds: offset_bounds(*bounds, offset_y),
                color: *color,
                orientation: *orientation,
                opacity_key: *opacity_key,
                hit_id: *hit_id,
            })
        }
        DisplayListItem::PushClip { bounds, border_radius } => {
            Some(DisplayListItem::PushClip {
                bounds: offset_bounds(*bounds, offset_y),
                border_radius: border_radius.clone(),
            })
        }
        DisplayListItem::PopClip => Some(DisplayListItem::PopClip),
        DisplayListItem::PushScrollFrame { clip_bounds, content_size, scroll_id } => {
            Some(DisplayListItem::PushScrollFrame {
                clip_bounds: offset_bounds(*clip_bounds, offset_y),
                content_size: *content_size,
                scroll_id: *scroll_id,
            })
        }
        DisplayListItem::PopScrollFrame => Some(DisplayListItem::PopScrollFrame),
        DisplayListItem::HitTestArea { bounds, tag } => {
            Some(DisplayListItem::HitTestArea {
                bounds: offset_bounds(*bounds, offset_y),
                tag: *tag,
            })
        }
        DisplayListItem::IFrame { child_dom_id, bounds, clip_rect } => {
            Some(DisplayListItem::IFrame {
                child_dom_id: *child_dom_id,
                bounds: offset_bounds(*bounds, offset_y),
                clip_rect: offset_bounds(*clip_rect, offset_y),
            })
        }
        DisplayListItem::PushStackingContext { z_index, bounds } => {
            Some(DisplayListItem::PushStackingContext {
                z_index: *z_index,
                bounds: offset_bounds(*bounds, offset_y),
            })
        }
        DisplayListItem::PopStackingContext => Some(DisplayListItem::PopStackingContext),
    }
}

/// Clone a display list item (for items that need to appear on multiple pages)
fn clone_display_item(item: &DisplayListItem) -> DisplayListItem {
    match item {
        DisplayListItem::Rect { bounds, color, border_radius } => {
            DisplayListItem::Rect { bounds: *bounds, color: *color, border_radius: *border_radius }
        }
        DisplayListItem::SelectionRect { bounds, border_radius, color } => {
            DisplayListItem::SelectionRect { bounds: *bounds, border_radius: *border_radius, color: *color }
        }
        DisplayListItem::CursorRect { bounds, color } => {
            DisplayListItem::CursorRect { bounds: *bounds, color: *color }
        }
        DisplayListItem::Border { bounds, widths, colors, styles, border_radius } => {
            DisplayListItem::Border { bounds: *bounds, widths: *widths, colors: *colors, styles: *styles, border_radius: border_radius.clone() }
        }
        DisplayListItem::TextLayout { layout, bounds, font_hash, font_size_px, color } => {
            DisplayListItem::TextLayout { layout: layout.clone(), bounds: *bounds, font_hash: *font_hash, font_size_px: *font_size_px, color: *color }
        }
        DisplayListItem::Text { glyphs, font_hash, font_size_px, color, clip_rect } => {
            DisplayListItem::Text { glyphs: glyphs.clone(), font_hash: *font_hash, font_size_px: *font_size_px, color: *color, clip_rect: *clip_rect }
        }
        DisplayListItem::Underline { bounds, color, thickness } => {
            DisplayListItem::Underline { bounds: *bounds, color: *color, thickness: *thickness }
        }
        DisplayListItem::Strikethrough { bounds, color, thickness } => {
            DisplayListItem::Strikethrough { bounds: *bounds, color: *color, thickness: *thickness }
        }
        DisplayListItem::Overline { bounds, color, thickness } => {
            DisplayListItem::Overline { bounds: *bounds, color: *color, thickness: *thickness }
        }
        DisplayListItem::Image { bounds, key } => {
            DisplayListItem::Image { bounds: *bounds, key: *key }
        }
        DisplayListItem::ScrollBar { bounds, color, orientation, opacity_key, hit_id } => {
            DisplayListItem::ScrollBar { bounds: *bounds, color: *color, orientation: *orientation, opacity_key: *opacity_key, hit_id: *hit_id }
        }
        DisplayListItem::PushClip { bounds, border_radius } => {
            DisplayListItem::PushClip { bounds: *bounds, border_radius: border_radius.clone() }
        }
        DisplayListItem::PopClip => DisplayListItem::PopClip,
        DisplayListItem::PushScrollFrame { clip_bounds, content_size, scroll_id } => {
            DisplayListItem::PushScrollFrame { clip_bounds: *clip_bounds, content_size: *content_size, scroll_id: *scroll_id }
        }
        DisplayListItem::PopScrollFrame => DisplayListItem::PopScrollFrame,
        DisplayListItem::HitTestArea { bounds, tag } => {
            DisplayListItem::HitTestArea { bounds: *bounds, tag: *tag }
        }
        DisplayListItem::IFrame { child_dom_id, bounds, clip_rect } => {
            DisplayListItem::IFrame { child_dom_id: *child_dom_id, bounds: *bounds, clip_rect: *clip_rect }
        }
        DisplayListItem::PushStackingContext { z_index, bounds } => {
            DisplayListItem::PushStackingContext { z_index: *z_index, bounds: *bounds }
        }
        DisplayListItem::PopStackingContext => DisplayListItem::PopStackingContext,
    }
}

/// Offset bounds by a Y amount
fn offset_bounds(bounds: LogicalRect, offset_y: f32) -> LogicalRect {
    LogicalRect {
        origin: LogicalPosition {
            x: bounds.origin.x,
            y: bounds.origin.y + offset_y,
        },
        size: bounds.size,
    }
}

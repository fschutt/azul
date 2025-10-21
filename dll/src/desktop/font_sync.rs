//! Font synchronization system - bridges azul-layout's Arc<ParsedFont> system
//! with WebRender's FontKey system.
//!
//! The problem: Layout uses Arc<ParsedFont> with font_hash, but the old CSS system
//! uses StyleFontFamilyHash. We need to scan the display list AFTER layout to collect
//! all actually-used fonts (including fallbacks) and register them with WebRender.

use std::collections::HashSet;

use azul_core::resources::{
    Au, DpiScaleFactor, FontInstanceKey, FontInstanceOptions, FontInstancePlatformOptions,
    FontKey, IdNamespace, RendererResources, FONT_INSTANCE_FLAG_NO_AUTOHINT,
};
use azul_layout::{solver3::display_list::DisplayListItem, window::DomLayoutResult};

/// Scans all display lists in layout_results and collects used font hashes.
///
/// This should be called AFTER layout_and_generate_display_list() to discover
/// which fonts (including fallbacks) are actually used.
pub fn collect_used_fonts(layout_results: &std::collections::BTreeMap<azul_core::dom::DomId, DomLayoutResult>) -> HashSet<u64> {
    let mut font_hashes = HashSet::new();

    for (_dom_id, result) in layout_results {
        for item in &result.display_list.items {
            if let DisplayListItem::Text { font_hash, .. } = item {
                font_hashes.insert(*font_hash);
            }
        }
    }

    font_hashes
}

/// Registers fonts with WebRender that are not yet in font_hash_map.
///
/// This adds the mapping font_hash → FontKey and creates FontInstanceKeys
/// for each required size/DPI combination.
///
/// Returns Vec<ResourceUpdate> to send to WebRender.
pub fn register_missing_fonts(
    font_hashes: &HashSet<u64>,
    renderer_resources: &mut RendererResources,
    layout_results: &std::collections::BTreeMap<azul_core::dom::DomId, DomLayoutResult>,
    id_namespace: IdNamespace,
    dpi: DpiScaleFactor,
    txn: &mut webrender::Transaction,
) -> Result<(), String> {
    use azul_core::resources::FontRenderMode;

    // Find which fonts need registration
    let missing_fonts: Vec<u64> = font_hashes
        .iter()
        .filter(|hash| !renderer_resources.font_hash_map.contains_key(hash))
        .copied()
        .collect();

    if missing_fonts.is_empty() {
        return Ok(());
    }

    eprintln!("[FontSync] Registering {} missing fonts", missing_fonts.len());

    // For each missing font, we need to:
    // 1. Get Arc<ParsedFont> from display list (it's embedded in Text items via GlyphRun)
    // 2. Generate FontKey
    // 3. Add font bytes to WebRender
    // 4. Store mapping in font_hash_map

    // Problem: Display list only has font_hash, not Arc<ParsedFont>!
    // We need to get it from somewhere else.
    //
    // Solution: The text_cache in LayoutWindow should have all loaded fonts.
    // But we don't have access to it here.
    //
    // Alternative: Collect font_hash → Arc<ParsedFont> mapping during layout
    // and store it in LayoutWindow or DomLayoutResult.

    // TODO: For now, log error and return
    eprintln!("[FontSync] ERROR: Cannot register fonts - Arc<ParsedFont> not accessible from display list");
    eprintln!("[FontSync] Missing fonts: {:?}", missing_fonts);

    Ok(())
}

/// Complete font synchronization flow:
/// 1. Collect used font hashes from display lists
/// 2. Register missing fonts with WebRender
/// 3. Create font instances for required sizes
pub fn sync_fonts_after_layout(
    layout_results: &std::collections::BTreeMap<azul_core::dom::DomId, DomLayoutResult>,
    renderer_resources: &mut RendererResources,
    id_namespace: IdNamespace,
    dpi: DpiScaleFactor,
    txn: &mut webrender::Transaction,
) -> Result<(), String> {
    let used_fonts = collect_used_fonts(layout_results);

    if used_fonts.is_empty() {
        return Ok(());
    }

    register_missing_fonts(
        &used_fonts,
        renderer_resources,
        layout_results,
        id_namespace,
        dpi,
        txn,
    )
}

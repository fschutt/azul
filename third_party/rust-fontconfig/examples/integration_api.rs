//! Integration API example
//!
//! Shows the complete workflow for integrating rust-fontconfig into a text
//! layout pipeline — from CSS font-family resolution to loading font bytes.
//!
//! Run with:
//!   cargo run --example integration_api

use rust_fontconfig::{FcFontCache, FcWeight, FontId, PatternMatch};

fn main() {
    println!("=== Font Integration Pipeline ===\n");

    // ── Step 1: Build font cache ──
    println!("Step 1: Build font cache");
    let cache = FcFontCache::build();
    println!("  {} fonts loaded\n", cache.list().len());

    // ── Step 2: Resolve CSS font-family ──
    let css_families = vec![
        "Helvetica".to_string(),
        "Arial".to_string(),
        "sans-serif".to_string(),
    ];
    println!("Step 2: Resolve font-family: {:?}\n", css_families);

    let chain = cache.resolve_font_chain(
        &css_families,
        FcWeight::Normal,
        PatternMatch::False,
        PatternMatch::False,
        &mut Vec::new(),
    );

    for (i, group) in chain.css_fallbacks.iter().enumerate() {
        print!("  [{}] '{}': {} fonts", i + 1, group.css_name, group.fonts.len());
        if let Some(first) = group.fonts.first() {
            if let Some(meta) = cache.get_metadata_by_id(&first.id) {
                print!(
                    " (first: {:?})",
                    meta.name.as_ref().or(meta.family.as_ref())
                );
            }
        }
        println!();
    }
    println!(
        "  + {} unicode fallback fonts\n",
        chain.unicode_fallbacks.len()
    );

    // ── Step 3: Resolve text to font runs ──
    let text = "Hello 世界! Привет мир";
    println!("Step 3: Resolve text: '{}'\n", text);

    let resolved = chain.resolve_text(&cache, text);

    // Group by runs of same font
    let mut runs: Vec<(String, Option<FontId>)> = Vec::new();
    let mut current_text = String::new();
    let mut current_id: Option<FontId> = None;

    for (ch, info) in &resolved {
        let this_id = info.as_ref().map(|(id, _)| *id);
        if this_id != current_id {
            if !current_text.is_empty() {
                runs.push((current_text.clone(), current_id));
                current_text.clear();
            }
            current_id = this_id;
        }
        current_text.push(*ch);
    }
    if !current_text.is_empty() {
        runs.push((current_text, current_id));
    }

    println!("  Font runs:");
    for (run_text, font_id) in &runs {
        let name = font_id
            .as_ref()
            .and_then(|id| cache.get_metadata_by_id(id))
            .and_then(|m| m.name.clone().or(m.family.clone()))
            .unwrap_or_else(|| "[NO FONT]".into());
        println!("    '{}' -> {}", run_text, name);
    }

    // ── Step 4: Load font bytes ──
    let unique_fonts: std::collections::HashSet<_> =
        runs.iter().filter_map(|(_, id)| *id).collect();

    println!(
        "\nStep 4: Load fonts ({} unique needed)\n",
        unique_fonts.len()
    );
    for font_id in &unique_fonts {
        if let Some(meta) = cache.get_metadata_by_id(font_id) {
            let name = meta
                .name
                .as_ref()
                .or(meta.family.as_ref())
                .map(|s| s.as_str())
                .unwrap_or("?");
            if let Some(source) = cache.get_font_by_id(font_id) {
                match source {
                    rust_fontconfig::OwnedFontSource::Disk(path) => {
                        println!("  {} -> {}", name, path.path);
                    }
                    rust_fontconfig::OwnedFontSource::Memory(font) => {
                        println!("  {} -> memory (id: {})", name, font.id);
                    }
                }
            }
        }
    }

    println!("\nPipeline summary:");
    println!("  1. FcFontCache::build()       — once at startup");
    println!("  2. cache.resolve_font_chain() — per CSS font-family");
    println!("  3. chain.resolve_text()       — per text run");
    println!("  4. cache.get_font_by_id()     — load bytes for shaping");
}

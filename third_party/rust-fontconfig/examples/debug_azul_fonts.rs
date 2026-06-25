//! Debug example for testing System Font resolution.
//!
//! This example demonstrates the italic race condition fix:
//! When querying "System Font" with italic=False, the result should
//! ALWAYS be a non-italic font, regardless of font discovery order.
//!
//! Run with: cargo run --example debug_azul_fonts --features "std,parsing"

use rust_fontconfig::{FcFontCache, FcPattern, FcWeight, PatternMatch};

fn main() {
    println!("=== System Font Italic Race Condition Test ===\n");
    
    // Build the cache (scans all system fonts)
    println!("Building font cache (scanning system fonts)...");
    let cache = FcFontCache::build();
    let all_fonts = cache.list();
    println!("Cache built: {} patterns\n", all_fonts.len());
    
    // FIRST: dump ALL fonts that contain "system" or "sfns" in their name
    println!("--- ALL fonts containing 'system' or 'sfns' in name/family ---");
    for (pattern, id) in all_fonts {
        let name_lower = pattern.name.as_deref().unwrap_or("").to_lowercase();
        let family_lower = pattern.family.as_deref().unwrap_or("").to_lowercase();
        if name_lower.contains("system") || name_lower.contains("sfns") 
           || family_lower.contains("system") || family_lower.contains("sfns") {
            let style_score = FcFontCache::calculate_style_score(
                &FcPattern {
                    weight: FcWeight::Normal,
                    italic: PatternMatch::False,
                    oblique: PatternMatch::False,
                    ..Default::default()
                },
                &pattern,
            );
            println!(
                "  id={} name={:?} family={:?} italic={:?} bold={:?} weight={:?} stretch={:?} style_score={} subfamily={:?}",
                id,
                pattern.name.as_deref().unwrap_or("?"),
                pattern.family.as_deref().unwrap_or("?"),
                pattern.italic,
                pattern.bold,
                pattern.weight,
                pattern.stretch,
                style_score,
                pattern.metadata.font_subfamily.as_deref().unwrap_or("?"),
            );
        }
    }
    println!();
    
    // Query "System Font" with italic=False
    let families = vec!["System Font".to_string()];
    let mut trace = Vec::new();
    
    let chain = cache.resolve_font_chain(
        &families,
        FcWeight::Normal,
        PatternMatch::False,  // italic=False
        PatternMatch::False,  // oblique=False
        &mut trace,
    );
    
    println!("Font chain for 'System Font' (italic=False, weight=Normal):");
    for group in &chain.css_fallbacks {
        println!("  CSS name: '{}'", group.css_name);
        for (i, font_match) in group.fonts.iter().enumerate() {
            let meta = cache.get_metadata_by_id(&font_match.id);
            if let Some(pattern) = meta {
                let style_score = FcFontCache::calculate_style_score(
                    &FcPattern {
                        weight: FcWeight::Normal,
                        italic: PatternMatch::False,
                        oblique: PatternMatch::False,
                        ..Default::default()
                    },
                    &pattern,
                );
                println!(
                    "    [{}] id={} name={:?} family={:?} italic={:?} bold={:?} weight={:?} style_score={} subfamily={:?}",
                    i,
                    font_match.id,
                    pattern.name.as_deref().unwrap_or("?"),
                    pattern.family.as_deref().unwrap_or("?"),
                    pattern.italic,
                    pattern.bold,
                    pattern.weight,
                    style_score,
                    pattern.metadata.font_subfamily.as_deref().unwrap_or("?"),
                );
            } else {
                println!("    [{}] id={} (no metadata)", i, font_match.id);
            }
        }
    }
    
    // Verify: the first matched font should NOT be italic
    let first_font = chain.css_fallbacks
        .iter()
        .flat_map(|g| g.fonts.iter())
        .next();
    
    if let Some(font) = first_font {
        if let Some(meta) = cache.get_metadata_by_id(&font.id) {
            if meta.italic == PatternMatch::True {
                eprintln!("\n❌ FAIL: First matched font is ITALIC!");
                eprintln!("   Font: {:?} (id={})", meta.name, font.id);
                eprintln!("   This is the italic race condition bug.");
                std::process::exit(1);
            } else {
                println!("\n✅ PASS: First matched font is NOT italic.");
                println!("   Font: {:?} (id={})", meta.name, font.id);
            }
        }
    } else {
        eprintln!("\n⚠ WARNING: No fonts matched 'System Font'");
    }
    
    // Also test with DontCare (original default before the fix)
    println!("\n--- Comparison: italic=DontCare ---");
    let mut trace2 = Vec::new();
    let chain2 = cache.resolve_font_chain(
        &families,
        FcWeight::Normal,
        PatternMatch::DontCare,
        PatternMatch::DontCare,
        &mut trace2,
    );
    
    for group in &chain2.css_fallbacks {
        for (i, font_match) in group.fonts.iter().enumerate() {
            if let Some(pattern) = cache.get_metadata_by_id(&font_match.id) {
                println!(
                    "    [{}] {:?} italic={:?} weight={:?}",
                    i,
                    pattern.name.as_deref().unwrap_or("?"),
                    pattern.italic,
                    pattern.weight,
                );
            }
        }
    }
    
    // Run the test 10 times to verify determinism
    println!("\n--- Determinism check (10 runs) ---");
    let mut all_same = true;
    let mut first_result = None;
    
    for run in 0..10 {
        // Clear chain cache to force re-resolution
        let mut trace_n = Vec::new();
        let chain_n = cache.resolve_font_chain(
            &families,
            FcWeight::Normal,
            PatternMatch::False,
            PatternMatch::False,
            &mut trace_n,
        );
        
        let first_id = chain_n.css_fallbacks
            .iter()
            .flat_map(|g| g.fonts.iter())
            .next()
            .map(|f| f.id);
        
        if let Some(id) = first_id {
            if first_result.is_none() {
                first_result = Some(id);
            } else if first_result != Some(id) {
                println!("    Run {}: id={} — DIFFERENT from first run!", run, id);
                all_same = false;
            }
        }
    }
    
    if all_same {
        println!("    All 10 runs returned the same first font. ✅");
    } else {
        println!("    Results varied across runs! ❌");
    }
    
    println!("\n=== Test complete ===");
}

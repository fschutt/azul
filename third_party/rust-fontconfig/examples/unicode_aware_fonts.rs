//! Unicode-aware font resolution
//!
//! Shows how font chains handle multi-script text by automatically selecting
//! the right font for each character (Latin, CJK, Arabic, Cyrillic, etc.).
//!
//! Run with:
//!   cargo run --example unicode_aware_fonts

use rust_fontconfig::{FcFontCache, FcWeight, FontFallbackChain, PatternMatch};

fn main() {
    let cache = FcFontCache::build();

    println!("=== Unicode-Aware Font Selection ===\n");

    // Create a font chain for sans-serif
    let chain = cache.resolve_font_chain(
        &["sans-serif".to_string()],
        FcWeight::Normal,
        PatternMatch::False,
        PatternMatch::False,
        &mut Vec::new(),
    );

    println!(
        "Font chain: {} CSS fallbacks, {} unicode fallbacks\n",
        chain.css_fallbacks.len(),
        chain.unicode_fallbacks.len()
    );

    // Resolve different scripts
    let texts = [
        ("Latin", "Hello World"),
        ("CJK", "你好世界"),
        ("Japanese", "こんにちは世界"),
        ("Arabic", "مرحبا بالعالم"),
        ("Cyrillic", "Привет мир"),
        ("Mixed", "Hello 世界 Привет"),
    ];

    for (label, text) in &texts {
        println!("{} text: '{}'", label, text);
        print_resolution(&cache, &chain, text);
        println!();
    }

    println!("Workflow:");
    println!("  1. resolve_font_chain() — creates fallback chain from CSS font-family");
    println!("  2. chain.resolve_text()  — maps each character to a font");
    println!("  3. Use font IDs to load and render glyphs");
}

fn print_resolution(cache: &FcFontCache, chain: &FontFallbackChain, text: &str) {
    let resolved = chain.resolve_text(cache, text);

    let mut current_font: Option<String> = None;
    let mut segment = String::new();

    for (ch, info) in &resolved {
        let font_name = info.as_ref().and_then(|(id, _)| {
            cache
                .get_metadata_by_id(id)
                .and_then(|m| m.name.clone().or(m.family.clone()))
        });
        if font_name != current_font {
            if !segment.is_empty() {
                println!(
                    "  '{}' -> {}",
                    segment,
                    current_font.as_deref().unwrap_or("[NO FONT]")
                );
                segment.clear();
            }
            current_font = font_name;
        }
        segment.push(*ch);
    }
    if !segment.is_empty() {
        println!(
            "  '{}' -> {}",
            segment,
            current_font.as_deref().unwrap_or("[NO FONT]")
        );
    }
}

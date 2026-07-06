//! Character resolution example
//!
//! Demonstrates how to resolve individual characters to fonts using font chains.
//! Useful for debugging font coverage issues (e.g. CJK, emoji, symbols).
//!
//! Run with:
//!   cargo run --example character_resolution

use rust_fontconfig::{FcFontCache, FcWeight, PatternMatch};

fn main() {
    let cache = FcFontCache::build();

    // Create a font chain with typical web defaults
    let families = vec!["system-ui".to_string(), "sans-serif".to_string()];

    let chain = cache.resolve_font_chain(
        &families,
        FcWeight::Normal,
        PatternMatch::False,
        PatternMatch::False,
        &mut Vec::new(),
    );

    // Test characters from different Unicode blocks
    let test_chars = [
        ('A', "Latin Capital Letter A"),
        ('a', "Latin Small Letter A"),
        ('0', "Digit Zero"),
        ('€', "Euro Sign"),
        ('→', "Rightwards Arrow"),
        ('中', "CJK Ideograph - China"),
        ('日', "CJK Ideograph - Sun/Day"),
        ('あ', "Hiragana Letter A"),
        ('ア', "Katakana Letter A"),
        ('한', "Hangul Syllable Han"),
        ('א', "Hebrew Letter Alef"),
        ('ا', "Arabic Letter Alef"),
        ('α', "Greek Small Letter Alpha"),
        ('я', "Cyrillic Small Letter Ya"),
        ('🙂', "Slightly Smiling Face"),
        ('♠', "Black Spade Suit"),
        ('∑', "N-ary Summation"),
        ('∞', "Infinity"),
    ];

    println!("Character resolution results:\n");
    println!("{:<6} {:<30} {:<40}", "Char", "Description", "Font");
    println!("{}", "-".repeat(76));

    for (ch, description) in &test_chars {
        let text = ch.to_string();
        let resolved = chain.resolve_text(&cache, &text);

        let font_name = resolved
            .first()
            .and_then(|(_, info)| info.as_ref())
            .and_then(|(id, _)| cache.get_metadata_by_id(id))
            .and_then(|m| m.name.clone().or(m.family.clone()))
            .unwrap_or_else(|| "⚠ NOT FOUND".to_string());

        println!("{:<6} {:<30} {}", ch, description, font_name);
    }

    // Check specific font coverage
    println!("\n\nArial coverage check:");
    let arial_chain = cache.resolve_font_chain(
        &["Arial".to_string()],
        FcWeight::Normal,
        PatternMatch::False,
        PatternMatch::False,
        &mut Vec::new(),
    );

    let arial_result = cache.query(
        &rust_fontconfig::FcPattern {
            family: Some("Arial".to_string()),
            ..Default::default()
        },
        &mut Vec::new(),
    );

    if let Some(arial_match) = arial_result {
        for ch in ['A', '中', '🙂', '→'] {
            let resolved = arial_chain.resolve_text(&cache, &ch.to_string());
            let in_arial = resolved
                .first()
                .and_then(|(_, info)| info.as_ref())
                .map(|(id, _)| id == &arial_match.id)
                .unwrap_or(false);

            println!(
                "  {} '{}' (U+{:04X})",
                if in_arial { "✓" } else { "✗" },
                ch,
                ch as u32
            );
        }
    } else {
        println!("  Arial not found on this system");
    }
}

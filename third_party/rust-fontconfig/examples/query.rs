//! Detailed font query patterns
//!
//! Shows how to query by family name, style (bold/italic), generic families,
//! list fonts, and search by name substring.
//!
//! Run with:
//!   cargo run --example query

use rust_fontconfig::{FcFontCache, FcPattern, FcWeight, PatternMatch};

fn main() {
    println!("Building font cache...");
    let cache = FcFontCache::build();
    println!("Font cache built with {} fonts\n", cache.list().len());

    // ── Query by family name ──
    println!("=== Query by Family Name ===");
    let mut trace = Vec::new();
    let pattern = FcPattern {
        family: Some("Arial".to_string()),
        ..Default::default()
    };

    if let Some(result) = cache.query(&pattern, &mut trace) {
        println!("Found Arial:");
        println!("  Font ID: {:?}", result.id);
        if let Some(meta) = cache.get_metadata_by_id(&result.id) {
            println!("  Family: {:?}", meta.family);
            println!("  Weight: {:?}", meta.weight);
            println!("  Italic: {:?}", meta.italic);
        }
        if let Some(source) = cache.get_font_by_id(&result.id) {
            match source {
                rust_fontconfig::OwnedFontSource::Disk(path) => println!("  Path: {}", path.path),
                rust_fontconfig::OwnedFontSource::Memory(font) => {
                    println!("  Memory font: {}", font.id)
                }
            }
        }
    } else {
        println!("Arial not found");
    }

    // ── Query generic family ──
    println!("\n=== Query Generic 'serif' ===");
    trace.clear();
    if let Some(result) = cache.query(
        &FcPattern {
            family: Some("serif".to_string()),
            ..Default::default()
        },
        &mut trace,
    ) {
        if let Some(meta) = cache.get_metadata_by_id(&result.id) {
            println!(
                "Found: {:?}",
                meta.name.as_ref().or(meta.family.as_ref())
            );
        }
    }

    // ── Query by style (bold + italic) ──
    println!("\n=== Query Bold Italic ===");
    trace.clear();
    if let Some(result) = cache.query(
        &FcPattern {
            family: Some("sans-serif".to_string()),
            weight: FcWeight::Bold,
            italic: PatternMatch::True,
            ..Default::default()
        },
        &mut trace,
    ) {
        if let Some(meta) = cache.get_metadata_by_id(&result.id) {
            println!(
                "Found: {:?} weight={:?} italic={:?}",
                meta.name.as_ref().or(meta.family.as_ref()),
                meta.weight,
                meta.italic
            );
        }
    }

    // ── List bold fonts ──
    println!("\n=== First 5 Bold Fonts ===");
    for (meta, id) in cache
        .list()
        .into_iter()
        .filter(|(m, _)| matches!(m.weight, FcWeight::Bold | FcWeight::ExtraBold | FcWeight::Black))
        .take(5)
    {
        println!(
            "  {:?}: {:?}",
            id,
            meta.name.as_ref().or(meta.family.as_ref())
        );
    }

    // ── Search by name substring ──
    println!("\n=== Fonts with 'Mono' in name ===");
    for (meta, _id) in cache.list().into_iter().filter(|(m, _)| {
        m.name
            .as_ref()
            .map(|n| n.contains("Mono"))
            .unwrap_or(false)
            || m.family
                .as_ref()
                .map(|f| f.contains("Mono"))
                .unwrap_or(false)
    }).take(10) {
        println!(
            "  {:?}",
            meta.name.as_ref().or(meta.family.as_ref())
        );
    }
}

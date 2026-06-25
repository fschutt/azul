//! Basic font query example (synchronous API)
//!
//! Demonstrates the classic `FcFontCache::build()` API which scans ALL system
//! fonts upfront. This works but is slow (~700ms on a system with 1000 fonts).
//! For faster startup, see the `registry` example.
//!
//! Run with:
//!   cargo run --example getfont

use rust_fontconfig::{FcFontCache, FcPattern, FcWeight};
use std::time::Instant;

fn main() {
    // Build the cache — scans and parses ALL system fonts
    let start = Instant::now();
    let cache = FcFontCache::build();
    let build_time = start.elapsed();

    println!("Cache built: {} fonts in {:?}\n", cache.list().len(), build_time);

    // Query various fonts to showcase fuzzy matching
    let queries = [
        ("Arial", FcWeight::Normal, "Common sans-serif"),
        ("Helvetica", FcWeight::Bold, "Bold variant"),
        ("Courier", FcWeight::Normal, "Monospace"),
        ("Georgia", FcWeight::Normal, "Serif"),
        ("sans-serif", FcWeight::Normal, "Generic family"),
    ];

    for (name, weight, desc) in &queries {
        let t = Instant::now();
        let result = cache.query(
            &FcPattern {
                name: Some(name.to_string()),
                weight: *weight,
                ..Default::default()
            },
            &mut Vec::new(),
        );

        match result {
            Some(fm) => {
                let found = cache
                    .get_metadata_by_id(&fm.id)
                    .and_then(|m| m.name.clone().or(m.family.clone()))
                    .unwrap_or_else(|| format!("{:?}", fm.id));
                println!(
                    "  ✓ '{}' ({}) -> {} [{:?}]",
                    name, desc, found, t.elapsed()
                );
            }
            None => println!("  ✗ '{}' ({}) -> NOT FOUND [{:?}]", name, desc, t.elapsed()),
        }
    }
}

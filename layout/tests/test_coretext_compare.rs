//! Compare our hinted glyph rendering against macOS CoreText.
//!
//! Run: cargo test -p azul-layout --features coretext_tests --test test_coretext_compare -- --nocapture

#![cfg(all(target_os = "macos", feature = "coretext_tests"))]

use std::fs;
use core_foundation::base::TCFType;
use core_graphics::color_space::CGColorSpace;
use core_graphics::context::CGContext;
use core_graphics::geometry::{CGPoint, CGRect, CGSize};
use core_text::font as ct_font;
use tiny_skia::{Pixmap, Paint, FillRule, Transform, Color};

use azul_layout::font::parsed::ParsedFont;
use azul_layout::glyph_cache::GlyphCache;

/// Render a single glyph with CoreText into a grayscale bitmap.
fn render_coretext_glyph(ch: char, font_size: f32, w: u32, h: u32) -> Option<Vec<u8>> {
    let ct = ct_font::new_from_name("Times New Roman", font_size as f64).ok()?;

    // Get glyph
    let chars = [ch as u16];
    let mut glyphs = [0u16; 1];
    unsafe {
        ct.get_glyphs_for_characters(chars.as_ptr(), glyphs.as_mut_ptr(), 1);
    }
    if glyphs[0] == 0 { return None; }
    let glyph = glyphs[0];

    // Create grayscale bitmap context
    let cs = CGColorSpace::create_device_gray();
    let mut ctx = CGContext::create_bitmap_context(
        None, w as usize, h as usize, 8, w as usize, &cs, 0,
    );

    // White background
    ctx.set_rgb_fill_color(1.0, 1.0, 1.0, 1.0);
    ctx.fill_rect(CGRect::new(&CGPoint::new(0.0, 0.0), &CGSize::new(w as f64, h as f64)));

    // Draw glyph in black with grayscale AA (no subpixel)
    ctx.set_rgb_fill_color(0.0, 0.0, 0.0, 1.0);
    ctx.set_allows_font_smoothing(false);
    ctx.set_should_smooth_fonts(false);
    ctx.set_allows_antialiasing(true);
    ctx.set_should_antialias(true);

    let baseline_y = font_size as f64 * 0.25;
    let positions = [CGPoint::new(2.0, baseline_y)];
    ct.draw_glyphs(&[glyph], &positions, ctx.clone());

    let data = ctx.data();
    Some(data.to_vec())
}

/// Render a glyph with our pipeline into a grayscale bitmap.
fn render_azul_glyph(font: &ParsedFont, ch: char, font_size: f32, w: u32, h: u32) -> Option<Vec<u8>> {
    let glyph_id = font.lookup_glyph_index(ch as u32)?;
    let owned = font.glyph_records_decoded.get(&glyph_id)?;
    let ppem = font_size.round() as u16;

    let mut cache = GlyphCache::new();
    let cached = cache.get_or_build(0, glyph_id, owned, font, ppem)?;

    let mut pixmap = Pixmap::new(w, h)?;
    pixmap.fill(Color::WHITE);

    let mut paint = Paint::default();
    paint.set_color(Color::BLACK);
    paint.anti_alias = true;

    // Match CoreText baseline: CoreText Y is bottom-up, tiny-skia is top-down
    let baseline_y = font_size * 0.75 + font_size * 0.25;
    let transform = if cached.is_hinted {
        Transform::from_translate(2.0, baseline_y)
    } else {
        let scale = font_size / font.font_metrics.units_per_em as f32;
        Transform::from_scale(scale, scale).post_translate(2.0, baseline_y)
    };

    pixmap.fill_path(cached.path, &paint, FillRule::Winding, transform, None);

    // Convert RGBA to grayscale (luminance)
    let rgba = pixmap.data();
    let mut gray = vec![0u8; (w * h) as usize];
    for i in 0..(w * h) as usize {
        let r = rgba[i * 4] as u32;
        let g = rgba[i * 4 + 1] as u32;
        let b = rgba[i * 4 + 2] as u32;
        gray[i] = ((r * 299 + g * 587 + b * 114) / 1000) as u8;
    }
    Some(gray)
}

/// Save grayscale bitmap as PGM.
fn save_pgm(path: &str, data: &[u8], w: u32, h: u32) {
    let header = format!("P5\n{} {}\n255\n", w, h);
    let mut pgm = header.into_bytes();
    pgm.extend_from_slice(data);
    fs::write(path, &pgm).ok();
}

#[test]
fn test_coretext_vs_azul_alphabet() {
    let font_bytes = fs::read("/System/Library/Fonts/Supplemental/Times New Roman.ttf")
        .or_else(|_| fs::read("/System/Library/Fonts/Times.ttc"))
        .ok();
    let font_bytes = match font_bytes {
        Some(b) => b,
        None => { eprintln!("Skipping: Times not found"); return; }
    };
    let mut warnings = Vec::new();
    let font = match ParsedFont::from_bytes(&font_bytes, 0, &mut warnings) {
        Some(f) => f,
        None => { eprintln!("Failed to parse font"); return; }
    };

    let test_chars: Vec<char> = "LoremABCDEFGHIJKabcdefghijklmnopqrst0123456789".chars().collect();
    let test_sizes = [12.0f32, 14.0, 16.0, 20.0, 24.0, 32.0, 48.0];

    eprintln!("\n=== CoreText vs Azul glyph comparison ===");
    eprintln!("{:>4} {:>5} {:>8} {:>8}", "char", "size", "diff_px", "max_val");

    let mut worst_diff = 0usize;
    let mut worst_char = ' ';
    let mut worst_size = 0.0f32;

    for &size in &test_sizes {
        let w = (size * 2.0) as u32;
        let h = (size * 2.0) as u32;

        for &ch in &test_chars {
            let ct = match render_coretext_glyph(ch, size, w, h) {
                Some(r) => r,
                None => continue,
            };
            let az = match render_azul_glyph(&font, ch, size, w, h) {
                Some(r) => r,
                None => continue,
            };

            // Count pixels with diff > threshold
            let mut diff_count = 0usize;
            let mut max_diff = 0u8;
            for i in 0..(w * h) as usize {
                let d = (ct[i] as i16 - az[i] as i16).unsigned_abs() as u8;
                if d > 20 {
                    diff_count += 1;
                    if d > max_diff { max_diff = d; }
                }
            }

            if diff_count > 0 {
                eprintln!("{:>4} {:>5.0} {:>8} {:>8}", ch, size, diff_count, max_diff);
            }
            if diff_count > worst_diff {
                worst_diff = diff_count;
                worst_char = ch;
                worst_size = size;
            }
        }
    }

    // Save worst case side-by-side
    if worst_diff > 0 {
        let w = (worst_size * 2.0) as u32;
        let h = (worst_size * 2.0) as u32;
        let ct = render_coretext_glyph(worst_char, worst_size, w, h).unwrap();
        let az = render_azul_glyph(&font, worst_char, worst_size, w, h).unwrap();

        save_pgm("/tmp/coretext_glyph.pgm", &ct, w, h);
        save_pgm("/tmp/azul_glyph.pgm", &az, w, h);

        // Diff image
        let diff: Vec<u8> = ct.iter().zip(az.iter())
            .map(|(&a, &b)| {
                let d = (a as i16 - b as i16).unsigned_abs() as u8;
                255 - d.min(255)
            })
            .collect();
        save_pgm("/tmp/glyph_diff.pgm", &diff, w, h);

        eprintln!("\nWorst: '{}' at {}px ({} diff pixels)", worst_char, worst_size, worst_diff);
        eprintln!("Saved: /tmp/coretext_glyph.pgm, /tmp/azul_glyph.pgm, /tmp/glyph_diff.pgm");
    }
}

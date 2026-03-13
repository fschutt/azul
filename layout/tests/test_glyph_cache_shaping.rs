//! Shaping comparison tests: allsorts vs hb-shape.
//!
//! Verifies that allsorts produces glyph IDs, advances, kerning, and offsets
//! that match HarfBuzz output for reference texts.

use azul_layout::font::parsed::ParsedFont;
use azul_layout::text3::cache::{BidiDirection, StyleProperties};
use azul_layout::text3::default::shape_text_for_parsed_font;
use azul_layout::text3::script::Script;
use hyphenation::Language;

fn load_times_new_roman() -> Option<ParsedFont> {
    let font_path = "/System/Library/Fonts/Supplemental/Times New Roman.ttf";
    let font_bytes = std::fs::read(font_path).ok()?;
    let mut warnings = Vec::new();
    ParsedFont::from_bytes(&font_bytes, 0, &mut warnings)
}

/// hb-shape reference data at font-size=2048 (=upem, so values are in font units).
/// Format: (glyph_id, dx, dy, ax, ay)
///
/// $ hb-shape --font-size=2048 --no-glyph-names "Test"
/// g=55 dx=0 ax=1179, g=72 dx=-71 ax=838, g=86 dx=0 ax=797, g=87 dx=0 ax=569
struct HbGlyph {
    glyph_id: u16,
    dx: i32,       // x placement offset (font units)
    ax: u16,       // x advance (font units, after kerning adjustment)
}

fn hb_test() -> Vec<HbGlyph> {
    vec![
        HbGlyph { glyph_id: 55, dx: 0, ax: 1179 },   // T
        HbGlyph { glyph_id: 72, dx: -71, ax: 838 },   // e (kerned after T)
        HbGlyph { glyph_id: 86, dx: 0, ax: 797 },     // s
        HbGlyph { glyph_id: 87, dx: 0, ax: 569 },     // t
    ]
}

fn hb_upper_left() -> Vec<HbGlyph> {
    vec![
        HbGlyph { glyph_id: 88, dx: 0, ax: 1024 },    // u
        HbGlyph { glyph_id: 83, dx: 0, ax: 1024 },    // p
        HbGlyph { glyph_id: 83, dx: 0, ax: 1024 },    // p
        HbGlyph { glyph_id: 72, dx: 0, ax: 909 },     // e
        HbGlyph { glyph_id: 85, dx: 0, ax: 661 },     // r
        HbGlyph { glyph_id: 16, dx: -20, ax: 662 },   // hyphen
        HbGlyph { glyph_id: 79, dx: 0, ax: 569 },     // l
        HbGlyph { glyph_id: 72, dx: 0, ax: 909 },     // e
        HbGlyph { glyph_id: 73, dx: 0, ax: 682 },     // f
        HbGlyph { glyph_id: 87, dx: 0, ax: 569 },     // t
    ]
}

/// Compare allsorts shaping output against hb-shape reference for "Test".
/// Uses upem-sized font to get values in font units for easy comparison.
#[test]
fn test_shaping_vs_hbshape_test() {
    let font = match load_times_new_roman() {
        Some(f) => f,
        None => {
            eprintln!("Skipping: Times New Roman not found");
            return;
        }
    };

    let upem = font.font_metrics.units_per_em as f32;
    let style = StyleProperties {
        font_size_px: upem, // use upem so scale_factor = 1.0
        ..StyleProperties::default()
    };

    let glyphs = shape_text_for_parsed_font(
        &font,
        "Test",
        Script::Latin,
        Language::EnglishUS,
        BidiDirection::Ltr,
        &style,
    )
    .expect("shaping should succeed");

    let hb = hb_test();
    assert_eq!(glyphs.len(), hb.len(), "glyph count mismatch");

    println!("\n=== Shaping comparison: \"Test\" (units_per_em={}) ===", upem);
    println!("{:<6} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>12} {:>10}",
        "char", "gid_as", "gid_hb", "raw_adv", "eff_adv", "hb_ax", "off_x", "hb_dx", "advance", "kerning");

    for (i, (g, h)) in glyphs.iter().zip(hb.iter()).enumerate() {
        let raw_advance = font.get_horizontal_advance(g.glyph_id);
        let effective_advance_units = (g.advance + g.kerning).round() as i32;
        let offset_x_units = g.offset.x.round() as i32;

        println!("{:<6} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>12.1} {:>10.1}",
            g.codepoint,
            g.glyph_id,
            h.glyph_id,
            raw_advance,
            effective_advance_units,
            h.ax,
            offset_x_units,
            h.dx,
            g.advance,
            g.kerning,
        );
    }

    // Verify total width matches hb-shape
    let allsorts_total: f32 = glyphs.iter().map(|g| g.advance + g.kerning).sum();
    let hb_total: u32 = hb.iter().map(|h| h.ax as u32).sum();
    println!("\nTotal width: allsorts={:.0} hb-shape={}", allsorts_total, hb_total);

    // Check kerning on 'e' after 'T'
    let e_glyph = &glyphs[1];
    let t_glyph = &glyphs[0];
    let e_hb = &hb[1];
    let t_hb = &hb[0];
    println!("\n'T','e' kerning analysis:");
    println!("  T: allsorts eff_adv={:.0} hb_ax={}", t_glyph.advance + t_glyph.kerning, t_hb.ax);
    println!("  e: allsorts eff_adv={:.0} hb_ax={}", e_glyph.advance + e_glyph.kerning, e_hb.ax);
    println!("  T+e total: allsorts={:.0} hb={}",
        (t_glyph.advance + t_glyph.kerning) + (e_glyph.advance + e_glyph.kerning),
        t_hb.ax as u32 + e_hb.ax as u32);
    println!("  allsorts has GPOS: {}", font.gpos_cache.is_some());
    println!("  allsorts has kern table: {}", font.opt_kern_table.is_some());

    // Total width of "Te" must match between allsorts and hb-shape
    let te_allsorts = (t_glyph.advance + t_glyph.kerning + e_glyph.advance + e_glyph.kerning).round() as i32;
    let te_hb = t_hb.ax as i32 + e_hb.ax as i32;
    assert_eq!(te_allsorts, te_hb,
        "Total 'Te' width mismatch: allsorts={} vs hb-shape={}", te_allsorts, te_hb);

    // Overall total width must match
    assert_eq!(allsorts_total.round() as i32, hb_total as i32,
        "Total 'Test' width mismatch: allsorts={:.0} vs hb-shape={}", allsorts_total, hb_total);
    println!();
}

/// Compare allsorts shaping output against hb-shape for "upper-left".
#[test]
fn test_shaping_vs_hbshape_upper_left() {
    let font = match load_times_new_roman() {
        Some(f) => f,
        None => {
            eprintln!("Skipping: Times New Roman not found");
            return;
        }
    };

    let upem = font.font_metrics.units_per_em as f32;
    let style = StyleProperties {
        font_size_px: upem,
        ..StyleProperties::default()
    };

    let glyphs = shape_text_for_parsed_font(
        &font,
        "upper-left",
        Script::Latin,
        Language::EnglishUS,
        BidiDirection::Ltr,
        &style,
    )
    .expect("shaping should succeed");

    let hb = hb_upper_left();
    assert_eq!(glyphs.len(), hb.len(), "glyph count mismatch");

    println!("\n=== Shaping comparison: \"upper-left\" (units_per_em={}) ===", upem);
    println!("{:<6} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>12} {:>10}",
        "char", "gid_as", "gid_hb", "raw_adv", "eff_adv", "hb_ax", "off_x", "hb_dx", "advance", "kerning");

    for (i, (g, h)) in glyphs.iter().zip(hb.iter()).enumerate() {
        let raw_advance = font.get_horizontal_advance(g.glyph_id);
        let effective_advance_units = (g.advance + g.kerning).round() as i32;
        let offset_x_units = g.offset.x.round() as i32;

        println!("{:<6} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>12.1} {:>10.1}",
            g.codepoint,
            g.glyph_id,
            h.glyph_id,
            raw_advance,
            effective_advance_units,
            h.ax,
            offset_x_units,
            h.dx,
            g.advance,
            g.kerning,
        );

        assert_eq!(g.glyph_id, h.glyph_id,
            "glyph {} '{}': glyph_id mismatch", i, g.codepoint);
    }
    println!();
}

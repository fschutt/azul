//! Compare our hinted glyph rendering against macOS CoreText.
//!
//! Multi-pass comparison:
//! 1. Binary pass: threshold to black/white, compare which pixels are "ink" vs "paper"
//! 2. Grayscale pass: compare the actual anti-aliasing coverage values
//!
//! Run: cargo test -p azul-layout --features coretext_tests --test test_coretext_compare -- --nocapture

#![cfg(all(target_os = "macos", feature = "coretext_tests"))]

use std::fs;
use core_graphics::color_space::CGColorSpace;
use core_graphics::context::CGContext;
use core_graphics::geometry::{CGPoint, CGRect, CGSize};
use core_text::font as ct_font;
use tiny_skia::{Pixmap, Paint, FillRule, Transform, Color};

use azul_layout::font::parsed::ParsedFont;
use azul_layout::glyph_cache::GlyphCache;

// ── Bitmap helpers ──────────────────────────────────────────────────

/// Grayscale bitmap with known dimensions.
struct GrayBitmap {
    data: Vec<u8>,
    w: u32,
    h: u32,
}

impl GrayBitmap {
    /// Find bounding box of non-white pixels (ink region).
    fn ink_bbox(&self) -> Option<(u32, u32, u32, u32)> {
        let (mut x0, mut y0, mut x1, mut y1) = (self.w, self.h, 0u32, 0u32);
        for y in 0..self.h {
            for x in 0..self.w {
                if self.data[(y * self.w + x) as usize] < 250 {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x);
                    y1 = y1.max(y);
                }
            }
        }
        if x1 >= x0 && y1 >= y0 { Some((x0, y0, x1, y1)) } else { None }
    }

    /// Crop to ink bounding box with padding.
    fn crop_to_ink(&self, pad: u32) -> Option<GrayBitmap> {
        let (x0, y0, x1, y1) = self.ink_bbox()?;
        let cx0 = x0.saturating_sub(pad);
        let cy0 = y0.saturating_sub(pad);
        let cx1 = (x1 + pad + 1).min(self.w);
        let cy1 = (y1 + pad + 1).min(self.h);
        let cw = cx1 - cx0;
        let ch = cy1 - cy0;
        let mut data = vec![255u8; (cw * ch) as usize];
        for y in 0..ch {
            for x in 0..cw {
                data[(y * cw + x) as usize] = self.data[((cy0 + y) * self.w + cx0 + x) as usize];
            }
        }
        Some(GrayBitmap { data, w: cw, h: ch })
    }

    /// Binarize: pixel < threshold → 0 (black), else → 255 (white).
    fn binarize(&self, threshold: u8) -> GrayBitmap {
        GrayBitmap {
            data: self.data.iter().map(|&v| if v < threshold { 0 } else { 255 }).collect(),
            w: self.w, h: self.h,
        }
    }

    /// Save as PGM file.
    fn save_pgm(&self, path: &str) {
        let mut out = format!("P5\n{} {}\n255\n", self.w, self.h).into_bytes();
        out.extend_from_slice(&self.data);
        fs::write(path, &out).ok();
    }

    /// Save as PNG via tiny-skia (grayscale → RGBA), scaled up 4x for visibility.
    fn save_png(&self, path: &str) {
        let scale = 4u32;
        let sw = self.w * scale;
        let sh = self.h * scale;
        if let Some(mut pm) = Pixmap::new(sw, sh) {
            for y in 0..self.h {
                for x in 0..self.w {
                    let v = self.data[(y * self.w + x) as usize];
                    let px = tiny_skia::PremultipliedColorU8::from_rgba(v, v, v, 255).unwrap();
                    for dy in 0..scale {
                        for dx in 0..scale {
                            pm.pixels_mut()[((y*scale+dy) * sw + x*scale+dx) as usize] = px;
                        }
                    }
                }
            }
            pm.save_png(path).ok();
        }
    }

    /// Create side-by-side image: [a | b | diff].
    fn side_by_side(a: &GrayBitmap, b: &GrayBitmap) -> GrayBitmap {
        let w = a.w.max(b.w);
        let h = a.h.max(b.h);
        let total_w = w * 3 + 4; // a | b | diff with 2px gaps
        let mut data = vec![200u8; (total_w * h) as usize]; // gray background
        // Copy a
        for y in 0..a.h.min(h) {
            for x in 0..a.w.min(w) {
                data[(y * total_w + x) as usize] = a.data[(y * a.w + x) as usize];
            }
        }
        // Copy b
        for y in 0..b.h.min(h) {
            for x in 0..b.w.min(w) {
                data[(y * total_w + w + 2 + x) as usize] = b.data[(y * b.w + x) as usize];
            }
        }
        // Diff
        for y in 0..a.h.min(b.h).min(h) {
            for x in 0..a.w.min(b.w).min(w) {
                let va = a.data[(y * a.w + x) as usize] as i16;
                let vb = b.data[(y * b.w + x) as usize] as i16;
                let d = (va - vb).unsigned_abs() as u8;
                data[(y * total_w + w * 2 + 4 + x) as usize] = 255 - d;
            }
        }
        GrayBitmap { data, w: total_w, h }
    }
}

/// Comparison result between two bitmaps.
struct CompareResult {
    /// Number of pixels differing above threshold.
    diff_count: usize,
    /// Maximum pixel value difference.
    max_diff: u8,
    /// Total absolute difference (sum of all pixel diffs).
    total_diff: u64,
}

/// Compare two same-size grayscale bitmaps.
fn compare_bitmaps(a: &GrayBitmap, b: &GrayBitmap, threshold: u8) -> CompareResult {
    let n = (a.w.min(b.w) * a.h.min(b.h)) as usize;
    let mut diff_count = 0;
    let mut max_diff = 0u8;
    let mut total_diff = 0u64;
    for i in 0..n {
        let ai = if i < a.data.len() { a.data[i] } else { 255 };
        let bi = if i < b.data.len() { b.data[i] } else { 255 };
        let d = (ai as i16 - bi as i16).unsigned_abs() as u8;
        total_diff += d as u64;
        if d > threshold {
            diff_count += 1;
            if d > max_diff { max_diff = d; }
        }
    }
    CompareResult { diff_count, max_diff, total_diff }
}

// ── CoreText rendering ─────────────────────────────────────────────

/// Render a glyph with CoreText (macOS native) into a grayscale bitmap.
/// Uses `-webkit-font-smoothing: antialiased` equivalent (grayscale AA, no LCD).
fn render_coretext(ch: char, font_name: &str, font_size: f32, w: u32, h: u32, baseline_y: f32) -> Option<GrayBitmap> {
    let ct = ct_font::new_from_name(font_name, font_size as f64).ok()?;

    let chars = [ch as u16];
    let mut glyphs = [0u16; 1];
    unsafe { ct.get_glyphs_for_characters(chars.as_ptr(), glyphs.as_mut_ptr(), 1); }
    if glyphs[0] == 0 { return None; }

    let cs = CGColorSpace::create_device_gray();
    let mut ctx = CGContext::create_bitmap_context(
        None, w as usize, h as usize, 8, w as usize, &cs, 0,
    );
    ctx.set_rgb_fill_color(1.0, 1.0, 1.0, 1.0);
    ctx.fill_rect(CGRect::new(&CGPoint::new(0.0, 0.0), &CGSize::new(w as f64, h as f64)));

    ctx.set_rgb_fill_color(0.0, 0.0, 0.0, 1.0);
    ctx.set_allows_font_smoothing(false);
    ctx.set_should_smooth_fonts(false);
    ctx.set_allows_antialiasing(false);
    ctx.set_should_antialias(false);

    // CoreText Y is bottom-up: baseline_y from bottom
    let ct_baseline = baseline_y as f64;
    ct.draw_glyphs(&[glyphs[0]], &[CGPoint::new(1.0, ct_baseline)], ctx.clone());

    // CGBitmapContextGetData returns rows top-down (row 0 = top).
    // No flip needed — this matches tiny-skia's coordinate system.
    let data = ctx.data().to_vec();
    Some(GrayBitmap { data, w, h })
}

// ── Azul rendering ─────────────────────────────────────────────────

/// Render a glyph with our hinted pipeline into a grayscale bitmap.
fn render_azul(font: &ParsedFont, ch: char, font_size: f32, w: u32, h: u32, baseline_y: f32) -> Option<GrayBitmap> {
    let glyph_id = font.lookup_glyph_index(ch as u32)?;
    let owned = font.glyph_records_decoded.get(&glyph_id)?;
    let ppem = font_size.round() as u16;

    let mut cache = GlyphCache::new();
    let cached = cache.get_or_build(0, glyph_id, owned, font, ppem)?;

    let mut pixmap = Pixmap::new(w, h)?;
    pixmap.fill(Color::WHITE);

    let mut paint = Paint::default();
    paint.set_color(Color::BLACK);
    paint.anti_alias = false; // binary coverage, no AA — matches CoreText no-AA mode

    // tiny-skia Y is top-down: baseline_y from top
    let transform = if cached.is_hinted {
        Transform::from_translate(1.0, baseline_y)
    } else {
        let scale = font_size / font.font_metrics.units_per_em as f32;
        Transform::from_scale(scale, scale).post_translate(1.0, baseline_y)
    };

    pixmap.fill_path(cached.path, &paint, FillRule::Winding, transform, None);

    // RGBA → grayscale
    let rgba = pixmap.data();
    let mut gray = vec![0u8; (w * h) as usize];
    for i in 0..(w * h) as usize {
        let r = rgba[i * 4] as u32;
        let g = rgba[i * 4 + 1] as u32;
        let b = rgba[i * 4 + 2] as u32;
        gray[i] = ((r * 299 + g * 587 + b * 114) / 1000) as u8;
    }
    Some(GrayBitmap { data: gray, w, h })
}

// ── Comparison helpers ─────────────────────────────────────────────

/// Render both CoreText and Azul, auto-align by ink bbox, return cropped pair.
fn render_aligned_pair(
    font: &ParsedFont, ch: char, font_name: &str, font_size: f32,
) -> Option<(GrayBitmap, GrayBitmap)> {
    let w = (font_size * 3.0) as u32;
    let h = (font_size * 2.5) as u32;

    // Use font metrics for baseline positioning
    let upem = font.font_metrics.units_per_em as f32;
    let ascent_px = (font.font_metrics.ascent as f32 / upem * font_size).ceil();

    // Baseline is ascent pixels from the top of the bitmap.
    let baseline_from_top = ascent_px;

    // Azul: Y-down coordinate system. translate_y = baseline_from_top.
    let az_baseline = baseline_from_top;

    // CoreText: Y-up coordinate system (Y=0 at bottom).
    // We pass baseline_y = distance from bottom.
    // After we flip the bitmap (bottom-up → top-down), a point at
    // Y=d from bottom becomes Y=(h-d) from top.
    // We want it at baseline_from_top from top, so:
    //   h - d = baseline_from_top  →  d = h - baseline_from_top
    let ct_baseline = h as f32 - baseline_from_top;

    let ct = render_coretext(ch, font_name, font_size, w, h, ct_baseline)?;
    let az = render_azul(font, ch, font_size, w, h, az_baseline)?;

    // Use UNION of both ink bboxes so glyphs stay vertically aligned
    let ct_bb = ct.ink_bbox()?;
    let az_bb = az.ink_bbox()?;
    let x0 = ct_bb.0.min(az_bb.0).saturating_sub(2);
    let y0 = ct_bb.1.min(az_bb.1).saturating_sub(2);
    let x1 = (ct_bb.2.max(az_bb.2) + 3).min(w);
    let y1 = (ct_bb.3.max(az_bb.3) + 3).min(h);
    let cw = x1 - x0;
    let ch = y1 - y0;

    let crop_region = |bmp: &GrayBitmap| -> GrayBitmap {
        let mut data = vec![255u8; (cw * ch) as usize];
        for y in 0..ch {
            for x in 0..cw {
                let sx = x0 + x;
                let sy = y0 + y;
                if sx < bmp.w && sy < bmp.h {
                    data[(y * cw + x) as usize] = bmp.data[(sy * bmp.w + sx) as usize];
                }
            }
        }
        GrayBitmap { data, w: cw, h: ch }
    };

    Some((crop_region(&ct), crop_region(&az)))
}

/// Run comparison for a single glyph, return (binary_result, grayscale_result).
fn compare_glyph(
    font: &ParsedFont, ch: char, font_name: &str, font_size: f32,
) -> Option<(CompareResult, CompareResult)> {
    let (ct, az) = render_aligned_pair(font, ch, font_name, font_size)?;

    // Make same size (pad smaller to match larger)
    let w = ct.w.max(az.w);
    let h = ct.h.max(az.h);
    let pad_to = |bmp: &GrayBitmap, tw: u32, th: u32| -> GrayBitmap {
        let mut data = vec![255u8; (tw * th) as usize];
        for y in 0..bmp.h.min(th) {
            for x in 0..bmp.w.min(tw) {
                data[(y * tw + x) as usize] = bmp.data[(y * bmp.w + x) as usize];
            }
        }
        GrayBitmap { data, w: tw, h: th }
    };
    let ct_pad = pad_to(&ct, w, h);
    let az_pad = pad_to(&az, w, h);

    // Pass 1: Binary (is the right pixel inked?)
    let ct_bin = ct_pad.binarize(128);
    let az_bin = az_pad.binarize(128);
    let bin_result = compare_bitmaps(&ct_bin, &az_bin, 0);

    // Pass 2: Grayscale (how much does the AA coverage differ?)
    let gray_result = compare_bitmaps(&ct_pad, &az_pad, 10);

    Some((bin_result, gray_result))
}

// ── Tests ──────────────────────────────────────────────────────────

fn load_times() -> Option<ParsedFont> {
    let bytes = fs::read("/System/Library/Fonts/Supplemental/Times New Roman.ttf")
        .or_else(|_| fs::read("/System/Library/Fonts/Times.ttc"))
        .ok()?;
    let mut w = Vec::new();
    ParsedFont::from_bytes(&bytes, 0, &mut w)
}

/// Pass 1: Binary comparison — are we inking the right pixels?
#[test]
fn test_binary_pixel_coverage() {
    let font = match load_times() { Some(f) => f, None => { eprintln!("Skip"); return; } };
    let chars: Vec<char> = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789".chars().collect();
    let sizes = [6.0f32, 8.0, 10.0, 12.0, 14.0, 16.0, 20.0, 24.0, 32.0, 48.0];

    eprintln!("\n=== BINARY (ink coverage) comparison ===");
    eprintln!("{:>4} {:>5} {:>8} {:>6}", "char", "size", "bin_diff", "max");

    let mut worst: Vec<(char, f32, usize)> = Vec::new();

    for &size in &sizes {
        for &ch in &chars {
            if let Some((bin, _)) = compare_glyph(&font, ch, "Times New Roman", size) {
                if bin.diff_count > 0 {
                    eprintln!("{:>4} {:>5.0} {:>8} {:>6}", ch, size, bin.diff_count, bin.max_diff);
                    worst.push((ch, size, bin.diff_count));
                }
            }
        }
    }

    worst.sort_by(|a, b| b.2.cmp(&a.2));
    eprintln!("\nTop 10 worst binary diffs:");
    for (ch, size, count) in worst.iter().take(10) {
        eprintln!("  '{}' at {}px: {} pixels wrong", ch, size, count);
    }

    // Save worst case
    if let Some(&(ch, size, _)) = worst.first() {
        if let Some((ct, az)) = render_aligned_pair(&font, ch, "Times New Roman", size) {
            let sbs = GrayBitmap::side_by_side(&ct, &az);
            sbs.save_png("/tmp/binary_worst_sbs.png");
            let ct_bin = ct.binarize(128);
            let az_bin = az.binarize(128);
            let sbs_bin = GrayBitmap::side_by_side(&ct_bin, &az_bin);
            sbs_bin.save_png("/tmp/binary_worst_sbs_bin.png");
            eprintln!("Saved /tmp/binary_worst_sbs.png and /tmp/binary_worst_sbs_bin.png");
        }
    }
}

/// Pass 2: Grayscale comparison — how does anti-aliasing coverage differ?
#[test]
fn test_grayscale_aa_coverage() {
    let font = match load_times() { Some(f) => f, None => { eprintln!("Skip"); return; } };
    let chars: Vec<char> = "Loremipsumdolorsitamet".chars().collect();
    let sizes = [6.0f32, 8.0, 10.0, 14.0, 16.0, 20.0, 32.0];

    eprintln!("\n=== GRAYSCALE (AA coverage) comparison ===");
    eprintln!("{:>4} {:>5} {:>8} {:>6} {:>10}", "char", "size", "gray_d", "max", "total_d");

    for &size in &sizes {
        for &ch in &chars {
            if let Some((_, gray)) = compare_glyph(&font, ch, "Times New Roman", size) {
                if gray.diff_count > 0 {
                    eprintln!("{:>4} {:>5.0} {:>8} {:>6} {:>10}",
                        ch, size, gray.diff_count, gray.max_diff, gray.total_diff);
                }
            }
        }
    }

    // Save ALL tested glyphs to /tmp for inspection
    let all_chars: Vec<char> = "Loremipsumdolorsitamet".chars().collect();
    for &size in &sizes {
        for &ch in &all_chars {
            if let Some((ct, az)) = render_aligned_pair(&font, ch, "Times New Roman", size) {
                let sbs = GrayBitmap::side_by_side(&ct, &az);
                let name = format!("/tmp/glyph_{}_{:.0}px_sbs.png", ch, size);
                sbs.save_png(&name);
            }
        }
    }
    eprintln!("\nSaved all glyphs to /tmp/glyph_<char>_<size>px_sbs.png");
}

/// Comprehensive: all chars at all sizes, sorted by total grayscale diff.
#[test]
fn test_comprehensive_ranking() {
    let font = match load_times() { Some(f) => f, None => { eprintln!("Skip"); return; } };
    let chars: Vec<char> = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789".chars().collect();
    let sizes = [6.0f32, 8.0, 10.0, 12.0, 14.0, 16.0, 20.0, 24.0, 32.0, 48.0];

    let mut results: Vec<(char, f32, usize, usize, u64)> = Vec::new();

    for &size in &sizes {
        for &ch in &chars {
            if let Some((bin, gray)) = compare_glyph(&font, ch, "Times New Roman", size) {
                results.push((ch, size, bin.diff_count, gray.diff_count, gray.total_diff));
            }
        }
    }

    // Sort by total grayscale diff
    results.sort_by(|a, b| b.4.cmp(&a.4));

    eprintln!("\n=== COMPREHENSIVE RANKING (by total grayscale diff) ===");
    eprintln!("{:>4} {:>5} {:>8} {:>8} {:>10}", "char", "size", "bin_diff", "gray_d", "total_d");
    for &(ch, size, bin, gray, total) in results.iter().take(20) {
        eprintln!("{:>4} {:>5.0} {:>8} {:>8} {:>10}", ch, size, bin, gray, total);
    }
    eprintln!("\nTotal glyphs compared: {}", results.len());
    let perfect = results.iter().filter(|r| r.2 == 0 && r.3 == 0).count();
    eprintln!("Perfect matches (0 diff): {}", perfect);
}

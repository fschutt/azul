#![cfg(all(target_os = "macos", feature = "coretext_tests"))]
#![allow(dead_code)]
//! CoreText-vs-azul hinting **autoregression** harness.
//!
//! Goal: autoregressively converge our hinted, CPU-rendered glyphs onto macOS
//! CoreText output. An LLM inspects the PNGs (upscaled 16x with a pixel grid,
//! 4 panels per case: `[ours-hinted | coretext | diff-heatmap | ours-unhinted]`)
//! and ranks worst-first using the machine-readable `metrics.jsonl`.
//!
//! Run:
//!   cargo test -p azul-layout --features coretext_tests \
//!       --test coretext_autoregression --release -- --nocapture
//! or `scripts/coretext_regression.sh` (rotates metrics + prints prev-vs-now deltas).
//!
//! ## Why this supersedes `test_coretext_compare.rs`
//! That file `use`s `tiny_skia`, which is NOT a dependency of `azul-layout`
//! (see `cpurender/pixmap.rs`: "Replaces tiny_skia::Pixmap"), and passes an
//! `Arc<OwnedGlyph>` where `&OwnedGlyph` is now required — so it no longer
//! compiles. This harness renders OURS through the **real product rasterizer**
//! (`agg_rust` + `GlyphCache::get_or_build` → `build_hinted_path`), which is
//! what `cpurender/raster.rs::render_text` uses, so what we measure is what
//! ships.
//!
//! ## Divergence-class -> suspect-code map (keep in sync with the shell script)
//! - over-ink everywhere .......... gamma/coverage in `cpurender/raster.rs`
//!                                  (agg has no gamma; CoreText applies a text
//!                                  gamma even with smoothing off)
//! - 1px vertical shift ........... phantom-point / baseline rounding in
//!                                  `glyph_cache.rs` (build_hinted_path) or the
//!                                  y-flip (build_path_from_contours negates Y)
//! - stems 1px too far @ small ppem CVT cut-in / round state in
//!                                  `third_party/allsorts/src/hinting/interpreter.rs`
//! - identical to unhinted ........ hinting not running: check `gasp`,
//!                                  `hint_instance`, or `build_hinted_path`
//!                                  returning None (metrics field `all_hinted`)
//!
//! ## Metrics / bucket definitions
//! - `rms_raw`     : coverage RMS over the union ink-bbox, glyphs at the SAME
//!                   pixel coords (no shifting). This is the honest, canonical
//!                   number used for ranking / bucket / filename — it counts BOTH
//!                   wrong shape AND wrong position (a 1px baseline slip is a real
//!                   defect we must NOT hide).
//! - `rms_aligned` : coverage RMS after aligning the two ink-bbox origins. Isolates
//!                   pure SHAPE fidelity — if `rms_aligned` is small but `rms_raw`
//!                   is large, the glyph is the right shape in the wrong place
//!                   (baseline / advance rounding), not a bad outline.
//! - bucket (on `rms_raw`): MATCH (<2) / CLOSE (<8) / DIVERGENT (>=8).

use std::fs;
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use agg_rust::basics::FillingRule;
use agg_rust::color::Rgba8;
use agg_rust::path_storage::PathStorage;
use agg_rust::pixfmt_rgba::PixfmtRgba32;
use agg_rust::rasterizer_scanline_aa::RasterizerScanlineAa;
use agg_rust::renderer_base::RendererBase;
use agg_rust::renderer_scanline::render_scanlines_aa_solid;
use agg_rust::rendering_buffer::RowAccessor;
use agg_rust::scanline_u::ScanlineU8;
use agg_rust::trans_affine::TransAffine;

use allsorts::hinting::f26dot6::{compute_scale, F26Dot6};

use azul_layout::font::parsed::{OwnedGlyph, ParsedFont};
use azul_layout::glyph_cache::{build_path_from_contours, GlyphCache};

use core_foundation::attributed_string::CFMutableAttributedString;
use core_foundation::base::{CFRange, TCFType};
use core_foundation::string::CFString;
use core_graphics::color_space::CGColorSpace;
use core_graphics::context::CGContext;
use core_graphics::data_provider::CGDataProvider;
use core_graphics::font::CGFont;
use core_graphics::geometry::{CGPoint, CGRect, CGSize};
use core_text::font as ct_font;
use core_text::font::CTFont;
use core_text::line::CTLine;
use core_text::string_attributes::kCTFontAttributeName;

// ── Bitmap ──────────────────────────────────────────────────────────

/// Grayscale bitmap (row 0 = top), matching both agg's `RowAccessor` buffer
/// layout AND CoreGraphics bitmap memory (top-down while the CG *coordinate*
/// space is y-up — the two cancel, see baseline math below).
struct GrayBitmap {
    data: Vec<u8>,
    w: u32,
    h: u32,
}

impl GrayBitmap {
    #[inline]
    fn at(&self, x: u32, y: u32) -> u8 {
        self.data[(y * self.w + x) as usize]
    }

    /// Inclusive ink bbox (`x0,y0,x1,y1`) of pixels darker than paper.
    /// Threshold 250 matches `test_coretext_compare.rs::ink_bbox`.
    fn ink_bbox(&self) -> Option<(u32, u32, u32, u32)> {
        let (mut x0, mut y0, mut x1, mut y1) = (self.w, self.h, 0u32, 0u32);
        let mut any = false;
        for y in 0..self.h {
            for x in 0..self.w {
                if self.at(x, y) < 250 {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x);
                    y1 = y1.max(y);
                    any = true;
                }
            }
        }
        if any {
            Some((x0, y0, x1, y1))
        } else {
            None
        }
    }

    /// Count of inked pixels (gray < 250).
    fn ink_pixels(&self) -> u32 {
        self.data.iter().filter(|&&v| v < 250).count() as u32
    }
}

// ── OURS: rasterize hinted / unhinted glyph runs via agg ────────────

/// Scale the raw outline points to F26Dot6 pixel coords WITHOUT running the
/// interpreter, then build the same path `build_hinted_path` would build.
/// This is the honest "hinting OFF" reference for the 4th panel and the
/// fallback when hinting is unavailable — same path builder, same coordinate
/// space (pixels, Y already negated by `build_path_from_contours`), only the
/// interpreter run differs.
fn unhinted_pixel_path(owned: &OwnedGlyph, ppem: u16, upem: u16) -> Option<PathStorage> {
    let rp = owned.raw_points.as_ref()?;
    let oc = owned.raw_on_curve.as_ref()?;
    let ce = owned.raw_contour_ends.as_ref()?;
    if rp.is_empty() || ce.is_empty() {
        return None;
    }
    let scale = compute_scale(ppem, upem);
    let pts: Vec<(i32, i32)> = rp
        .iter()
        .map(|&(x, y)| {
            (
                F26Dot6::from_funits(i32::from(x), scale).to_bits(),
                F26Dot6::from_funits(i32::from(y), scale).to_bits(),
            )
        })
        .collect();
    build_path_from_contours(&pts, oc, ce)
}

/// Rasterize a glyph run (one or more gids at pen positions) into a grayscale
/// bitmap, mirroring `cpurender/raster.rs::render_text` EXACTLY: white paper,
/// black ink, grayscale AA (`NonZero`), no gamma. Returns the bitmap, the
/// per-glyph advances (px, pre-rounding), and whether EVERY glyph was actually
/// hinted (false => hinting silently fell back to unhinted for some glyph).
///
/// Pen model: each glyph's origin is snapped to the integer pixel grid
/// (`pen.round()`), matching how a hinting renderer — and CoreText with
/// subpixel positioning OFF — grid-fits each glyph origin.
#[allow(clippy::too_many_arguments)]
fn render_ours(
    font: &ParsedFont,
    cache: &mut GlyphCache,
    gids: &[u16],
    ppem: u16,
    upem: u16,
    want_hinted: bool,
    w: u32,
    h: u32,
    baseline: f32,
    origin_x: f32,
) -> (GrayBitmap, Vec<f32>, bool) {
    let mut buf = vec![255u8; (w * h * 4) as usize]; // white RGBA paper
    let mut advances = Vec::with_capacity(gids.len());
    let mut all_hinted = true;
    let upem_f = f32::from(upem);
    {
        let stride = (w * 4) as i32;
        let mut ra = unsafe { RowAccessor::new_with_buf(buf.as_mut_ptr(), w, h, stride) };
        let pf = PixfmtRgba32::new(&mut ra);
        let mut rb = RendererBase::new(pf);
        let mut ras = RasterizerScanlineAa::new();
        ras.filling_rule(FillingRule::NonZero); // matches raster.rs render_text
        let mut sl = ScanlineU8::new();
        let black = Rgba8::new(0, 0, 0, 255);
        let ty = f64::from(baseline);
        let mut pen = origin_x;

        for &gid in gids {
            let adv_px = f32::from(font.get_horizontal_advance(gid)) / upem_f * ppem as f32;
            let tx = f64::from(pen.round()); // integer-origin grid-fit
            let t = TransAffine::new_translation(tx, ty);

            if let Some(owned) = font.get_or_decode_glyph(gid) {
                let mut drew = false;
                if want_hinted {
                    // Product path: GlyphCache::get_or_build -> build_hinted_path.
                    if let Some(c) = cache.get_or_build(0, gid, &*owned, font, ppem) {
                        if c.is_hinted {
                            ras.reset();
                            ras.add_path_vertices_transformed(c.path.vertices(), &t);
                            render_scanlines_aa_solid(&mut ras, &mut sl, &mut rb, &black);
                            drew = true;
                        }
                    }
                    if !drew {
                        // Hinting did not run for this glyph — flag it, draw unhinted
                        // so the panel still shows something.
                        all_hinted = false;
                        if let Some(p) = unhinted_pixel_path(&owned, ppem, upem) {
                            ras.reset();
                            ras.add_path_vertices_transformed(p.vertices(), &t);
                            render_scanlines_aa_solid(&mut ras, &mut sl, &mut rb, &black);
                        }
                    }
                } else if let Some(p) = unhinted_pixel_path(&owned, ppem, upem) {
                    ras.reset();
                    ras.add_path_vertices_transformed(p.vertices(), &t);
                    render_scanlines_aa_solid(&mut ras, &mut sl, &mut rb, &black);
                }
            }
            advances.push(adv_px);
            pen += adv_px;
        }
    }
    (rgba_to_gray(&buf, w, h), advances, all_hinted)
}

fn rgba_to_gray(buf: &[u8], w: u32, h: u32) -> GrayBitmap {
    let mut gray = vec![0u8; (w * h) as usize];
    for i in 0..(w * h) as usize {
        let r = u32::from(buf[i * 4]);
        let g = u32::from(buf[i * 4 + 1]);
        let b = u32::from(buf[i * 4 + 2]);
        gray[i] = ((r * 299 + g * 587 + b * 114) / 1000) as u8;
    }
    GrayBitmap { data: gray, w, h }
}

// ── CORETEXT rendering ──────────────────────────────────────────────

/// Grayscale CGBitmapContext with every rendering-parity flag pinned. Each flag
/// is annotated with the CoreText default it deliberately overrides so the two
/// engines are comparable.
fn make_gray_context(w: u32, h: u32) -> CGContext {
    let cs = CGColorSpace::create_device_gray();
    // 8bpc, bytesPerRow = w (tightly packed) => data() is exactly w*h bytes.
    let ctx = CGContext::create_bitmap_context(None, w as usize, h as usize, 8, w as usize, &cs, 0);

    // Paper = white; ink = black — same as our agg white-bg / black-ink render.
    ctx.set_rgb_fill_color(1.0, 1.0, 1.0, 1.0);
    ctx.fill_rect(CGRect::new(
        &CGPoint::new(0.0, 0.0),
        &CGSize::new(f64::from(w), f64::from(h)),
    ));
    ctx.set_rgb_fill_color(0.0, 0.0, 0.0, 1.0);

    // Font smoothing OFF (CoreText default: ON). ON dilates stems ("stem
    // darkening") and, on LCD, does subpixel — neither of which our agg path
    // does. OFF gives plain grayscale coverage comparable to ours. Set BOTH
    // allows_* and should_* — some macOS builds honor only one.
    ctx.set_allows_font_smoothing(false);
    ctx.set_should_smooth_fonts(false);

    // Antialiasing ON (grayscale) — matches agg `RasterizerScanlineAa`'s AA.
    ctx.set_allows_antialiasing(true);
    ctx.set_should_antialias(true);

    // Subpixel positioning/quantization OFF (CoreText default: ON). We hand
    // CoreText integer glyph origins and our hinted render is integer-origin;
    // OFF makes CoreText grid-fit each glyph origin to the pixel to match.
    ctx.set_should_subpixel_quantize_fonts(false);
    ctx.set_should_subpixel_position_fonts(false);
    ctx
}

/// Single glyph via `CTFontDrawGlyphs` at an integer origin (matches OURS char
/// path). `baseline_bottom` is measured from the bitmap bottom (CG y-up).
fn render_ct_char(
    ctfont: &CTFont,
    ch: char,
    w: u32,
    h: u32,
    baseline_bottom: f64,
    origin_x: f64,
) -> Option<GrayBitmap> {
    let chars = [ch as u16];
    let mut glyphs = [0u16; 1];
    unsafe {
        ctfont.get_glyphs_for_characters(chars.as_ptr(), glyphs.as_mut_ptr(), 1);
    }
    if glyphs[0] == 0 {
        return None; // not in cmap
    }
    let mut ctx = make_gray_context(w, h);
    // draw_glyphs takes the context by value (retain); the bitmap is shared.
    ctfont.draw_glyphs(
        &[glyphs[0]],
        &[CGPoint::new(origin_x, baseline_bottom)],
        ctx.clone(),
    );
    let data = ctx.data().to_vec();
    Some(GrayBitmap { data, w, h })
}

/// A word via `CTLine`/`CTLineDraw` so CoreText owns positioning (advances +
/// kerning). Returns the bitmap and CoreText's per-glyph advances (px) derived
/// from run positions, plus the CoreText glyph count (may differ from our
/// per-char count if CoreText forms ligatures).
fn render_ct_word(
    ctfont: &CTFont,
    word: &str,
    w: u32,
    h: u32,
    baseline_bottom: f64,
    origin_x: f64,
) -> Option<(GrayBitmap, Vec<f32>, usize)> {
    // Build the attributed string with OUR exact CTFont (identical outlines,
    // since both sides came from the same file bytes / face 0).
    let mut s = CFMutableAttributedString::new();
    s.replace_str(&CFString::new(word), CFRange::init(0, 0));
    let len = s.char_len();
    unsafe {
        s.set_attribute(CFRange::init(0, len), kCTFontAttributeName, ctfont);
    }
    let line = CTLine::new_with_attributed_string(s.as_concrete_TypeRef());

    let mut ctx = make_gray_context(w, h);
    // Lay out from the same integer pen origin OURS uses.
    ctx.set_text_position(origin_x, baseline_bottom);
    line.draw(&ctx);
    let data = ctx.data().to_vec();

    // Per-glyph advances from run positions (LTR, relative to line origin).
    let mut xs: Vec<f64> = Vec::new();
    let runs = line.glyph_runs();
    for run in runs.iter() {
        let pos = run.positions();
        for p in pos.iter() {
            xs.push(p.x);
        }
    }
    let width = line.get_typographic_bounds().width;
    let mut advances = Vec::with_capacity(xs.len());
    for i in 0..xs.len() {
        let nx = if i + 1 < xs.len() { xs[i + 1] } else { width };
        advances.push((nx - xs[i]) as f32);
    }
    let count = xs.len();
    Some((GrayBitmap { data, w, h }, advances, count))
}

// ── Metrics ─────────────────────────────────────────────────────────

/// Union of the provided ink bboxes, padded and clamped; returned as an
/// EXCLUSIVE region `(x0,y0,x1,y1)`. Falls back to a small top-left region
/// when nothing is inked.
fn union_region(bmps: &[&GrayBitmap], pad: u32) -> (u32, u32, u32, u32) {
    let (w, h) = (bmps[0].w, bmps[0].h);
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0u32, 0u32);
    let mut any = false;
    for b in bmps {
        if let Some((a, c, e, f)) = b.ink_bbox() {
            x0 = x0.min(a);
            y0 = y0.min(c);
            x1 = x1.max(e);
            y1 = y1.max(f);
            any = true;
        }
    }
    if !any {
        return (0, 0, w.min(24), h.min(24));
    }
    (
        x0.saturating_sub(pad),
        y0.saturating_sub(pad),
        (x1 + pad + 1).min(w),
        (y1 + pad + 1).min(h),
    )
}

/// Coverage RMS over an exclusive region, glyphs at the SAME coords.
fn rms_region(a: &GrayBitmap, b: &GrayBitmap, region: (u32, u32, u32, u32)) -> f64 {
    let (x0, y0, x1, y1) = region;
    let mut sumsq = 0.0f64;
    let mut n = 0u64;
    for y in y0..y1 {
        for x in x0..x1 {
            let d = f64::from(a.at(x, y)) - f64::from(b.at(x, y));
            sumsq += d * d;
            n += 1;
        }
    }
    if n == 0 {
        0.0
    } else {
        (sumsq / n as f64).sqrt()
    }
}

/// Coverage RMS after aligning the two ink-bbox top-left origins (pure shape).
fn rms_ink_aligned(a: &GrayBitmap, b: &GrayBitmap) -> f64 {
    let (ab, bb) = match (a.ink_bbox(), b.ink_bbox()) {
        (Some(x), Some(y)) => (x, y),
        _ => return 0.0,
    };
    let (aw, ah) = (ab.2 - ab.0 + 1, ab.3 - ab.1 + 1);
    let (bw, bh) = (bb.2 - bb.0 + 1, bb.3 - bb.1 + 1);
    let (mw, mh) = (aw.min(bw), ah.min(bh));
    let mut sumsq = 0.0f64;
    let mut n = 0u64;
    for j in 0..mh {
        for i in 0..mw {
            let da = f64::from(a.at(ab.0 + i, ab.1 + j));
            let db = f64::from(b.at(bb.0 + i, bb.1 + j));
            let d = da - db;
            sumsq += d * d;
            n += 1;
        }
    }
    if n == 0 {
        0.0
    } else {
        (sumsq / n as f64).sqrt()
    }
}

/// Max per-column and per-row coverage-sum difference (in fully-covered-pixel
/// units) over an exclusive region. A stem 1px off spikes one column's sum =>
/// a strong stem-placement signal that RMS alone smears out.
fn colrow_diff(a: &GrayBitmap, b: &GrayBitmap, region: (u32, u32, u32, u32)) -> (f64, f64) {
    let (x0, y0, x1, y1) = region;
    let mut max_col = 0i64;
    for x in x0..x1 {
        let (mut sa, mut sb) = (0i64, 0i64);
        for y in y0..y1 {
            sa += 255 - i64::from(a.at(x, y));
            sb += 255 - i64::from(b.at(x, y));
        }
        max_col = max_col.max((sa - sb).abs());
    }
    let mut max_row = 0i64;
    for y in y0..y1 {
        let (mut sa, mut sb) = (0i64, 0i64);
        for x in x0..x1 {
            sa += 255 - i64::from(a.at(x, y));
            sb += 255 - i64::from(b.at(x, y));
        }
        max_row = max_row.max((sa - sb).abs());
    }
    (max_col as f64 / 255.0, max_row as f64 / 255.0)
}

fn bucket(rms: f64) -> &'static str {
    if rms < 2.0 {
        "MATCH"
    } else if rms < 8.0 {
        "CLOSE"
    } else {
        "DIVERGENT"
    }
}

// ── PNG panels (16x nearest-neighbor + grid) ────────────────────────

const UPSCALE: u32 = 16;
const SEP: u32 = 2;
const HDR: u32 = 6;
const GRID_C: (u8, u8, u8) = (70, 70, 80);
const SEP_C: (u8, u8, u8) = (18, 18, 26);
const H_HINTED: (u8, u8, u8) = (40, 180, 70); // green
const H_CT: (u8, u8, u8) = (50, 120, 220); // blue
const H_DIFF: (u8, u8, u8) = (200, 60, 160); // magenta
const H_UNH: (u8, u8, u8) = (150, 150, 150); // gray

#[inline]
fn put(img: &mut [u8], stride: u32, x: u32, y: u32, c: (u8, u8, u8)) {
    let i = ((y * stride + x) * 3) as usize;
    img[i] = c.0;
    img[i + 1] = c.1;
    img[i + 2] = c.2;
}

/// Crop `b` to `region`, emit an RGB (v,v,v) source panel.
fn gray_source(b: &GrayBitmap, region: (u32, u32, u32, u32)) -> Vec<u8> {
    let (x0, y0, x1, y1) = region;
    let (cw, ch) = (x1 - x0, y1 - y0);
    let mut v = Vec::with_capacity((cw * ch * 3) as usize);
    for cy in 0..ch {
        for cx in 0..cw {
            let g = if x0 + cx < b.w && y0 + cy < b.h {
                b.at(x0 + cx, y0 + cy)
            } else {
                255
            };
            v.extend_from_slice(&[g, g, g]);
        }
    }
    v
}

/// Diff heatmap source: gray where equal, red ∝ (ours-ct) over-ink, blue ∝
/// (ct-ours) under-ink.
fn diff_source(hinted: &GrayBitmap, ct: &GrayBitmap, region: (u32, u32, u32, u32)) -> Vec<u8> {
    let (x0, y0, x1, y1) = region;
    let (cw, ch) = (x1 - x0, y1 - y0);
    let mut v = Vec::with_capacity((cw * ch * 3) as usize);
    for cy in 0..ch {
        for cx in 0..cw {
            let (x, y) = (x0 + cx, y0 + cy);
            let gh = if x < hinted.w && y < hinted.h { hinted.at(x, y) } else { 255 };
            let gc = if x < ct.w && y < ct.h { ct.at(x, y) } else { 255 };
            let cov_h = 255i32 - i32::from(gh);
            let cov_c = 255i32 - i32::from(gc);
            let d = cov_h - cov_c;
            let px = if d == 0 {
                let g = (255 - cov_h) as u8; // grayscale where they agree
                (g, g, g)
            } else if d > 0 {
                let dd = d.min(255) as u8; // ours has MORE ink -> red
                (255, 255u8.saturating_sub(dd), 255u8.saturating_sub(dd))
            } else {
                let dd = (-d).min(255) as u8; // ct has MORE ink -> blue
                (255u8.saturating_sub(dd), 255u8.saturating_sub(dd), 255)
            };
            v.extend_from_slice(&[px.0, px.1, px.2]);
        }
    }
    v
}

fn write_png_rgb(path: &Path, w: u32, h: u32, rgb: &[u8]) {
    let file = match File::create(path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("[coretext_autoregression] cannot create {path:?}: {e}");
            return;
        }
    };
    let mut enc = png::Encoder::new(BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    match enc.write_header() {
        Ok(mut wr) => {
            let _ = wr.write_image_data(rgb);
        }
        Err(e) => eprintln!("[coretext_autoregression] png header {path:?}: {e}"),
    }
}

/// Compose the 4-panel PNG (each cropped to a shared `region`, upscaled 16x
/// nearest-neighbor with a 1px grid per source pixel, 2px separators, and a
/// colored header bar per panel for quick LLM identification).
fn write_panels(
    path: &Path,
    hinted: &GrayBitmap,
    ct: &GrayBitmap,
    unhinted: &GrayBitmap,
    region: (u32, u32, u32, u32),
) {
    let (x0, y0, x1, y1) = region;
    let (cw, ch) = (x1 - x0, y1 - y0);
    if cw == 0 || ch == 0 {
        return;
    }
    let panels: [((u8, u8, u8), Vec<u8>); 4] = [
        (H_HINTED, gray_source(hinted, region)),
        (H_CT, gray_source(ct, region)),
        (H_DIFF, diff_source(hinted, ct, region)),
        (H_UNH, gray_source(unhinted, region)),
    ];

    let panel_w = cw * UPSCALE;
    let panel_h = ch * UPSCALE;
    let total_w = panel_w * 4 + SEP * 3;
    let total_h = panel_h + HDR;
    let mut img = vec![0u8; (total_w * total_h * 3) as usize];
    for p in img.chunks_mut(3) {
        p[0] = SEP_C.0;
        p[1] = SEP_C.1;
        p[2] = SEP_C.2;
    }

    for (k, (hdr, src)) in panels.iter().enumerate() {
        let xo = k as u32 * (panel_w + SEP);
        for y in 0..HDR {
            for x in 0..panel_w {
                put(&mut img, total_w, xo + x, y, *hdr);
            }
        }
        for oy in 0..panel_h {
            for ox in 0..panel_w {
                let sx = ox / UPSCALE;
                let sy = oy / UPSCALE;
                let si = ((sy * cw + sx) * 3) as usize;
                let mut c = (src[si], src[si + 1], src[si + 2]);
                if ox % UPSCALE == UPSCALE - 1 || oy % UPSCALE == UPSCALE - 1 {
                    c = GRID_C;
                }
                put(&mut img, total_w, xo + ox, HDR + oy, c);
            }
        }
    }
    write_png_rgb(path, total_w, total_h, &img);
}

// ── JSON (hand-rolled: serde_json is NOT enabled by coretext_tests) ─

fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            _ => o.push(c),
        }
    }
    o
}

fn bb_json(b: Option<(u32, u32, u32, u32)>) -> String {
    match b {
        Some((a, c, e, f)) => format!("[{a},{c},{e},{f}]"),
        None => "null".to_string(),
    }
}

// ── One case's result (for SUMMARY ranking) ─────────────────────────

struct CaseResult {
    ppem: u16,
    id: String,
    text: String,
    kind: &'static str,
    rms_raw: f64,
    rms_aligned: f64,
    bbox_delta: (i32, i32, i32, i32),
    bucket: &'static str,
    file: String,
    all_hinted: bool,
}

// ── Config ──────────────────────────────────────────────────────────

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn resolve_font() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("AZ_CT_FONT") {
        return Some(PathBuf::from(p));
    }
    // First existing of the standard supplemental faces (all single-face .ttf,
    // so ParsedFont face 0 == CGFont face 0 => identical outlines both sides).
    const CANDS: [&str; 3] = [
        "/System/Library/Fonts/Supplemental/Arial.ttf",
        "/System/Library/Fonts/Supplemental/Times New Roman.ttf",
        "/System/Library/Fonts/Supplemental/Verdana.ttf",
    ];
    CANDS.iter().find(|p| Path::new(p).exists()).map(PathBuf::from)
}

// ── The test ────────────────────────────────────────────────────────

#[test]
fn coretext_autoregression() {
    let font_path = match resolve_font() {
        Some(p) => p,
        None => {
            eprintln!(
                "[coretext_autoregression] SKIP: no font found — set AZ_CT_FONT to a .ttf path"
            );
            return;
        }
    };
    let bytes = match fs::read(&font_path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[coretext_autoregression] SKIP: cannot read {font_path:?}: {e}");
            return;
        }
    };

    // OURS: parse + prime (tests read raw outlines by gid).
    let mut warnings = Vec::new();
    let mut font = match ParsedFont::from_bytes(&bytes, 0, &mut warnings) {
        Some(f) => f,
        None => {
            eprintln!("[coretext_autoregression] SKIP: ParsedFont::from_bytes failed");
            return;
        }
    };
    font.prime_glyph_cache();
    let upem = font.font_metrics.units_per_em;
    if upem == 0 {
        eprintln!("[coretext_autoregression] SKIP: units_per_em == 0");
        return;
    }
    let upem_f = f32::from(upem);
    let ascent = font.font_metrics.ascent; // already f32

    // CORETEXT: build a CGFont from the SAME bytes (face 0) so glyph outlines
    // are byte-identical — avoids CoreText resolving a *different* file by name.
    let provider = CGDataProvider::from_buffer(Arc::new(bytes.clone()));
    let cgfont = match CGFont::from_data_provider(provider) {
        Ok(f) => f,
        Err(()) => {
            eprintln!("[coretext_autoregression] SKIP: CGFont::from_data_provider failed");
            return;
        }
    };

    // Config
    let ppems: Vec<u16> = env_or("AZ_CT_PPEMS", "9,10,11,12,13,14,16,18,24")
        .split(',')
        .filter_map(|s| s.trim().parse::<u16>().ok())
        .filter(|&p| p > 0)
        .collect();
    let chars: Vec<char> = env_or("AZ_CT_CHARS", "AaBbEeGgHhIlMmOoRrSsTtWwXx0123468")
        .chars()
        .collect();
    let words: Vec<String> = env_or("AZ_CT_WORDS", "hamburgevons,Illinois,minimum,WAVE")
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    let out_dir = PathBuf::from(env_or("AZ_CT_OUT", "target/coretext_autoregression"));
    let max_rms: Option<f64> = std::env::var("AZ_CT_MAX_RMS")
        .ok()
        .and_then(|s| s.trim().parse::<f64>().ok());

    if let Err(e) = fs::create_dir_all(&out_dir) {
        eprintln!("[coretext_autoregression] SKIP: cannot create {out_dir:?}: {e}");
        return;
    }

    eprintln!(
        "[coretext_autoregression] font={font_path:?} ppems={ppems:?} chars={} words={words:?} out={out_dir:?}",
        chars.len()
    );

    let origin_x = 1.0f32; // 1px left margin so ink is not clipped at x=0
    let mut cache = GlyphCache::new();
    let mut jsonl = String::new();
    let mut results: Vec<CaseResult> = Vec::new();

    // Deterministic order: ppem asc (as given), then chars (as given), then words.
    for &ppem in &ppems {
        // CTFont sized in points == pixels at the context's 72dpi default.
        let ctfont = ct_font::new_from_CGFont(&cgfont, f64::from(ppem));

        // Baseline = ascent px from the top; ceil for headroom. Integer so the
        // glyph origin lands on the pixel grid (hinting assumes integer origin).
        let baseline = (ascent / upem_f * ppem as f32).ceil();
        let h = (ppem as f32 * 2.5).ceil() as u32;
        // CoreGraphics is y-up (origin bottom-left) but its bitmap memory is
        // top-down; the two cancel, so a baseline `baseline` px from the top ==
        // `h - baseline` px from the bottom in CG user space.
        let ct_baseline = f64::from(h) - f64::from(baseline);

        for &c in &chars {
            let Some(gid) = font.lookup_glyph_index(c as u32) else {
                continue;
            };
            let w = (ppem as f32 * 3.0).ceil() as u32;

            let ct = match render_ct_char(&ctfont, c, w, h, ct_baseline, f64::from(origin_x)) {
                Some(b) => b,
                None => {
                    eprintln!("[coretext_autoregression] ct miss '{c}' @ {ppem}px");
                    continue;
                }
            };
            let (hinted, _adv, all_hinted) =
                render_ours(&font, &mut cache, &[gid], ppem, upem, true, w, h, baseline, origin_x);
            let (unhinted, _, _) =
                render_ours(&font, &mut cache, &[gid], ppem, upem, false, w, h, baseline, origin_x);

            let id = format!("char-{:04x}", c as u32); // hex codepoint: case-safe on APFS
            emit_case(
                &out_dir, &mut jsonl, &mut results, ppem, &id, &c.to_string(), "char",
                &hinted, &ct, &unhinted, all_hinted, &[], &[], 1, 1,
            );
        }

        for word in &words {
            let gids: Vec<u16> = word
                .chars()
                .filter_map(|c| font.lookup_glyph_index(c as u32))
                .collect();
            if gids.is_empty() {
                continue;
            }
            // Width: fit the wider of our pen extent and CoreText's line width.
            let our_total: f32 = gids
                .iter()
                .map(|&g| f32::from(font.get_horizontal_advance(g)) / upem_f * ppem as f32)
                .sum();
            let w = (origin_x + our_total + ppem as f32 * 1.5).ceil() as u32;

            let (ct, ct_adv, ct_glyphs) =
                match render_ct_word(&ctfont, word, w, h, ct_baseline, f64::from(origin_x)) {
                    Some(t) => t,
                    None => {
                        eprintln!("[coretext_autoregression] ct word miss '{word}' @ {ppem}px");
                        continue;
                    }
                };
            let (hinted, our_adv, all_hinted) =
                render_ours(&font, &mut cache, &gids, ppem, upem, true, w, h, baseline, origin_x);
            let (unhinted, _, _) =
                render_ours(&font, &mut cache, &gids, ppem, upem, false, w, h, baseline, origin_x);

            let id = format!("word-{}", sanitize(word));
            emit_case(
                &out_dir, &mut jsonl, &mut results, ppem, &id, word, "word",
                &hinted, &ct, &unhinted, all_hinted, &our_adv, &ct_adv, gids.len(), ct_glyphs,
            );
        }
    }

    // metrics.jsonl (deterministic order = emission order).
    let jsonl_path = out_dir.join("metrics.jsonl");
    if let Err(e) = fs::write(&jsonl_path, &jsonl) {
        eprintln!("[coretext_autoregression] cannot write {jsonl_path:?}: {e}");
    }

    // SUMMARY.md + stdout top-10.
    write_summary(&out_dir, &font_path, &ppems, &results);

    // Optional regression gate.
    if let Some(max) = max_rms {
        let offenders: Vec<&CaseResult> =
            results.iter().filter(|r| r.rms_raw > max).collect();
        if !offenders.is_empty() {
            let mut msg = format!(
                "AZ_CT_MAX_RMS={max:.2} exceeded by {} case(s):\n",
                offenders.len()
            );
            let mut sorted = offenders.clone();
            sorted.sort_by(|a, b| b.rms_raw.partial_cmp(&a.rms_raw).unwrap());
            for r in sorted.iter().take(20) {
                msg.push_str(&format!(
                    "  {:>10} @ {:>2}px  rms_raw={:.2}  ({})\n",
                    r.id, r.ppem, r.rms_raw, r.file
                ));
            }
            panic!("{msg}");
        }
    }
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

/// Compute metrics for one case, write its PNG, append its JSON line, and record
/// a `CaseResult` for the summary ranking.
#[allow(clippy::too_many_arguments)]
fn emit_case(
    out_dir: &Path,
    jsonl: &mut String,
    results: &mut Vec<CaseResult>,
    ppem: u16,
    id: &str,
    text: &str,
    kind: &'static str,
    hinted: &GrayBitmap,
    ct: &GrayBitmap,
    unhinted: &GrayBitmap,
    all_hinted: bool,
    our_adv: &[f32],
    ct_adv: &[f32],
    our_glyphs: usize,
    ct_glyphs: usize,
) {
    let ob = hinted.ink_bbox();
    let cb = ct.ink_bbox();
    let ub = unhinted.ink_bbox();

    let raw_region = union_region(&[hinted, ct], 1);
    let rms_raw = rms_region(hinted, ct, raw_region);
    let rms_aligned = rms_ink_aligned(hinted, ct);
    let (max_col, max_row) = colrow_diff(hinted, ct, raw_region);

    let bbox_delta = match (ob, cb) {
        (Some(o), Some(c)) => (
            o.0 as i32 - c.0 as i32,
            o.1 as i32 - c.1 as i32,
            (o.2 - o.0) as i32 - (c.2 - c.0) as i32,
            (o.3 - o.1) as i32 - (c.3 - c.1) as i32,
        ),
        _ => (0, 0, 0, 0),
    };
    let bkt = bucket(rms_raw);

    let file = format!("{ppem:02}px_{id}_{bkt}_rms{rms_raw:.2}.png");
    let panel_region = union_region(&[hinted, ct, unhinted], 2);
    write_panels(&out_dir.join(&file), hinted, ct, unhinted, panel_region);

    // advances JSON: [[our,ct,delta],...] paired up to the shorter run.
    let mut adv_json = String::from("[");
    let n = our_adv.len().min(ct_adv.len());
    for i in 0..n {
        if i > 0 {
            adv_json.push(',');
        }
        adv_json.push_str(&format!(
            "[{:.3},{:.3},{:.3}]",
            our_adv[i],
            ct_adv[i],
            our_adv[i] - ct_adv[i]
        ));
    }
    adv_json.push(']');

    jsonl.push_str(&format!(
        "{{\"ppem\":{ppem},\"case\":\"{id}\",\"kind\":\"{kind}\",\"text\":\"{text}\",\
\"bucket\":\"{bkt}\",\"rms_raw\":{rms_raw:.4},\"rms_aligned\":{rms_aligned:.4},\
\"ours_bbox\":{ob_j},\"ct_bbox\":{cb_j},\"unhinted_bbox\":{ub_j},\
\"ours_px\":{ours_px},\"ct_px\":{ct_px},\"unhinted_px\":{unh_px},\
\"bbox_delta\":[{bd0},{bd1},{bd2},{bd3}],\
\"max_col_diff\":{max_col:.3},\"max_row_diff\":{max_row:.3},\
\"all_hinted\":{all_hinted},\"our_glyphs\":{our_glyphs},\"ct_glyphs\":{ct_glyphs},\
\"advances\":{adv_json},\"file\":\"{file}\"}}\n",
        text = esc(text),
        ob_j = bb_json(ob),
        cb_j = bb_json(cb),
        ub_j = bb_json(ub),
        ours_px = hinted.ink_pixels(),
        ct_px = ct.ink_pixels(),
        unh_px = unhinted.ink_pixels(),
        bd0 = bbox_delta.0,
        bd1 = bbox_delta.1,
        bd2 = bbox_delta.2,
        bd3 = bbox_delta.3,
    ));

    results.push(CaseResult {
        ppem,
        id: id.to_string(),
        text: text.to_string(),
        kind,
        rms_raw,
        rms_aligned,
        bbox_delta,
        bucket: bkt,
        file,
        all_hinted,
    });
}

fn write_summary(out_dir: &Path, font_path: &Path, ppems: &[u16], results: &[CaseResult]) {
    // Sort worst-first: rms_raw desc, then ppem asc, then id asc (stable/total).
    let mut sorted: Vec<&CaseResult> = results.iter().collect();
    sorted.sort_by(|a, b| {
        b.rms_raw
            .partial_cmp(&a.rms_raw)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.ppem.cmp(&b.ppem))
            .then(a.id.cmp(&b.id))
    });

    let mut md = String::new();
    md.push_str("# CoreText autoregression — worst-first\n\n");
    md.push_str(&format!("- font: `{}`\n", font_path.display()));
    md.push_str(&format!("- cases: {}\n", results.len()));
    let (mut nm, mut nc, mut nd) = (0u32, 0u32, 0u32);
    for r in results {
        match r.bucket {
            "MATCH" => nm += 1,
            "CLOSE" => nc += 1,
            _ => nd += 1,
        }
    }
    md.push_str(&format!(
        "- buckets: MATCH {nm} / CLOSE {nc} / DIVERGENT {nd}\n"
    ));
    let not_hinted: Vec<&CaseResult> = results.iter().filter(|r| !r.all_hinted).collect();
    if !not_hinted.is_empty() {
        md.push_str(&format!(
            "- **WARNING: {} case(s) fell back to UNHINTED** (hinting not running — see divergence map)\n",
            not_hinted.len()
        ));
    }
    md.push_str("\nPanels per PNG: `[ours-hinted (green) | coretext (blue) | diff-heatmap (magenta) | ours-unhinted (gray)]`. Diff: red=over-ink (ours>ct), blue=under-ink (ct>ours).\n");

    md.push_str("\n## Ranked (rms_raw desc)\n\n");
    md.push_str("| case | ppem | rms_raw | rms_aln | bboxΔ(dx0,dy0,dw,dh) | bucket | hinted | file |\n");
    md.push_str("|---|---:|---:|---:|---|---|:---:|---|\n");
    for r in &sorted {
        md.push_str(&format!(
            "| {} | {} | {:.2} | {:.2} | ({},{},{},{}) | {} | {} | {} |\n",
            r.id,
            r.ppem,
            r.rms_raw,
            r.rms_aligned,
            r.bbox_delta.0,
            r.bbox_delta.1,
            r.bbox_delta.2,
            r.bbox_delta.3,
            r.bucket,
            if r.all_hinted { "y" } else { "**N**" },
            r.file,
        ));
    }

    md.push_str("\n## Per-ppem histogram\n\n");
    md.push_str("| ppem | MATCH | CLOSE | DIVERGENT | worst rms |\n|---:|---:|---:|---:|---:|\n");
    for &p in ppems {
        let (mut m, mut c, mut d) = (0u32, 0u32, 0u32);
        let mut worst = 0.0f64;
        for r in results.iter().filter(|r| r.ppem == p) {
            match r.bucket {
                "MATCH" => m += 1,
                "CLOSE" => c += 1,
                _ => d += 1,
            }
            worst = worst.max(r.rms_raw);
        }
        md.push_str(&format!("| {p} | {m} | {c} | {d} | {worst:.2} |\n"));
    }

    md.push_str("\n## Top 10 worst\n\n");
    for (i, r) in sorted.iter().take(10).enumerate() {
        md.push_str(&format!(
            "{}. `{}` @ {}px — rms_raw {:.2} (aln {:.2}) — {} — {}\n",
            i + 1,
            r.text,
            r.ppem,
            r.rms_raw,
            r.rms_aligned,
            r.bucket,
            r.file
        ));
    }

    let summary_path = out_dir.join("SUMMARY.md");
    if let Err(e) = fs::write(&summary_path, &md) {
        eprintln!("[coretext_autoregression] cannot write {summary_path:?}: {e}");
    }

    // Stdout top-10 (visible with --nocapture).
    println!("\n=== coretext_autoregression: top 10 worst (rms_raw) ===");
    println!("out dir: {}", out_dir.display());
    for (i, r) in sorted.iter().take(10).enumerate() {
        println!(
            "{:>2}. {:>12} @ {:>2}px  rms_raw={:>7.2}  rms_aln={:>7.2}  {:<9}  hinted={}  {}",
            i + 1,
            r.text,
            r.ppem,
            r.rms_raw,
            r.rms_aligned,
            r.bucket,
            if r.all_hinted { "y" } else { "N" },
            r.file,
        );
    }
    println!(
        "buckets: MATCH {nm} / CLOSE {nc} / DIVERGENT {nd}   (SUMMARY.md + metrics.jsonl written)"
    );
}

//! Compare allsorts hinted output against FreeType reference values.
//!
//! FreeType values were captured with freetype-py at ppem=16, 72dpi on
//! HelveticaNeue.ttc (index 0) using FT_LOAD_TARGET_MONO (full X+Y hinting).
//!
//! TARGET_MONO is the correct comparison baseline because our interpreter
//! applies full X+Y hinting (unlike FreeType's default v40 subpixel mode
//! which discards X-axis hint movements).

use std::fmt::Write as FmtWrite;
use std::fs;

use azul_layout::font::parsed::ParsedFont;
use azul_layout::glyph_cache::build_path_from_contours;
use allsorts::hinting::f26dot6::{compute_scale, F26Dot6};

fn load_helvetica_neue() -> Option<ParsedFont> {
    let font_path = "/System/Library/Fonts/HelveticaNeue.ttc";
    let font_bytes = fs::read(font_path).ok()?;
    let mut warnings = Vec::new();
    ParsedFont::from_bytes(&font_bytes, 0, &mut warnings)
}

/// FreeType reference data for one glyph at ppem=16.
struct FtRef {
    name: &'static str,
    codepoint: u32,
    glyph_id: u16,
    advance_x: i32,  // F26Dot6
    points: &'static [(i32, i32)],  // F26Dot6 (x, y)
}

// FreeType FT_LOAD_TARGET_MONO reference data at ppem=16, 72dpi, HelveticaNeue.ttc index 0
const FT_T: FtRef = FtRef {
    name: "T",
    codepoint: 0x54,
    glyph_id: 59,
    advance_x: 576,
    points: &[
        (256, 640), (256, 0), (384, 0), (384, 640),
        (628, 640), (628, 704), (12, 704), (12, 640),
    ],
};

const FT_R: FtRef = FtRef {
    name: "R",
    codepoint: 0x52,
    glyph_id: 57,
    advance_x: 704,
    points: &[
        (64, 704), (64, 0), (192, 0), (192, 320),
        (414, 320), (447, 320), (487, 298), (512, 261),
        (524, 212), (529, 185), (534, 157), (536, 101),
        (538, 51), (545, 12), (555, 0), (664, 0),
        (648, 18), (633, 66), (625, 119), (623, 174),
        (621, 201), (618, 228), (606, 276), (583, 316),
        (543, 345), (511, 351), (511, 353), (579, 370),
        (640, 464), (640, 526), (640, 609), (520, 704),
        (414, 704), (375, 384), (192, 384), (192, 640),
        (410, 640), (472, 640), (528, 570), (528, 515),
        (528, 474), (503, 424), (461, 394), (406, 384),
    ],
};

const FT_F: FtRef = FtRef {
    name: "f",
    codepoint: 0x66,
    glyph_id: 77,
    advance_x: 320,
    points: &[
        (128, 448), (128, 0), (192, 0), (192, 448),
        (295, 448), (295, 512), (192, 512), (192, 583),
        (192, 616), (231, 640), (266, 640), (278, 640),
        (308, 636), (320, 632), (320, 697), (308, 701),
        (278, 704), (267, 704), (199, 704), (128, 643),
        (128, 584), (128, 512), (39, 512), (39, 448),
    ],
};

const FT_S: FtRef = FtRef {
    name: "s",
    codepoint: 0x73,
    glyph_id: 90,
    advance_x: 512,
    points: &[
        (128, 192), (64, 192), (66, 139), (99, 65),
        (153, 20), (224, 0), (264, 0), (299, 0),
        (371, 14), (428, 51), (464, 111), (464, 156),
        (464, 192), (435, 240), (388, 272), (328, 291),
        (296, 298), (266, 304), (207, 317), (159, 335),
        (128, 363), (128, 384), (128, 404), (150, 428),
        (186, 442), (229, 448), (250, 448), (274, 448),
        (319, 435), (357, 405), (382, 357), (384, 320),
        (448, 320), (445, 376), (413, 450), (359, 494),
        (289, 512), (247, 512), (215, 512), (149, 496),
        (97, 462), (64, 408), (64, 371), (64, 323),
        (114, 269), (188, 239), (276, 223), (350, 202),
        (400, 170), (400, 138), (400, 115), (375, 85),
        (337, 70), (292, 64), (271, 64), (244, 64),
        (194, 77), (154, 106), (129, 156),
    ],
};

fn hint_glyph(font: &ParsedFont, codepoint: u32, ppem: u16) -> Option<Vec<(i32, i32)>> {
    let glyph_id = font.lookup_glyph_index(codepoint)?;
    let owned = font.glyph_records_decoded.get(&glyph_id)?;
    let raw_points = owned.raw_points.as_ref()?;
    let raw_on_curve = owned.raw_on_curve.as_ref()?;
    let raw_contour_ends = owned.raw_contour_ends.as_ref()?;
    let instructions = owned.instructions.as_deref().unwrap_or(&[]);

    let upem = font.font_metrics.units_per_em;
    let scale = compute_scale(ppem, upem);

    let points_f26dot6: Vec<(i32, i32)> = raw_points.iter().map(|(x, y)| {
        (F26Dot6::from_funits(*x as i32, scale).to_bits(),
         F26Dot6::from_funits(*y as i32, scale).to_bits())
    }).collect();
    let adv_f26dot6 = F26Dot6::from_funits(owned.horz_advance as i32, scale).to_bits();

    let hint_mutex = font.hint_instance.as_ref()?;
    let mut hint = hint_mutex.lock().ok()?;
    hint.set_ppem(ppem, ppem as f64).ok()?;

    hint.hint_glyph_with_orus(
        &points_f26dot6,
        Some(raw_points.as_slice()),
        raw_on_curve,
        raw_contour_ends,
        instructions,
        adv_f26dot6,
    ).ok()
}

fn hint_glyph_with_flags(font: &ParsedFont, codepoint: u32, ppem: u16) -> Option<(Vec<(i32, i32)>, Vec<bool>, Vec<bool>)> {
    let glyph_id = font.lookup_glyph_index(codepoint)?;
    let owned = font.glyph_records_decoded.get(&glyph_id)?;
    let raw_points = owned.raw_points.as_ref()?;
    let raw_on_curve = owned.raw_on_curve.as_ref()?;
    let raw_contour_ends = owned.raw_contour_ends.as_ref()?;
    let instructions = owned.instructions.as_deref().unwrap_or(&[]);

    let upem = font.font_metrics.units_per_em;
    let scale = compute_scale(ppem, upem);

    let points_f26dot6: Vec<(i32, i32)> = raw_points.iter().map(|(x, y)| {
        (F26Dot6::from_funits(*x as i32, scale).to_bits(),
         F26Dot6::from_funits(*y as i32, scale).to_bits())
    }).collect();
    let adv_f26dot6 = F26Dot6::from_funits(owned.horz_advance as i32, scale).to_bits();

    let hint_mutex = font.hint_instance.as_ref()?;
    let mut hint = hint_mutex.lock().ok()?;
    hint.set_ppem(ppem, ppem as f64).ok()?;

    let (coords, post_flags) = hint.hint_glyph_with_flags(
        &points_f26dot6, raw_on_curve, raw_contour_ends, instructions, adv_f26dot6
    ).ok()?;

    Some((coords, raw_on_curve.clone(), post_flags))
}

#[test]
fn test_flag_changes_after_hinting() {
    let font = match load_helvetica_neue() {
        Some(f) => f,
        None => {
            eprintln!("Skipping: HelveticaNeue.ttc not found");
            return;
        }
    };

    let ppem: u16 = 16;
    let glyphs = [('T', 0x54u32), ('R', 0x52), ('f', 0x66), ('s', 0x73)];

    for (name, cp) in &glyphs {
        if let Some((_, pre_flags, post_flags)) = hint_glyph_with_flags(&font, *cp, ppem) {
            let mut changed = Vec::new();
            for (i, (pre, post)) in pre_flags.iter().zip(post_flags.iter()).enumerate() {
                if pre != post {
                    changed.push((i, *pre, *post));
                }
            }
            if changed.is_empty() {
                eprintln!("'{}': no flag changes after hinting", name);
            } else {
                eprintln!("'{}': {} flags changed after hinting!", name, changed.len());
                for (i, pre, post) in &changed {
                    eprintln!("  pt[{}]: {} -> {}", i,
                        if *pre { "ON" } else { "OFF" },
                        if *post { "ON" } else { "OFF" });
                }
            }
        }
    }
}

#[test]
fn test_s_path_segments() {
    let font = match load_helvetica_neue() {
        Some(f) => f,
        None => {
            eprintln!("Skipping: HelveticaNeue.ttc not found");
            return;
        }
    };

    let ppem: u16 = 16;
    let cp = 0x73u32; // 's'

    let glyph_id = font.lookup_glyph_index(cp).unwrap();
    let owned = font.glyph_records_decoded.get(&glyph_id).unwrap();
    let raw_on_curve = owned.raw_on_curve.as_ref().unwrap();
    let raw_contour_ends = owned.raw_contour_ends.as_ref().unwrap();

    eprintln!("=== 's' glyph: {} points, contour_ends={:?} ===", raw_on_curve.len(), raw_contour_ends);
    eprintln!("On-curve flags:");
    for (i, &f) in raw_on_curve.iter().enumerate() {
        eprintln!("  pt[{:2}]: {}", i, if f { "ON " } else { "OFF" });
    }

    // Get hinted points
    let hinted = hint_glyph(&font, cp, ppem).unwrap();
    eprintln!("\nHinted points (F26Dot6 -> px):");
    for (i, &(x, y)) in hinted.iter().enumerate() {
        eprintln!("  pt[{:2}]: ({:6},{:6}) = ({:8.4},{:8.4}) px  {}",
            i, x, y, x as f64 / 64.0, y as f64 / 64.0,
            if raw_on_curve[i] { "ON " } else { "OFF" });
    }

    // Also compare with FreeType reference
    eprintln!("\nFreeType reference:");
    for (i, &(fx, fy)) in FT_S.points.iter().enumerate() {
        let (ax, ay) = hinted[i];
        let dx = ax - fx;
        let dy = ay - fy;
        if dx != 0 || dy != 0 {
            eprintln!("  pt[{:2}]: allsorts=({:6},{:6}) ft=({:6},{:6}) delta=({:+},{:+})",
                i, ax, ay, fx, fy, dx, dy);
        }
    }
}

#[test]
fn test_hint_vs_freetype() {
    let font = match load_helvetica_neue() {
        Some(f) => f,
        None => {
            eprintln!("Skipping: HelveticaNeue.ttc not found");
            return;
        }
    };

    let ppem: u16 = 16;
    let refs = [&FT_T, &FT_R, &FT_F, &FT_S];

    let mut report = String::new();
    writeln!(report, "=== Allsorts vs FreeType hinted comparison (ppem={}) ===\n", ppem).unwrap();

    let mut total_diffs = 0usize;
    let mut total_points = 0usize;
    let mut max_delta = 0i32;

    for ft in &refs {
        let hinted = match hint_glyph(&font, ft.codepoint, ppem) {
            Some(h) => h,
            None => {
                writeln!(report, "=== '{}': HINT FAILED ===\n", ft.name).unwrap();
                continue;
            }
        };

        writeln!(report, "=== Glyph '{}' (gid={}) ===", ft.name, ft.glyph_id).unwrap();

        if hinted.len() != ft.points.len() {
            writeln!(report, "  POINT COUNT MISMATCH: allsorts={} vs freetype={}\n",
                hinted.len(), ft.points.len()).unwrap();
            continue;
        }

        let mut glyph_diffs = 0;
        for (i, ((ax, ay), &(fx, fy))) in hinted.iter().zip(ft.points.iter()).enumerate() {
            let dx = ax - fx;
            let dy = ay - fy;
            total_points += 1;

            if dx != 0 || dy != 0 {
                glyph_diffs += 1;
                total_diffs += 1;
                let max_d = dx.abs().max(dy.abs());
                if max_d > max_delta {
                    max_delta = max_d;
                }
                writeln!(report,
                    "  pt[{:2}]: allsorts=({:6},{:6}) freetype=({:6},{:6}) delta=({:+4},{:+4}) = ({:+.4},{:+.4}) px",
                    i, ax, ay, fx, fy, dx, dy, dx as f64 / 64.0, dy as f64 / 64.0
                ).unwrap();
            }
        }

        if glyph_diffs == 0 {
            writeln!(report, "  PERFECT MATCH ({} points)", ft.points.len()).unwrap();
        } else {
            writeln!(report, "  {}/{} points differ", glyph_diffs, ft.points.len()).unwrap();
        }
        writeln!(report).unwrap();
    }

    writeln!(report, "=== Summary ===").unwrap();
    writeln!(report, "Total points compared: {}", total_points).unwrap();
    writeln!(report, "Points with differences: {}", total_diffs).unwrap();
    writeln!(report, "Max delta: {} F26Dot6 = {:.4} px", max_delta, max_delta as f64 / 64.0).unwrap();

    fs::write("/tmp/hint_comparison_allsorts_vs_freetype.txt", &report)
        .expect("failed to write comparison");
    eprintln!("Wrote /tmp/hint_comparison_allsorts_vs_freetype.txt");
    eprintln!("{}", report);

    // Fail the test if any points differ by more than 1 pixel (64 F26Dot6 units)
    // This is a generous threshold — ideally it should be 0
    assert!(max_delta <= 64,
        "Max hinting delta vs FreeType: {} F26Dot6 ({:.2} px) exceeds 1.0 px threshold",
        max_delta, max_delta as f64 / 64.0);
}

/// Render hinted glyphs (T, R, f, s) to a PNG for visual inspection.
/// Also renders FreeType reference points as a separate image for comparison.
#[test]
fn test_render_hinted_glyphs_png() {
    use tiny_skia::{Pixmap, Paint, FillRule, Transform, Color, PathBuilder};

    let font = match load_helvetica_neue() {
        Some(f) => f,
        None => {
            eprintln!("Skipping: HelveticaNeue.ttc not found");
            return;
        }
    };

    let ppem: u16 = 16;
    let scale_up = 8; // scale up 8x for visibility (each pixel = 8x8 block)
    let glyph_width = 12; // max glyph width in pixels at ppem=16
    let glyph_height = 14; // max glyph height in pixels at ppem=16
    let margin = 2;

    let glyphs_to_render = [
        ("T", 0x54u32),
        ("R", 0x52),
        ("f", 0x66),
        ("s", 0x73),
    ];

    let img_w = (glyphs_to_render.len() as u32) * (glyph_width + margin) as u32 * scale_up as u32;
    let img_h = (glyph_height + margin) as u32 * scale_up as u32;

    // Render allsorts hinted
    let mut pixmap = Pixmap::new(img_w, img_h).unwrap();
    pixmap.fill(Color::from_rgba8(255, 255, 255, 255));

    let mut paint = Paint::default();
    paint.set_color(Color::from_rgba8(0, 0, 0, 255));
    paint.anti_alias = false; // crisp rendering to see pixel grid

    for (gi, (name, cp)) in glyphs_to_render.iter().enumerate() {
        let glyph_id = match font.lookup_glyph_index(*cp) {
            Some(id) => id,
            None => continue,
        };
        let owned = font.glyph_records_decoded.get(&glyph_id).unwrap();
        let raw_on_curve = owned.raw_on_curve.as_ref().unwrap();
        let raw_contour_ends = owned.raw_contour_ends.as_ref().unwrap();

        let hinted = match hint_glyph(&font, *cp, ppem) {
            Some(h) => h,
            None => continue,
        };

        let path = match build_path_from_contours(&hinted, raw_on_curve, raw_contour_ends) {
            Some(p) => p,
            None => continue,
        };

        let x_off = (gi as f32) * (glyph_width + margin) as f32 * scale_up as f32;
        let y_off = (glyph_height as f32 - 2.0) * scale_up as f32; // baseline

        let transform = Transform::from_scale(scale_up as f32, scale_up as f32)
            .post_translate(x_off, y_off);

        pixmap.fill_path(&path, &paint, FillRule::Winding, transform, None);
    }

    let allsorts_path = "/tmp/hinted_glyphs_allsorts.png";
    pixmap.save_png(allsorts_path).unwrap();
    eprintln!("Wrote {}", allsorts_path);

    // Also render FreeType reference points as paths for comparison
    let mut pixmap2 = Pixmap::new(img_w, img_h).unwrap();
    pixmap2.fill(Color::from_rgba8(255, 255, 255, 255));

    let ft_refs: [(&str, u32, &[(i32, i32)]); 4] = [
        ("T", 0x54, FT_T.points),
        ("R", 0x52, FT_R.points),
        ("f", 0x66, FT_F.points),
        ("s", 0x73, FT_S.points),
    ];

    for (gi, (name, cp, ft_points)) in ft_refs.iter().enumerate() {
        let glyph_id = match font.lookup_glyph_index(*cp) {
            Some(id) => id,
            None => continue,
        };
        let owned = font.glyph_records_decoded.get(&glyph_id).unwrap();
        let raw_on_curve = owned.raw_on_curve.as_ref().unwrap();
        let raw_contour_ends = owned.raw_contour_ends.as_ref().unwrap();

        let path = match build_path_from_contours(ft_points, raw_on_curve, raw_contour_ends) {
            Some(p) => p,
            None => continue,
        };

        let x_off = (gi as f32) * (glyph_width + margin) as f32 * scale_up as f32;
        let y_off = (glyph_height as f32 - 2.0) * scale_up as f32;

        let transform = Transform::from_scale(scale_up as f32, scale_up as f32)
            .post_translate(x_off, y_off);

        pixmap2.fill_path(&path, &paint, FillRule::Winding, transform, None);
    }

    let freetype_path = "/tmp/hinted_glyphs_freetype.png";
    pixmap2.save_png(freetype_path).unwrap();
    eprintln!("Wrote {}", freetype_path);
}

/// Render full alphabet (uppercase + lowercase + digits) to visually check for breakouts.
#[test]
fn test_render_full_alphabet_png() {
    use tiny_skia::{Pixmap, Paint, FillRule, Transform, Color};

    let font = match load_helvetica_neue() {
        Some(f) => f,
        None => {
            eprintln!("Skipping: HelveticaNeue.ttc not found");
            return;
        }
    };

    let ppem: u16 = 16;
    let scale_up: u32 = 6;
    let glyph_w: u32 = 10;
    let glyph_h: u32 = 14;
    let cols: u32 = 26;

    let chars: Vec<char> = ('A'..='Z').chain('a'..='z').chain('0'..='9').collect();
    let rows = (chars.len() as u32 + cols - 1) / cols;

    let img_w = cols * glyph_w * scale_up;
    let img_h = rows * glyph_h * scale_up;

    let mut pixmap = Pixmap::new(img_w, img_h).unwrap();
    pixmap.fill(Color::from_rgba8(255, 255, 255, 255));

    let mut paint = Paint::default();
    paint.set_color(Color::from_rgba8(0, 0, 0, 255));
    paint.anti_alias = true;

    let mut failed = Vec::new();

    for (i, ch) in chars.iter().enumerate() {
        let cp = *ch as u32;
        let col = (i as u32) % cols;
        let row = (i as u32) / cols;

        let glyph_id = match font.lookup_glyph_index(cp) {
            Some(id) => id,
            None => continue,
        };
        let owned = match font.glyph_records_decoded.get(&glyph_id) {
            Some(o) => o,
            None => continue,
        };
        let raw_on_curve = match owned.raw_on_curve.as_ref() {
            Some(f) => f,
            None => continue,
        };
        let raw_contour_ends = match owned.raw_contour_ends.as_ref() {
            Some(c) => c,
            None => continue,
        };

        let hinted = match hint_glyph(&font, cp, ppem) {
            Some(h) => h,
            None => {
                failed.push(format!("'{}' hint failed", ch));
                continue;
            }
        };

        let path = match build_path_from_contours(&hinted, raw_on_curve, raw_contour_ends) {
            Some(p) => p,
            None => continue,
        };

        let x_off = (col * glyph_w * scale_up) as f32;
        let y_off = (row * glyph_h * scale_up) as f32 + (glyph_h as f32 - 3.0) * scale_up as f32;

        let transform = Transform::from_scale(scale_up as f32, scale_up as f32)
            .post_translate(x_off, y_off);

        pixmap.fill_path(&path, &paint, FillRule::Winding, transform, None);
    }

    let path = "/tmp/hinted_full_alphabet.png";
    pixmap.save_png(path).unwrap();
    eprintln!("Wrote {}", path);
    if !failed.is_empty() {
        eprintln!("Failed glyphs: {:?}", failed);
    }
}

/// Render a few suspect glyphs at large scale for visual inspection.
#[test]
fn test_render_suspect_glyphs_large() {
    use tiny_skia::{Pixmap, Paint, FillRule, Transform, Color};

    let font = match load_helvetica_neue() {
        Some(f) => f,
        None => { eprintln!("Skipping"); return; }
    };

    let ppem: u16 = 16;
    let scale_up: u32 = 20;
    let glyph_w: u32 = 14;
    let glyph_h: u32 = 14;

    let chars: Vec<(char, u32)> = vec![
        ('H', 0x48), ('I', 0x49), ('M', 0x4D), ('N', 0x4E), ('W', 0x57),
        ('a', 0x61), ('e', 0x65), ('g', 0x67),
        ('6', 0x36), ('8', 0x38), ('9', 0x39),
    ];

    let cols = chars.len() as u32;
    let img_w = cols * glyph_w * scale_up;
    let img_h = glyph_h * scale_up;

    let mut pixmap = Pixmap::new(img_w, img_h).unwrap();
    pixmap.fill(Color::from_rgba8(255, 255, 255, 255));

    let mut paint = Paint::default();
    paint.set_color(Color::from_rgba8(0, 0, 0, 255));
    paint.anti_alias = true;

    for (i, (ch, cp)) in chars.iter().enumerate() {
        let glyph_id = match font.lookup_glyph_index(*cp) {
            Some(id) => id, None => continue,
        };
        let owned = match font.glyph_records_decoded.get(&glyph_id) {
            Some(o) => o, None => continue,
        };
        let raw_on_curve = match owned.raw_on_curve.as_ref() { Some(f) => f, None => continue };
        let raw_contour_ends = match owned.raw_contour_ends.as_ref() { Some(c) => c, None => continue };

        let hinted = match hint_glyph(&font, *cp, ppem) {
            Some(h) => h, None => { eprintln!("'{}' hint failed", ch); continue; }
        };

        let path = match build_path_from_contours(&hinted, raw_on_curve, raw_contour_ends) {
            Some(p) => p, None => continue,
        };

        let x_off = (i as u32 * glyph_w * scale_up) as f32;
        let y_off = (glyph_h as f32 - 3.0) * scale_up as f32;

        let transform = Transform::from_scale(scale_up as f32, scale_up as f32)
            .post_translate(x_off, y_off);

        pixmap.fill_path(&path, &paint, FillRule::Winding, transform, None);
    }

    let path = "/tmp/hinted_suspect_glyphs.png";
    pixmap.save_png(path).unwrap();
    eprintln!("Wrote {}", path);
}

/// Debug specific problematic glyphs by dumping their hinted points and on-curve flags.
#[test]
fn test_debug_h_glyph() {
    let font = match load_helvetica_neue() {
        Some(f) => f,
        None => {
            eprintln!("Skipping: HelveticaNeue.ttc not found");
            return;
        }
    };

    let ppem: u16 = 16;
    // Debug H, M, 6, 8
    let debug_chars = [('H', 0x48u32), ('M', 0x4Du32), ('6', 0x36u32), ('8', 0x38u32)];

    for (name, cp) in &debug_chars {
        let glyph_id = match font.lookup_glyph_index(*cp) {
            Some(id) => id,
            None => { eprintln!("'{}': glyph not found", name); continue; }
        };
        let owned = font.glyph_records_decoded.get(&glyph_id).unwrap();
        let raw_points = owned.raw_points.as_ref().unwrap();
        let raw_on_curve = owned.raw_on_curve.as_ref().unwrap();
        let raw_contour_ends = owned.raw_contour_ends.as_ref().unwrap();
        let instructions = owned.instructions.as_ref();

        eprintln!("=== '{}' (gid={}) ===", name, glyph_id);
        eprintln!("  {} points, {} contours, contour_ends={:?}",
            raw_points.len(), raw_contour_ends.len(), raw_contour_ends);
        eprintln!("  instructions: {} bytes", instructions.map_or(0, |i| i.len()));

        let hinted = match hint_glyph(&font, *cp, ppem) {
            Some(h) => h,
            None => { eprintln!("  HINT FAILED"); continue; }
        };

        eprintln!("  Hinted points:");
        for (i, &(x, y)) in hinted.iter().enumerate() {
            let is_end = raw_contour_ends.contains(&(i as u16));
            eprintln!("    pt[{:2}]: ({:6},{:6}) = ({:7.3},{:7.3}) px  {}{}",
                i, x, y, x as f64 / 64.0, y as f64 / 64.0,
                if raw_on_curve[i] { "ON " } else { "OFF" },
                if is_end { " <END>" } else { "" });
        }
    }
}

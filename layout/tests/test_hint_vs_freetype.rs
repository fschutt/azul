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

/// Render Times New Roman "s" and "u" at 16px to debug serif hinting issue.
#[test]
fn test_times_serif_hinting() {
    use tiny_skia::{Pixmap, Paint, FillRule, Transform, Color};

    let font_bytes = std::fs::read("/System/Library/Fonts/Supplemental/Times New Roman.ttf")
        .or_else(|_| std::fs::read("/System/Library/Fonts/Times.ttc"))
        .ok();
    let font_bytes = match font_bytes {
        Some(b) => b,
        None => { eprintln!("Skipping: Times font not found"); return; }
    };
    let mut warnings = Vec::new();
    let font = match ParsedFont::from_bytes(&font_bytes, 0, &mut warnings) {
        Some(f) => f,
        None => { eprintln!("Failed to parse Times"); return; }
    };

    let ppem: u16 = 16;
    let scale_up: u32 = 12;
    let glyph_w: u32 = 12;
    let glyph_h: u32 = 18;

    let test_chars = ['s', 'u', 'T', 'e', 'a', 'h', 'n', 'p'];
    let cols = test_chars.len() as u32;

    let img_w = cols * glyph_w * scale_up;
    let img_h = glyph_h * scale_up;

    let mut pixmap = Pixmap::new(img_w, img_h).unwrap();
    pixmap.fill(Color::from_rgba8(255, 255, 255, 255));

    let mut paint = Paint::default();
    paint.set_color(Color::from_rgba8(0, 0, 0, 255));
    paint.anti_alias = true;

    for (i, ch) in test_chars.iter().enumerate() {
        let cp = *ch as u32;
        let glyph_id = match font.lookup_glyph_index(cp) {
            Some(id) => id,
            None => { eprintln!("'{ch}': glyph not found"); continue; }
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

        let hinted = match hint_glyph_any(&font, cp, ppem) {
            Some(h) => h,
            None => { eprintln!("'{ch}': hint failed"); continue; }
        };

        // Print hinted points for 's' and 'u'
        if *ch == 's' || *ch == 'u' {
            eprintln!("\n'{ch}' hinted points ({} pts, {} contours):", hinted.len(), raw_contour_ends.len());
            for (pi, &(x, y)) in hinted.iter().enumerate() {
                let on = if pi < raw_on_curve.len() { if raw_on_curve[pi] { "ON " } else { "OFF" } } else { "?  " };
                eprintln!("  pt[{pi:2}]: ({x:6},{y:6}) = ({:8.4},{:8.4}) px  {on}",
                    x as f32 / 64.0, y as f32 / 64.0);
            }
        }

        let path = match build_path_from_contours(&hinted, raw_on_curve, raw_contour_ends) {
            Some(p) => p,
            None => continue,
        };

        let x_off = (i as u32 * glyph_w * scale_up) as f32;
        let y_off = (glyph_h as f32 - 3.0) * scale_up as f32;

        let transform = Transform::from_scale(scale_up as f32, scale_up as f32)
            .post_translate(x_off, y_off);

        pixmap.fill_path(&path, &paint, FillRule::Winding, transform, None);
    }

    let path = "/tmp/times_serif_hinting.png";
    pixmap.save_png(path).unwrap();
    eprintln!("Wrote {path}");
}

/// Helper to hint a glyph using any ParsedFont (not just HelveticaNeue)
fn hint_glyph_any(font: &ParsedFont, codepoint: u32, ppem: u16) -> Option<Vec<(i32, i32)>> {
    let glyph_id = font.lookup_glyph_index(codepoint)?;
    let owned = font.glyph_records_decoded.get(&glyph_id)?;
    let raw_points = owned.raw_points.as_ref()?;
    let raw_on_curve = owned.raw_on_curve.as_ref()?;
    let raw_contour_ends = owned.raw_contour_ends.as_ref()?;
    let instructions = owned.instructions.as_ref()?;

    let hint_mutex = font.hint_instance.as_ref()?;
    let mut hint = hint_mutex.lock().ok()?;

    let upem = font.font_metrics.units_per_em;
    hint.set_ppem(ppem, ppem as f64).ok()?;

    let scale = compute_scale(ppem, upem);
    let points_f26dot6: Vec<(i32, i32)> = raw_points
        .iter()
        .map(|&(x, y)| {
            (F26Dot6::from_funits(x as i32, scale).to_bits(),
             F26Dot6::from_funits(y as i32, scale).to_bits())
        })
        .collect();

    let adv_f26dot6 = F26Dot6::from_funits(owned.horz_advance as i32, scale).to_bits();

    hint.hint_glyph_with_orus(
        &points_f26dot6,
        Some(raw_points.as_slice()),
        raw_on_curve,
        raw_contour_ends,
        instructions,
        adv_f26dot6,
    ).ok()
}

/// Compare hinted vs unhinted "8" from Times New Roman at ppem=80 to debug bulge distortion.
#[test]
fn test_digit_8_hinting_comparison() {
    use tiny_skia::{Pixmap, Paint, FillRule, Transform, Color};

    let font_bytes = std::fs::read("/System/Library/Fonts/Supplemental/Times New Roman.ttf")
        .or_else(|_| std::fs::read("/System/Library/Fonts/Times.ttc"))
        .ok();
    let font_bytes = match font_bytes {
        Some(b) => b,
        None => { eprintln!("Skipping: Times font not found"); return; }
    };
    let mut warnings = Vec::new();
    let font = match ParsedFont::from_bytes(&font_bytes, 0, &mut warnings) {
        Some(f) => f,
        None => { eprintln!("Failed to parse Times"); return; }
    };

    let ppem: u16 = 80;
    let upem = font.font_metrics.units_per_em;
    let scale_f = ppem as f32 / upem as f32;
    let glyph_id = font.lookup_glyph_index('8' as u32).unwrap();
    let owned = font.glyph_records_decoded.get(&glyph_id).unwrap();
    let raw_points = owned.raw_points.as_ref().unwrap();
    let raw_on_curve = owned.raw_on_curve.as_ref().unwrap();
    let raw_contour_ends = owned.raw_contour_ends.as_ref().unwrap();

    // Hinted points
    let hinted = hint_glyph_any(&font, '8' as u32, ppem).unwrap();

    // Unhinted points (just scaled)
    let scale = compute_scale(ppem, upem);
    let unhinted: Vec<(i32, i32)> = raw_points.iter().map(|&(x, y)| {
        (F26Dot6::from_funits(x as i32, scale).to_bits(),
         F26Dot6::from_funits(y as i32, scale).to_bits())
    }).collect();

    eprintln!("'8' at ppem={ppem}: {} points, {} contours", hinted.len(), raw_contour_ends.len());
    eprintln!("Contour ends: {:?}", raw_contour_ends);
    eprintln!("\n{:>4} {:>10} {:>10}  {:>10} {:>10}  {:>6} {:>6}  {}",
        "pt", "hint_x", "hint_y", "unhint_x", "unhint_y", "dx", "dy", "on");
    for i in 0..hinted.len() {
        let (hx, hy) = hinted[i];
        let (ux, uy) = unhinted[i];
        let dx = hx - ux;
        let dy = hy - uy;
        let on = if raw_on_curve[i] { "ON " } else { "OFF" };
        // Flag large deviations
        let flag = if dx.abs() > 64 || dy.abs() > 64 { " <<<" } else { "" };
        eprintln!("{:4} {:10} {:10}  {:10} {:10}  {:6} {:6}  {} {}",
            i, hx, hy, ux, uy, dx, dy, on, flag);
    }

    // Render side-by-side: hinted vs unhinted
    let gw: u32 = (ppem as u32) + 10;
    let gh: u32 = (ppem as u32) + 10;
    let img_w = gw * 2;
    let img_h = gh;
    let mut pixmap = Pixmap::new(img_w, img_h).unwrap();
    pixmap.fill(Color::from_rgba8(255, 255, 255, 255));
    let mut paint = Paint::default();
    paint.set_color(Color::from_rgba8(0, 0, 0, 255));
    paint.anti_alias = true;

    // Hinted path
    if let Some(path) = build_path_from_contours(&hinted, raw_on_curve, raw_contour_ends) {
        let t = Transform::from_translate(2.0, (ppem as f32) * 0.85);
        pixmap.fill_path(&path, &paint, FillRule::Winding, t, None);
    }

    // Unhinted path (scale from font units)
    if let Some(path) = build_path_from_contours(&unhinted, raw_on_curve, raw_contour_ends) {
        let t = Transform::from_translate(gw as f32 + 2.0, (ppem as f32) * 0.85);
        pixmap.fill_path(&path, &paint, FillRule::Winding, t, None);
    }

    let out_path = "/tmp/digit_8_hinted_vs_unhinted.png";
    pixmap.save_png(out_path).unwrap();
    eprintln!("Wrote {out_path} (left=hinted, right=unhinted)");
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

/// Render suspect glyphs UNHINTED (using raw scaled points + build_path_from_contours)
/// vs HINTED to isolate whether breakouts come from path builder or hinting.
#[test]
fn test_render_hinted_vs_unhinted() {
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
        ('H', 0x48), ('M', 0x4D), ('N', 0x4E), ('W', 0x57),
        ('a', 0x61), ('e', 0x65), ('6', 0x36), ('8', 0x38),
    ];

    let cols = chars.len() as u32;
    let img_w = cols * glyph_w * scale_up;
    let img_h = glyph_h * scale_up * 2; // two rows: unhinted on top, hinted on bottom

    let mut pixmap = Pixmap::new(img_w, img_h).unwrap();
    pixmap.fill(Color::from_rgba8(255, 255, 255, 255));

    let mut paint = Paint::default();
    paint.set_color(Color::from_rgba8(0, 0, 0, 255));
    paint.anti_alias = true;

    let upem = font.font_metrics.units_per_em;
    let scale = compute_scale(ppem, upem);

    for (i, (ch, cp)) in chars.iter().enumerate() {
        let glyph_id = match font.lookup_glyph_index(*cp) {
            Some(id) => id, None => continue,
        };
        let owned = match font.glyph_records_decoded.get(&glyph_id) {
            Some(o) => o, None => continue,
        };
        let raw_on_curve = match owned.raw_on_curve.as_ref() { Some(f) => f, None => continue };
        let raw_contour_ends = match owned.raw_contour_ends.as_ref() { Some(c) => c, None => continue };
        let raw_points = match owned.raw_points.as_ref() { Some(p) => p, None => continue };

        let x_off = (i as u32 * glyph_w * scale_up) as f32;

        // Row 1: UNHINTED (raw points scaled to F26Dot6, no hinting applied)
        let unhinted: Vec<(i32, i32)> = raw_points.iter().map(|&(x, y)| {
            (F26Dot6::from_funits(x as i32, scale).to_bits(),
             F26Dot6::from_funits(y as i32, scale).to_bits())
        }).collect();

        if let Some(path) = build_path_from_contours(&unhinted, raw_on_curve, raw_contour_ends) {
            let y_off = (glyph_h as f32 - 3.0) * scale_up as f32;
            let transform = Transform::from_scale(scale_up as f32, scale_up as f32)
                .post_translate(x_off, y_off);
            pixmap.fill_path(&path, &paint, FillRule::Winding, transform, None);
        }

        // Row 2: HINTED
        if let Some(hinted) = hint_glyph(&font, *cp, ppem) {
            if let Some(path) = build_path_from_contours(&hinted, raw_on_curve, raw_contour_ends) {
                let y_off = (glyph_h as f32 - 3.0) * scale_up as f32 + (glyph_h * scale_up) as f32;
                let transform = Transform::from_scale(scale_up as f32, scale_up as f32)
                    .post_translate(x_off, y_off);
                pixmap.fill_path(&path, &paint, FillRule::Winding, transform, None);
            }
        }
    }

    let path = "/tmp/hinted_vs_unhinted.png";
    pixmap.save_png(path).unwrap();
    eprintln!("Wrote {} (top=unhinted, bottom=hinted)", path);
}

/// Test EvenOdd vs Winding fill rule to diagnose counter artifacts.
#[test]
fn test_fill_rule_comparison() {
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
        ('M', 0x4D), ('N', 0x4E), ('a', 0x61), ('e', 0x65), ('6', 0x36), ('8', 0x38),
    ];

    let cols = chars.len() as u32;
    let img_w = cols * glyph_w * scale_up;
    let img_h = glyph_h * scale_up * 2;

    let mut pixmap = Pixmap::new(img_w, img_h).unwrap();
    pixmap.fill(Color::from_rgba8(255, 255, 255, 255));

    let mut paint = Paint::default();
    paint.set_color(Color::from_rgba8(0, 0, 0, 255));
    paint.anti_alias = true;

    let upem = font.font_metrics.units_per_em;
    let scale = compute_scale(ppem, upem);

    for (i, (_ch, cp)) in chars.iter().enumerate() {
        let glyph_id = match font.lookup_glyph_index(*cp) {
            Some(id) => id, None => continue,
        };
        let owned = match font.glyph_records_decoded.get(&glyph_id) {
            Some(o) => o, None => continue,
        };
        let raw_on_curve = match owned.raw_on_curve.as_ref() { Some(f) => f, None => continue };
        let raw_contour_ends = match owned.raw_contour_ends.as_ref() { Some(c) => c, None => continue };
        let raw_points = match owned.raw_points.as_ref() { Some(p) => p, None => continue };

        let x_off = (i as u32 * glyph_w * scale_up) as f32;

        let unhinted: Vec<(i32, i32)> = raw_points.iter().map(|&(x, y)| {
            (F26Dot6::from_funits(x as i32, scale).to_bits(),
             F26Dot6::from_funits(y as i32, scale).to_bits())
        }).collect();

        if let Some(path) = build_path_from_contours(&unhinted, raw_on_curve, raw_contour_ends) {
            // Row 1: Winding
            let y_off = (glyph_h as f32 - 3.0) * scale_up as f32;
            let transform = Transform::from_scale(scale_up as f32, scale_up as f32)
                .post_translate(x_off, y_off);
            pixmap.fill_path(&path, &paint, FillRule::Winding, transform, None);

            // Row 2: EvenOdd
            let y_off2 = y_off + (glyph_h * scale_up) as f32;
            let transform2 = Transform::from_scale(scale_up as f32, scale_up as f32)
                .post_translate(x_off, y_off2);
            pixmap.fill_path(&path, &paint, FillRule::EvenOdd, transform2, None);
        }
    }

    let path = "/tmp/fill_rule_comparison.png";
    pixmap.save_png(path).unwrap();
    eprintln!("Wrote {} (top=Winding, bottom=EvenOdd)", path);
}

/// Dump path segments for 'M' to diagnose the diagonal fill issue.
#[test]
fn test_dump_m_contour() {
    let font = match load_helvetica_neue() {
        Some(f) => f,
        None => { eprintln!("Skipping"); return; }
    };

    let ppem: u16 = 16;
    let upem = font.font_metrics.units_per_em;
    let scale = compute_scale(ppem, upem);

    // Dump M, N, 8
    for (name, cp) in &[('M', 0x4Du32), ('N', 0x4Eu32), ('8', 0x38u32)] {
        let glyph_id = font.lookup_glyph_index(*cp).unwrap();
        let owned = font.glyph_records_decoded.get(&glyph_id).unwrap();
        let raw_points = owned.raw_points.as_ref().unwrap();
        let raw_on_curve = owned.raw_on_curve.as_ref().unwrap();
        let raw_contour_ends = owned.raw_contour_ends.as_ref().unwrap();

        let scaled: Vec<(i32, i32)> = raw_points.iter().map(|&(x, y)| {
            (F26Dot6::from_funits(x as i32, scale).to_bits(),
             F26Dot6::from_funits(y as i32, scale).to_bits())
        }).collect();

        eprintln!("\n=== '{}' contour dump ===", name);
        eprintln!("  {} points, contour_ends={:?}", raw_points.len(), raw_contour_ends);

        let mut contour_start = 0usize;
        for (ci, &end_idx) in raw_contour_ends.iter().enumerate() {
            let end = end_idx as usize;
            eprintln!("  Contour {} (pts {}..={}):", ci, contour_start, end);
            for j in contour_start..=end {
                let (x, y) = scaled[j];
                eprintln!("    [{:2}] ({:7.3}, {:7.3}) px  {}",
                    j, x as f64 / 64.0, -y as f64 / 64.0,
                    if raw_on_curve[j] { "ON" } else { "OFF" });
            }
            contour_start = end + 1;
        }
    }
}

/// Compare build_glyph_path (visitor-based) vs build_path_from_contours (raw TrueType)
/// to see if the path builder itself produces wrong winding.
#[test]
fn test_visitor_vs_contour_path() {
    use tiny_skia::{Pixmap, Paint, FillRule, Transform, Color};
    use azul_layout::font::parsed::build_glyph_path;

    let font = match load_helvetica_neue() {
        Some(f) => f,
        None => { eprintln!("Skipping"); return; }
    };

    let scale_up: u32 = 20;
    let glyph_w: u32 = 14;
    let glyph_h: u32 = 14;
    let font_scale = 16.0 / font.font_metrics.units_per_em as f32;

    let chars: Vec<(char, u32)> = vec![
        ('M', 0x4D), ('a', 0x61), ('e', 0x65), ('6', 0x36), ('8', 0x38),
    ];

    let cols = chars.len() as u32;
    let img_w = cols * glyph_w * scale_up;
    let img_h = glyph_h * scale_up * 2;

    let mut pixmap = Pixmap::new(img_w, img_h).unwrap();
    pixmap.fill(Color::from_rgba8(255, 255, 255, 255));

    let mut paint = Paint::default();
    paint.set_color(Color::from_rgba8(0, 0, 0, 255));
    paint.anti_alias = true;

    let ppem: u16 = 16;
    let upem = font.font_metrics.units_per_em;
    let scale = compute_scale(ppem, upem);

    for (i, (_ch, cp)) in chars.iter().enumerate() {
        let glyph_id = match font.lookup_glyph_index(*cp) {
            Some(id) => id, None => continue,
        };
        let owned = match font.glyph_records_decoded.get(&glyph_id) {
            Some(o) => o, None => continue,
        };

        let x_off = (i as u32 * glyph_w * scale_up) as f32;

        // Row 1: build_glyph_path (visitor-based, uses GlyphOutline data)
        if let Some(path) = build_glyph_path(owned) {
            let y_off = (glyph_h as f32 - 3.0) * scale_up as f32;
            let transform = Transform::from_scale(
                font_scale * scale_up as f32,
                font_scale * scale_up as f32,
            ).post_translate(x_off, y_off);
            pixmap.fill_path(&path, &paint, FillRule::Winding, transform, None);
        }

        // Row 2: build_path_from_contours (raw TrueType data)
        let raw_on_curve = match owned.raw_on_curve.as_ref() { Some(f) => f, None => continue };
        let raw_contour_ends = match owned.raw_contour_ends.as_ref() { Some(c) => c, None => continue };
        let raw_points = match owned.raw_points.as_ref() { Some(p) => p, None => continue };

        let scaled: Vec<(i32, i32)> = raw_points.iter().map(|&(x, y)| {
            (F26Dot6::from_funits(x as i32, scale).to_bits(),
             F26Dot6::from_funits(y as i32, scale).to_bits())
        }).collect();

        if let Some(path) = build_path_from_contours(&scaled, raw_on_curve, raw_contour_ends) {
            let y_off = (glyph_h as f32 - 3.0) * scale_up as f32 + (glyph_h * scale_up) as f32;
            let transform = Transform::from_scale(scale_up as f32, scale_up as f32)
                .post_translate(x_off, y_off);
            pixmap.fill_path(&path, &paint, FillRule::Winding, transform, None);
        }
    }

    let path = "/tmp/visitor_vs_contour.png";
    pixmap.save_png(path).unwrap();
    eprintln!("Wrote {} (top=visitor/build_glyph_path, bottom=raw/build_path_from_contours)", path);
}

/// Render each contour of '8' separately to check winding directions.
#[test]
fn test_eight_contours_separate() {
    use tiny_skia::{Pixmap, Paint, FillRule, Transform, Color};
    use azul_layout::font::parsed::build_glyph_path;

    let font = match load_helvetica_neue() {
        Some(f) => f,
        None => { eprintln!("Skipping"); return; }
    };

    let ppem: u16 = 16;
    let scale_up: u32 = 30;
    let glyph_w: u32 = 12;
    let glyph_h: u32 = 14;
    let upem = font.font_metrics.units_per_em;
    let scale = compute_scale(ppem, upem);
    let font_scale = ppem as f32 / upem as f32;

    let glyph_id = font.lookup_glyph_index(0x38).unwrap(); // '8'
    let owned = font.glyph_records_decoded.get(&glyph_id).unwrap();
    let raw_on_curve = owned.raw_on_curve.as_ref().unwrap();
    let raw_contour_ends = owned.raw_contour_ends.as_ref().unwrap();
    let raw_points = owned.raw_points.as_ref().unwrap();

    let scaled: Vec<(i32, i32)> = raw_points.iter().map(|&(x, y)| {
        (F26Dot6::from_funits(x as i32, scale).to_bits(),
         F26Dot6::from_funits(y as i32, scale).to_bits())
    }).collect();

    // 4 columns: all contours together, contour 0 alone, contour 1 alone, contour 2 alone
    // 2 rows: our build_path_from_contours vs allsorts build_glyph_path
    let cols = 5u32;
    let img_w = cols * glyph_w * scale_up;
    let img_h = glyph_h * scale_up * 2;

    let mut pixmap = Pixmap::new(img_w, img_h).unwrap();
    pixmap.fill(Color::from_rgba8(255, 255, 255, 255));

    let colors = [
        Color::from_rgba8(255, 0, 0, 255),   // red
        Color::from_rgba8(0, 128, 0, 255),    // green
        Color::from_rgba8(0, 0, 255, 255),    // blue
    ];

    let y_off = (glyph_h as f32 - 3.0) * scale_up as f32;

    // Row 1: build_path_from_contours - all together with Winding
    let mut paint = Paint::default();
    paint.set_color(Color::from_rgba8(0, 0, 0, 255));
    paint.anti_alias = true;
    if let Some(path) = build_path_from_contours(&scaled, raw_on_curve, raw_contour_ends) {
        let transform = Transform::from_scale(scale_up as f32, scale_up as f32)
            .post_translate(0.0, y_off);
        pixmap.fill_path(&path, &paint, FillRule::Winding, transform, None);
    }

    // Row 1: each contour separately
    let mut contour_start = 0usize;
    for (ci, &end_idx) in raw_contour_ends.iter().enumerate() {
        let end = end_idx as usize;
        let single_ends = vec![end_idx - contour_start as u16];
        let pts = &scaled[contour_start..=end];
        let flags = &raw_on_curve[contour_start..=end];

        if let Some(path) = build_path_from_contours(pts, flags, &single_ends) {
            let mut p = Paint::default();
            p.set_color(colors[ci % 3]);
            p.anti_alias = true;
            let x_off = ((ci + 1) as u32 * glyph_w * scale_up) as f32;
            let transform = Transform::from_scale(scale_up as f32, scale_up as f32)
                .post_translate(x_off, y_off);
            pixmap.fill_path(&path, &p, FillRule::Winding, transform, None);
        }
        contour_start = end + 1;
    }

    // Row 1, col 4: all contours with EvenOdd
    if let Some(path) = build_path_from_contours(&scaled, raw_on_curve, raw_contour_ends) {
        let transform = Transform::from_scale(scale_up as f32, scale_up as f32)
            .post_translate(4.0 * glyph_w as f32 * scale_up as f32, y_off);
        pixmap.fill_path(&path, &paint, FillRule::EvenOdd, transform, None);
    }

    // Row 2: build_glyph_path (visitor) - all together + EvenOdd
    let y_off2 = y_off + (glyph_h * scale_up) as f32;
    if let Some(path) = build_glyph_path(owned) {
        let transform = Transform::from_scale(
            font_scale * scale_up as f32, font_scale * scale_up as f32
        ).post_translate(0.0, y_off2);
        pixmap.fill_path(&path, &paint, FillRule::Winding, transform, None);

        // Also with EvenOdd
        let transform2 = Transform::from_scale(
            font_scale * scale_up as f32, font_scale * scale_up as f32
        ).post_translate(4.0 * glyph_w as f32 * scale_up as f32, y_off2);
        pixmap.fill_path(&path, &paint, FillRule::EvenOdd, transform2, None);
    }

    // Row 2: each contour from visitor
    for (ci, outline) in owned.outline.iter().enumerate() {
        let mut pb = tiny_skia::PathBuilder::new();
        for op in outline.operations.as_slice() {
            match op {
                azul_core::resources::GlyphOutlineOperation::MoveTo(m) => {
                    pb.move_to(m.x as f32, -(m.y as f32));
                }
                azul_core::resources::GlyphOutlineOperation::LineTo(l) => {
                    pb.line_to(l.x as f32, -(l.y as f32));
                }
                azul_core::resources::GlyphOutlineOperation::QuadraticCurveTo(q) => {
                    pb.quad_to(q.ctrl_1_x as f32, -(q.ctrl_1_y as f32),
                              q.end_x as f32, -(q.end_y as f32));
                }
                azul_core::resources::GlyphOutlineOperation::CubicCurveTo(c) => {
                    pb.cubic_to(c.ctrl_1_x as f32, -(c.ctrl_1_y as f32),
                               c.ctrl_2_x as f32, -(c.ctrl_2_y as f32),
                               c.end_x as f32, -(c.end_y as f32));
                }
                azul_core::resources::GlyphOutlineOperation::ClosePath => {
                    pb.close();
                }
            }
        }
        if let Some(path) = pb.finish() {
            let mut p = Paint::default();
            p.set_color(colors[ci % 3]);
            p.anti_alias = true;
            let x_off = ((ci + 1) as u32 * glyph_w * scale_up) as f32;
            let transform = Transform::from_scale(
                font_scale * scale_up as f32, font_scale * scale_up as f32
            ).post_translate(x_off, y_off2);
            pixmap.fill_path(&path, &p, FillRule::Winding, transform, None);
        }
    }

    let path = "/tmp/eight_contours.png";
    pixmap.save_png(path).unwrap();
    eprintln!("Wrote {} (row1=our contour builder, row2=visitor)", path);
    eprintln!("Col 0=all/Winding, Col 1-3=separate contours R/G/B, Col 4=all/EvenOdd");
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

/// Compare path operations from build_glyph_path vs build_path_from_contours
/// for 'O' glyph to find the exact winding difference.
#[test]
fn test_compare_path_ops() {
    use tiny_skia::{Pixmap, Paint, FillRule, Transform, Color};
    use azul_layout::font::parsed::build_glyph_path;

    let font = match load_helvetica_neue() {
        Some(f) => f,
        None => { eprintln!("Skipping"); return; }
    };

    let ppem: u16 = 16;
    let upem = font.font_metrics.units_per_em;
    let scale = compute_scale(ppem, upem);

    let test_chars = [('O', 0x4Fu32), ('8', 0x38u32), ('a', 0x61u32)];

    for (name, cp) in &test_chars {
        let glyph_id = match font.lookup_glyph_index(*cp) {
            Some(id) => id, None => continue,
        };
        let owned = match font.glyph_records_decoded.get(&glyph_id) {
            Some(o) => o, None => continue,
        };

        let raw_points = owned.raw_points.as_ref().unwrap();
        let raw_on_curve = owned.raw_on_curve.as_ref().unwrap();
        let raw_contour_ends = owned.raw_contour_ends.as_ref().unwrap();

        eprintln!("\n=== '{}' (gid={}) ===", name, glyph_id);
        eprintln!("  {} points, {} contours, ends={:?}",
            raw_points.len(), raw_contour_ends.len(), raw_contour_ends);

        // Compute signed area (shoelace) for each contour to determine winding
        let mut contour_start = 0usize;
        for (ci, &end_idx) in raw_contour_ends.iter().enumerate() {
            let end = end_idx as usize;
            let pts = &raw_points[contour_start..=end];

            // Shoelace on font-unit coords (Y negated for screen space)
            let mut area_font = 0.0f64;
            for i in 0..pts.len() {
                let j = (i + 1) % pts.len();
                let x0 = pts[i].0 as f64;
                let y0 = -(pts[i].1 as f64);
                let x1 = pts[j].0 as f64;
                let y1 = -(pts[j].1 as f64);
                area_font += x0 * y1 - x1 * y0;
            }
            area_font /= 2.0;

            // Same for F26Dot6 scaled
            let scaled: Vec<(f64, f64)> = pts.iter().map(|&(x, y)| {
                let sx = F26Dot6::from_funits(x as i32, scale).to_bits() as f64 / 64.0;
                let sy = -(F26Dot6::from_funits(y as i32, scale).to_bits() as f64 / 64.0);
                (sx, sy)
            }).collect();
            let mut area_f26 = 0.0f64;
            for i in 0..scaled.len() {
                let j = (i + 1) % scaled.len();
                area_f26 += scaled[i].0 * scaled[j].1 - scaled[j].0 * scaled[i].1;
            }
            area_f26 /= 2.0;

            let dir_font = if area_font > 0.0 { "CCW" } else { "CW" };
            let dir_f26 = if area_f26 > 0.0 { "CCW" } else { "CW" };
            eprintln!("  Contour {}: font_area={:.1} ({}) f26_area={:.4} ({}) {}",
                ci, area_font, dir_font, area_f26, dir_f26,
                if dir_font != dir_f26 { "<<< MISMATCH!" } else { "OK" });

            contour_start = end + 1;
        }
    }

    // Render comparison: 4 columns x 3 rows
    let scale_up: u32 = 20;
    let glyph_w: u32 = 16;
    let glyph_h: u32 = 16;
    let font_scale = ppem as f32 / upem as f32;

    let rows = test_chars.len() as u32;
    let cols = 4u32;
    let img_w = cols * glyph_w * scale_up;
    let img_h = rows * glyph_h * scale_up;

    let mut pixmap = Pixmap::new(img_w, img_h).unwrap();
    pixmap.fill(Color::from_rgba8(255, 255, 255, 255));

    let mut paint = Paint::default();
    paint.set_color(Color::from_rgba8(0, 0, 0, 255));
    paint.anti_alias = true;

    for (row, (_name, cp)) in test_chars.iter().enumerate() {
        let glyph_id = match font.lookup_glyph_index(*cp) {
            Some(id) => id, None => continue,
        };
        let owned = match font.glyph_records_decoded.get(&glyph_id) {
            Some(o) => o, None => continue,
        };
        let raw_points = owned.raw_points.as_ref().unwrap();
        let raw_on_curve = owned.raw_on_curve.as_ref().unwrap();
        let raw_contour_ends = owned.raw_contour_ends.as_ref().unwrap();

        let y_off = (row as u32 * glyph_h * scale_up) as f32 + (glyph_h as f32 - 3.0) * scale_up as f32;

        // Col 0: build_glyph_path + Winding
        if let Some(path) = build_glyph_path(owned) {
            let transform = Transform::from_scale(font_scale * scale_up as f32, font_scale * scale_up as f32)
                .post_translate(0.0, y_off);
            pixmap.fill_path(&path, &paint, FillRule::Winding, transform, None);
        }

        // Col 1: build_path_from_contours (F26Dot6) + Winding
        let scaled_f26: Vec<(i32, i32)> = raw_points.iter().map(|&(x, y)| {
            (F26Dot6::from_funits(x as i32, scale).to_bits(),
             F26Dot6::from_funits(y as i32, scale).to_bits())
        }).collect();
        if let Some(path) = build_path_from_contours(&scaled_f26, raw_on_curve, raw_contour_ends) {
            let x_off = (1 * glyph_w * scale_up) as f32;
            let transform = Transform::from_scale(scale_up as f32, scale_up as f32)
                .post_translate(x_off, y_off);
            pixmap.fill_path(&path, &paint, FillRule::Winding, transform, None);
        }

        // Col 2: build_path_from_contours (font_units << 6) + Winding
        let scaled_raw: Vec<(i32, i32)> = raw_points.iter().map(|&(x, y)| {
            ((x as i32) << 6, (y as i32) << 6)
        }).collect();
        if let Some(path) = build_path_from_contours(&scaled_raw, raw_on_curve, raw_contour_ends) {
            let x_off = (2 * glyph_w * scale_up) as f32;
            let transform = Transform::from_scale(font_scale * scale_up as f32, font_scale * scale_up as f32)
                .post_translate(x_off, y_off);
            pixmap.fill_path(&path, &paint, FillRule::Winding, transform, None);
        }

        // Col 3: build_path_from_contours (F26Dot6) + EvenOdd
        if let Some(path) = build_path_from_contours(&scaled_f26, raw_on_curve, raw_contour_ends) {
            let x_off = (3 * glyph_w * scale_up) as f32;
            let transform = Transform::from_scale(scale_up as f32, scale_up as f32)
                .post_translate(x_off, y_off);
            pixmap.fill_path(&path, &paint, FillRule::EvenOdd, transform, None);
        }
    }

    let path = "/tmp/compare_path_ops.png";
    pixmap.save_png(path).unwrap();
    eprintln!("Wrote {}", path);
    eprintln!("Cols: 0=visitor+Winding, 1=contour(F26)+Winding, 2=contour(raw<<6)+Winding, 3=contour(F26)+EvenOdd");
}

/// Debug kerning: check if GPOS kern is applied for "Te" pair in Times New Roman.
#[test]
fn test_debug_kerning() {
    // Try Times New Roman or Times
    let font_bytes = std::fs::read("/System/Library/Fonts/Supplemental/Times New Roman.ttf")
        .or_else(|_| std::fs::read("/System/Library/Fonts/Times.ttc"))
        .ok();
    let font_bytes = match font_bytes {
        Some(b) => b,
        None => { eprintln!("Skipping: Times font not found"); return; }
    };
    let mut warnings = Vec::new();
    let font = match ParsedFont::from_bytes(&font_bytes, 0, &mut warnings) {
        Some(f) => f,
        None => { eprintln!("Failed to parse Times"); return; }
    };

    eprintln!("Font: {} glyphs, upem={}", font.glyph_records_decoded.len(), font.font_metrics.units_per_em);
    eprintln!("Has GPOS: {}", font.gpos_cache.is_some());
    eprintln!("Has kern table: {}", font.opt_kern_table.is_some());

    let t_gid = font.lookup_glyph_index(0x54); // 'T'
    let e_gid = font.lookup_glyph_index(0x65); // 'e'
    eprintln!("T gid={:?}, e gid={:?}", t_gid, e_gid);

    if let (Some(t), Some(e)) = (t_gid, e_gid) {
        let t_adv = font.get_horizontal_advance(t);
        let e_adv = font.get_horizontal_advance(e);
        let upem = font.font_metrics.units_per_em as f32;
        eprintln!("T advance={} funits ({:.2}px @16px), e advance={} funits ({:.2}px @16px)",
            t_adv, t_adv as f32 * 16.0 / upem, e_adv, e_adv as f32 * 16.0 / upem);
    }

    // Shape "Test" and check kerning values
    use allsorts::gpos;
    use allsorts::gsub::{self, Features, RawGlyph, RawGlyphFlags};

    let text = "Test passes";
    let opt_gdef = font.opt_gdef_table.as_ref().map(|v| &**v);

    let mut raw_glyphs: Vec<RawGlyph<()>> = Vec::new();
    for ch in text.chars() {
        let gid = font.lookup_glyph_index(ch as u32).unwrap_or(0);
        raw_glyphs.push(RawGlyph {
            unicodes: tinyvec::tiny_vec!([char; 1] => ch),
            glyph_index: gid,
            liga_component_pos: 0,
            glyph_origin: gsub::GlyphOrigin::Char(ch),
            flags: RawGlyphFlags::empty(),
            variation: None,
            extra_data: (),
        });
    }

    let mut infos = gpos::Info::init_from_glyphs(opt_gdef, raw_glyphs);

    if let Some(gpos_cache) = font.gpos_cache.as_ref() {
        let kern_table = font.opt_kern_table.as_ref().map(|kt| kt.as_borrowed());
        let apply_kerning = true;
        let script_tag = allsorts::tag::LATN;
        let lang_tag = allsorts::tag::DFLT;

        match gpos::apply(
            gpos_cache,
            opt_gdef,
            kern_table,
            apply_kerning,
            &Features::Custom(vec![]),
            None,
            script_tag,
            Some(lang_tag),
            &mut infos,
        ) {
            Ok(()) => eprintln!("GPOS apply succeeded"),
            Err(e) => eprintln!("GPOS apply failed: {:?}", e),
        }
    }

    let scale = 16.0 / font.font_metrics.units_per_em as f32;
    let ppem: u16 = 16;
    for (i, info) in infos.iter().enumerate() {
        let ch = text.chars().nth(i).unwrap_or('?');
        let adv = font.get_horizontal_advance(info.glyph.glyph_index);
        let unhinted = adv as f32 * scale;
        let hinted = font.get_hinted_advance_px(info.glyph.glyph_index, ppem);
        eprintln!("  [{}] '{}' gid={} adv_unhinted={:.3}px hinted={:?}px kern={} ({:.2}px)",
            i, ch, info.glyph.glyph_index,
            unhinted, hinted,
            info.kerning, info.kerning as f32 * scale);
    }
}

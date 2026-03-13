//! Two-phase hinting debug test for HelveticaNeue.ttc glyphs R, T, f.
//!
//! Phase 1 (decoding): dumps raw glyph data and compares with fonttools reference.
//! Phase 2 (interpretation): runs hint_glyph and dumps hinted output.
//!
//! Output files:
//!   /tmp/phase1_decoding_allsorts.txt  — raw glyph data from allsorts
//!   /tmp/phase2_hinting_allsorts.txt   — hinted point output from allsorts
//!   /tmp/font_data_allsorts.txt        — font-level data (CVT, fpgm, prep, maxp)

use std::fmt::Write as FmtWrite;
use std::fs;

use azul_layout::font::parsed::ParsedFont;

use allsorts::binary::read::ReadScope;
use allsorts::font_data::FontData;
use allsorts::hinting::f26dot6::{compute_scale, F26Dot6};
use allsorts::tables::FontTableProvider;
use allsorts::tag;

fn load_helvetica_neue() -> Option<ParsedFont> {
    let font_path = "/System/Library/Fonts/HelveticaNeue.ttc";
    let font_bytes = fs::read(font_path).ok()?;
    let mut warnings = Vec::new();
    ParsedFont::from_bytes(&font_bytes, 0, &mut warnings)
}

/// Read a raw font table from the original bytes.
fn read_table(font_bytes: &[u8], font_index: usize, table_tag: u32) -> Option<Vec<u8>> {
    let scope = ReadScope::new(font_bytes);
    let font_data = scope.read::<FontData<'_>>().ok()?;
    let provider = font_data.table_provider(font_index).ok()?;
    let data = provider.table_data(table_tag).ok()??;
    Some(data.into_owned())
}

fn hex_dump(data: &[u8]) -> String {
    let mut s = String::new();
    for (i, chunk) in data.chunks(16).enumerate() {
        write!(s, "  {:04x}: ", i * 16).unwrap();
        for b in chunk {
            write!(s, "{:02x} ", b).unwrap();
        }
        s.push('\n');
    }
    s
}

fn hex_dump_short(data: &[u8]) -> String {
    data.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ")
}

#[test]
fn test_phase1_decoding() {
    let font = match load_helvetica_neue() {
        Some(f) => f,
        None => {
            eprintln!("Skipping: HelveticaNeue.ttc not found");
            return;
        }
    };

    let ppem: u16 = 16;
    let upem = font.font_metrics.units_per_em;
    let scale = compute_scale(ppem, upem);

    // ---- Font-level data ----
    let mut font_out = String::new();
    writeln!(font_out, "=== Font-level hinting data for HelveticaNeue.ttc (index 0) ===").unwrap();
    writeln!(font_out, "units_per_em: {}", upem).unwrap();
    writeln!(font_out, "ppem: {}", ppem).unwrap();
    writeln!(font_out, "scale (16.16 fixed): {}", scale).unwrap();
    writeln!(font_out).unwrap();

    // maxp values
    let maxp = &font.maxp_table;
    writeln!(font_out, "=== maxp table ===").unwrap();
    writeln!(font_out, "num_glyphs: {}", maxp.num_glyphs).unwrap();
    if let Some(ref v1) = maxp.version1_sub_table {
        writeln!(font_out, "max_stack_elements: {}", v1.max_stack_elements).unwrap();
        writeln!(font_out, "max_storage: {}", v1.max_storage).unwrap();
        writeln!(font_out, "max_function_defs: {}", v1.max_function_defs).unwrap();
        writeln!(font_out, "max_twilight_points: {}", v1.max_twilight_points).unwrap();
        writeln!(font_out, "max_instruction_defs: {}", v1.max_instruction_defs).unwrap();
        writeln!(font_out, "max_size_of_instructions: {}", v1.max_size_of_instructions).unwrap();
    }
    writeln!(font_out).unwrap();

    // Read raw fpgm and prep tables
    let fpgm_data = read_table(&font.original_bytes, font.original_index, tag::FPGM);
    let prep_data = read_table(&font.original_bytes, font.original_index, tag::PREP);

    writeln!(font_out, "=== fpgm bytecode ({} bytes) ===", fpgm_data.as_ref().map_or(0, |d| d.len())).unwrap();
    if let Some(ref data) = fpgm_data {
        font_out.push_str(&hex_dump(data));
    }
    writeln!(font_out).unwrap();

    writeln!(font_out, "=== prep bytecode ({} bytes) ===", prep_data.as_ref().map_or(0, |d| d.len())).unwrap();
    if let Some(ref data) = prep_data {
        font_out.push_str(&hex_dump(data));
    }
    writeln!(font_out).unwrap();

    // CVT values
    if let Some(ref hint_mutex) = font.hint_instance {
        let mut hint = hint_mutex.lock().unwrap();

        if let Err(e) = hint.set_ppem(ppem, ppem as f64) {
            writeln!(font_out, "set_ppem error: {:?}", e).unwrap();
        }

        let cvt_funits = hint.cvt_funits();
        writeln!(font_out, "=== CVT funits ({} entries) ===", cvt_funits.len()).unwrap();
        for (i, &v) in cvt_funits.iter().enumerate() {
            writeln!(font_out, "  cvt[{}] = {} funits", i, v).unwrap();
        }
        writeln!(font_out).unwrap();

        let cvt_scaled = hint.interpreter.cvt();
        writeln!(font_out, "=== CVT scaled F26Dot6 ({} entries) ===", cvt_scaled.len()).unwrap();
        for (i, &v) in cvt_scaled.iter().enumerate() {
            let pixels = v as f64 / 64.0;
            writeln!(font_out, "  cvt[{}] = {} (F26Dot6) = {:.4} px", i, v, pixels).unwrap();
        }
        writeln!(font_out).unwrap();

        writeln!(font_out, "=== Interpreter state after prep ===").unwrap();
        writeln!(font_out, "max_stack: {}", hint.interpreter.max_stack()).unwrap();
        writeln!(font_out, "storage_len: {}", hint.interpreter.storage_len()).unwrap();
        writeln!(font_out, "fdef_count: {}", hint.interpreter.fdef_count()).unwrap();
        writeln!(font_out, "twilight_point_count: {}", hint.interpreter.twilight_point_count()).unwrap();
        writeln!(font_out, "fpgm_executed: {}", hint.fpgm_executed()).unwrap();
        writeln!(font_out, "prep_bytecode len: {}", hint.prep_bytecode().len()).unwrap();
        writeln!(font_out).unwrap();
    } else {
        writeln!(font_out, "hint_instance: None (no TrueType hinting)").unwrap();
    }

    fs::write("/tmp/font_data_allsorts.txt", &font_out).expect("failed to write font_data_allsorts.txt");
    eprintln!("Wrote /tmp/font_data_allsorts.txt ({} bytes)", font_out.len());

    // ---- Per-glyph decoding data ----
    let glyphs_to_test = [
        ('R', "R"), ('T', "T"), ('f', "f"),
    ];

    let mut decode_out = String::new();

    for (ch, name) in &glyphs_to_test {
        writeln!(decode_out, "========================================").unwrap();
        writeln!(decode_out, "=== Glyph '{}' (U+{:04X}) ===", name, *ch as u32).unwrap();
        writeln!(decode_out, "========================================").unwrap();

        let glyph_id = font.lookup_glyph_index(*ch as u32);
        writeln!(decode_out, "glyph_id: {:?}", glyph_id).unwrap();

        let glyph_id = match glyph_id {
            Some(gid) => gid,
            None => {
                writeln!(decode_out, "ERROR: glyph not found in cmap\n").unwrap();
                continue;
            }
        };

        match font.glyph_records_decoded.get(&glyph_id) {
            Some(owned) => {
                writeln!(decode_out, "horz_advance: {}", owned.horz_advance).unwrap();
                writeln!(decode_out, "bounding_box: min=({}, {}), max=({}, {})",
                    owned.bounding_box.min_x, owned.bounding_box.min_y,
                    owned.bounding_box.max_x, owned.bounding_box.max_y).unwrap();
                writeln!(decode_out).unwrap();

                // raw_points
                writeln!(decode_out, "--- raw_points ---").unwrap();
                match &owned.raw_points {
                    Some(pts) => {
                        writeln!(decode_out, "count: {}", pts.len()).unwrap();
                        for (i, (x, y)) in pts.iter().enumerate() {
                            writeln!(decode_out, "  pt[{}] = ({}, {})", i, x, y).unwrap();
                        }
                    }
                    None => { writeln!(decode_out, "  None").unwrap(); }
                }
                writeln!(decode_out).unwrap();

                // raw_on_curve
                writeln!(decode_out, "--- raw_on_curve ---").unwrap();
                match &owned.raw_on_curve {
                    Some(flags) => {
                        writeln!(decode_out, "count: {}", flags.len()).unwrap();
                        let flags_str: Vec<&str> = flags.iter().map(|&f| if f { "1" } else { "0" }).collect();
                        writeln!(decode_out, "  [{}]", flags_str.join(", ")).unwrap();
                    }
                    None => { writeln!(decode_out, "  None").unwrap(); }
                }
                writeln!(decode_out).unwrap();

                // raw_contour_ends
                writeln!(decode_out, "--- raw_contour_ends ---").unwrap();
                match &owned.raw_contour_ends {
                    Some(ends) => {
                        writeln!(decode_out, "count: {}", ends.len()).unwrap();
                        let ends_str: Vec<String> = ends.iter().map(|e| e.to_string()).collect();
                        writeln!(decode_out, "  [{}]", ends_str.join(", ")).unwrap();
                    }
                    None => { writeln!(decode_out, "  None").unwrap(); }
                }
                writeln!(decode_out).unwrap();

                // instructions
                writeln!(decode_out, "--- instructions ---").unwrap();
                match &owned.instructions {
                    Some(instr) => {
                        writeln!(decode_out, "length: {} bytes", instr.len()).unwrap();
                        writeln!(decode_out, "hex: {}", hex_dump_short(instr)).unwrap();
                    }
                    None => { writeln!(decode_out, "  None").unwrap(); }
                }
                writeln!(decode_out).unwrap();

                // phantom_points
                writeln!(decode_out, "--- phantom_points ---").unwrap();
                match &owned.phantom_points {
                    Some(pts) => {
                        for (i, p) in pts.iter().enumerate() {
                            writeln!(decode_out, "  phantom[{}] = ({}, {})", i, p.0, p.1).unwrap();
                        }
                    }
                    None => { writeln!(decode_out, "  None").unwrap(); }
                }
                writeln!(decode_out).unwrap();

                // Scaled F26Dot6 points (what we'd feed to hint_glyph)
                writeln!(decode_out, "--- Scaled F26Dot6 points (ppem={}) ---", ppem).unwrap();
                if let Some(ref pts) = owned.raw_points {
                    for (i, (x, y)) in pts.iter().enumerate() {
                        let sx = F26Dot6::from_funits(*x as i32, scale);
                        let sy = F26Dot6::from_funits(*y as i32, scale);
                        writeln!(decode_out, "  pt[{}] = funits({}, {}) => F26Dot6({}, {}) = ({:.4}, {:.4}) px",
                            i, x, y, sx.to_bits(), sy.to_bits(),
                            sx.to_bits() as f64 / 64.0, sy.to_bits() as f64 / 64.0).unwrap();
                    }
                }
                writeln!(decode_out).unwrap();
            }
            None => {
                writeln!(decode_out, "ERROR: glyph_id {} not found in glyph_records_decoded\n", glyph_id).unwrap();
            }
        }
    }

    fs::write("/tmp/phase1_decoding_allsorts.txt", &decode_out).expect("failed to write");
    eprintln!("Wrote /tmp/phase1_decoding_allsorts.txt ({} bytes)", decode_out.len());
}

#[test]
fn test_phase2_hinting() {
    let font = match load_helvetica_neue() {
        Some(f) => f,
        None => {
            eprintln!("Skipping: HelveticaNeue.ttc not found");
            return;
        }
    };

    let ppem: u16 = 16;
    let upem = font.font_metrics.units_per_em;
    let scale = compute_scale(ppem, upem);

    let hint_mutex = match &font.hint_instance {
        Some(h) => h,
        None => {
            eprintln!("Skipping: no hint_instance");
            return;
        }
    };

    let glyphs_to_test = [
        ('R', "R"), ('T', "T"), ('f', "f"),
    ];

    let mut hint_out = String::new();
    writeln!(hint_out, "=== Phase 2: Hinting execution (ppem={}, upem={}, scale={}) ===", ppem, upem, scale).unwrap();
    writeln!(hint_out).unwrap();

    for (ch, name) in &glyphs_to_test {
        writeln!(hint_out, "========================================").unwrap();
        writeln!(hint_out, "=== Hinting glyph '{}' (U+{:04X}) ===", name, *ch as u32).unwrap();
        writeln!(hint_out, "========================================").unwrap();

        let glyph_id = match font.lookup_glyph_index(*ch as u32) {
            Some(gid) => gid,
            None => {
                writeln!(hint_out, "ERROR: glyph not found in cmap\n").unwrap();
                continue;
            }
        };

        let owned = match font.glyph_records_decoded.get(&glyph_id) {
            Some(g) => g,
            None => {
                writeln!(hint_out, "ERROR: glyph_id {} not found\n", glyph_id).unwrap();
                continue;
            }
        };

        let raw_points = match &owned.raw_points {
            Some(p) => p,
            None => {
                writeln!(hint_out, "SKIP: no raw_points (composite/CFF)\n").unwrap();
                continue;
            }
        };
        let raw_on_curve = match &owned.raw_on_curve {
            Some(p) => p,
            None => {
                writeln!(hint_out, "SKIP: no raw_on_curve\n").unwrap();
                continue;
            }
        };
        let raw_contour_ends = match &owned.raw_contour_ends {
            Some(p) => p,
            None => {
                writeln!(hint_out, "SKIP: no raw_contour_ends\n").unwrap();
                continue;
            }
        };
        let instructions = owned.instructions.as_deref().unwrap_or(&[]);

        writeln!(hint_out, "glyph_id: {}", glyph_id).unwrap();
        writeln!(hint_out, "num_points: {}", raw_points.len()).unwrap();
        writeln!(hint_out, "num_contours: {}", raw_contour_ends.len()).unwrap();
        writeln!(hint_out, "instruction_bytes: {}", instructions.len()).unwrap();
        writeln!(hint_out, "horz_advance: {}", owned.horz_advance).unwrap();
        writeln!(hint_out).unwrap();

        // Scale points to F26Dot6
        let points_f26dot6: Vec<(i32, i32)> = raw_points.iter().map(|(x, y)| {
            (F26Dot6::from_funits(*x as i32, scale).to_bits(),
             F26Dot6::from_funits(*y as i32, scale).to_bits())
        }).collect();

        let adv_f26dot6 = F26Dot6::from_funits(owned.horz_advance as i32, scale).to_bits();

        writeln!(hint_out, "--- Input points (F26Dot6) ---").unwrap();
        for (i, (x, y)) in points_f26dot6.iter().enumerate() {
            writeln!(hint_out, "  pt[{}] = ({}, {})  = ({:.4}, {:.4}) px",
                i, x, y, *x as f64 / 64.0, *y as f64 / 64.0).unwrap();
        }
        writeln!(hint_out, "  advance_width = {} = {:.4} px", adv_f26dot6, adv_f26dot6 as f64 / 64.0).unwrap();
        writeln!(hint_out).unwrap();

        // Run hinting
        let mut hint = hint_mutex.lock().unwrap();
        if let Err(e) = hint.set_ppem(ppem, ppem as f64) {
            writeln!(hint_out, "set_ppem error: {:?}", e).unwrap();
        }

        writeln!(hint_out, "--- hint_glyph result ---").unwrap();
        let result = hint.hint_glyph(
            &points_f26dot6,
            raw_on_curve,
            raw_contour_ends,
            instructions,
            adv_f26dot6,
        );

        match result {
            Ok(hinted_pts) => {
                writeln!(hint_out, "OK - {} hinted points", hinted_pts.len()).unwrap();
                for (i, (x, y)) in hinted_pts.iter().enumerate() {
                    if i < points_f26dot6.len() {
                        let orig = &points_f26dot6[i];
                        writeln!(hint_out, "  pt[{}]: ({}, {}) => ({}, {})  delta=({}, {})  px=({:.4}, {:.4})",
                            i, orig.0, orig.1, x, y,
                            x - orig.0, y - orig.1,
                            *x as f64 / 64.0, *y as f64 / 64.0).unwrap();
                    } else {
                        // phantom points
                        writeln!(hint_out, "  phantom[{}]: ({}, {})  px=({:.4}, {:.4})",
                            i - points_f26dot6.len(), x, y,
                            *x as f64 / 64.0, *y as f64 / 64.0).unwrap();
                    }
                }
            }
            Err(e) => {
                writeln!(hint_out, "ERROR: {:?}", e).unwrap();
            }
        }
        writeln!(hint_out).unwrap();
    }

    fs::write("/tmp/phase2_hinting_allsorts.txt", &hint_out).expect("failed to write");
    eprintln!("Wrote /tmp/phase2_hinting_allsorts.txt ({} bytes)", hint_out.len());
}

/// Phase 2b: Trace the interpreter step-by-step for glyph 'T' only.
/// This captures the full instruction trace to find where negative point indices originate.
#[test]
fn test_phase2b_trace_t() {
    let font = match load_helvetica_neue() {
        Some(f) => f,
        None => {
            eprintln!("Skipping: HelveticaNeue.ttc not found");
            return;
        }
    };

    let ppem: u16 = 16;
    let upem = font.font_metrics.units_per_em;
    let scale = compute_scale(ppem, upem);

    let hint_mutex = match &font.hint_instance {
        Some(h) => h,
        None => {
            eprintln!("Skipping: no hint_instance");
            return;
        }
    };

    let ch = 'T';
    let glyph_id = font.lookup_glyph_index(ch as u32).expect("T not in cmap");
    let owned = font.glyph_records_decoded.get(&glyph_id).expect("T not decoded");
    let raw_points = owned.raw_points.as_ref().expect("no raw_points");
    let raw_on_curve = owned.raw_on_curve.as_ref().expect("no on_curve");
    let raw_contour_ends = owned.raw_contour_ends.as_ref().expect("no contour_ends");
    let instructions = owned.instructions.as_deref().unwrap_or(&[]);

    let points_f26dot6: Vec<(i32, i32)> = raw_points.iter().map(|(x, y)| {
        (F26Dot6::from_funits(*x as i32, scale).to_bits(),
         F26Dot6::from_funits(*y as i32, scale).to_bits())
    }).collect();
    let adv_f26dot6 = F26Dot6::from_funits(owned.horz_advance as i32, scale).to_bits();

    let mut hint = hint_mutex.lock().unwrap();
    if let Err(e) = hint.set_ppem(ppem, ppem as f64) {
        panic!("set_ppem error: {:?}", e);
    }

    // Enable trace mode
    hint.interpreter.trace_mode = true;

    // Redirect stderr trace to a file by capturing the result
    let result = hint.hint_glyph(
        &points_f26dot6,
        raw_on_curve,
        raw_contour_ends,
        instructions,
        adv_f26dot6,
    );

    hint.interpreter.trace_mode = false;

    match result {
        Ok(pts) => eprintln!("T hinting OK: {} points", pts.len()),
        Err(e) => eprintln!("T hinting ERROR: {:?}", e),
    }
}

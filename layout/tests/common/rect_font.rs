//! In-memory generator for a minimal, valid TrueType (sfnt) font whose every
//! glyph outline is a single filled rectangle.
//!
//! This is a faithful Rust port of `scripts/gen_mock_fonts.py::build_font`
//! (head/hhea/maxp/OS2/post/cmap-format4/glyf/loca/hmtx/name), WITHOUT any
//! GSUB/GPOS/GDEF layout tables. It exists so tests can synthesize a
//! large-cmap "CJK" rectangle font at runtime instead of reading (or
//! committing) a multi-hundred-KiB font file.
//!
//! Every printable codepoint in the (single, contiguous) range gets its own
//! glyph id (`gid = cp - range_start + 1`; gid 0 is `.notdef`) whose outline
//! is one filled box. Nothing about the SHAPE matters; a CPU rasterizer draws
//! each box as a visible coloured rectangle.

#![allow(dead_code)] // shared test helper: not every test uses every function

fn pad4(mut b: Vec<u8>) -> Vec<u8> {
    while b.len() % 4 != 0 {
        b.push(0);
    }
    b
}

fn checksum(b: &[u8]) -> u32 {
    let mut padded = b.to_vec();
    while padded.len() % 4 != 0 {
        padded.push(0);
    }
    let mut total: u32 = 0;
    for chunk in padded.chunks_exact(4) {
        let v = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        total = total.wrapping_add(v);
    }
    total
}

// --- big-endian push helpers -------------------------------------------------
fn pu16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_be_bytes());
}
fn pi16(out: &mut Vec<u8>, v: i16) {
    out.extend_from_slice(&v.to_be_bytes());
}
fn pu32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_be_bytes());
}

/// One closed contour, 4 on-curve points: a filled rectangle. Port of
/// `_glyf_rect`.
fn glyf_rect(xmin: i16, ymin: i16, xmax: i16, ymax: i16) -> Vec<u8> {
    let mut out = Vec::new();
    pi16(&mut out, 1); // numContours
    pi16(&mut out, xmin);
    pi16(&mut out, ymin);
    pi16(&mut out, xmax);
    pi16(&mut out, ymax);
    pu16(&mut out, 3); // endPtsOfContours[0] = 3 (4 points)
    pu16(&mut out, 0); // instructionLength
    out.extend_from_slice(&[0x01, 0x01, 0x01, 0x01]); // on-curve flags
    // x coords as int16 deltas
    pi16(&mut out, xmin);
    pi16(&mut out, xmax - xmin);
    pi16(&mut out, 0);
    pi16(&mut out, xmin - xmax);
    // y coords as int16 deltas
    pi16(&mut out, ymin);
    pi16(&mut out, 0);
    pi16(&mut out, ymax - ymin);
    pi16(&mut out, 0);
    pad4(out)
}

/// Build a minimal rectangle sfnt covering a single contiguous codepoint
/// range. `descent` is the (positive) descent magnitude, matching the Python
/// generator (stored as `-descent` in hhea).
///
/// # Panics
/// Panics if `codepoints` is empty or not a single contiguous ascending run.
pub fn build_rect_font(
    family: &str,
    upem: u16,
    advance: u16,
    ascent: i16,
    descent: i16,
    codepoints: &[u32],
) -> Vec<u8> {
    assert!(!codepoints.is_empty(), "need at least one codepoint");
    let lo = codepoints[0];
    let hi = *codepoints.last().unwrap();
    assert!(
        codepoints.iter().cloned().eq(lo..=hi),
        "generator supports one contiguous ascending range"
    );
    assert!(hi <= 0xFFFF, "cmap format 4 is BMP-only");

    let box_ = (
        50i16,
        0i16,
        advance as i16 - 50,
        700i16,
    );
    let n_glyphs = 1 + codepoints.len();

    // ---- glyf / loca ----
    let mut glyf: Vec<u8> = Vec::new();
    let mut loca: Vec<u32> = vec![0];
    // .notdef: no contours (blank), still advances.
    loca.push(glyf.len() as u32);
    let rect = glyf_rect(box_.0, box_.1, box_.2, box_.3);
    for &cp in codepoints {
        if cp != 0x20 {
            glyf.extend_from_slice(&rect);
        }
        loca.push(glyf.len() as u32);
    }
    let mut loca_tbl = Vec::new();
    for o in &loca {
        pu32(&mut loca_tbl, *o); // long format
    }

    // ---- hmtx: every glyph the same advance ----
    let mut hmtx = Vec::new();
    for _ in 0..n_glyphs {
        pu16(&mut hmtx, advance);
        pi16(&mut hmtx, 0);
    }

    // ---- cmap (format 4, one contiguous segment) ----
    let id_delta: u16 = ((1i32 - lo as i32) & 0xFFFF) as u16; // glyph id of `lo` is 1
    let seg_count: u16 = 2;
    let mut sub = Vec::new();
    pu16(&mut sub, 4); // format
    pu16(&mut sub, 16 + 8 * seg_count); // length
    pu16(&mut sub, 0); // language
    pu16(&mut sub, seg_count * 2); // segCountX2
    pu16(&mut sub, 2); // searchRange
    pu16(&mut sub, 1); // entrySelector
    pu16(&mut sub, 0); // rangeShift
    pu16(&mut sub, hi as u16); // endCode[0]
    pu16(&mut sub, 0xFFFF); // endCode[1]
    pu16(&mut sub, 0); // reservedPad
    pu16(&mut sub, lo as u16); // startCode[0]
    pu16(&mut sub, 0xFFFF); // startCode[1]
    pu16(&mut sub, id_delta); // idDelta[0]
    pu16(&mut sub, 1); // idDelta[1]
    pu16(&mut sub, 0); // idRangeOffset[0]
    pu16(&mut sub, 0); // idRangeOffset[1]

    let mut cmap = Vec::new();
    pu16(&mut cmap, 0); // version
    pu16(&mut cmap, 2); // numTables
    let off: u32 = 4 + 8 * 2;
    pu16(&mut cmap, 3);
    pu16(&mut cmap, 1);
    pu32(&mut cmap, off); // windows BMP
    pu16(&mut cmap, 0);
    pu16(&mut cmap, 3);
    pu32(&mut cmap, off); // unicode BMP
    cmap.extend_from_slice(&sub);

    // ---- name ----
    let subfamily = "Regular";
    let names: [(u16, String); 6] = [
        (1, family.to_string()),
        (2, subfamily.to_string()),
        (3, format!("AzulMock;{family}")),
        (4, format!("{family} {subfamily}")),
        (5, "Version 1.000".to_string()),
        (6, family.replace(' ', "")),
    ];
    // Build entries: (plat, enc, lang, name_id, data)
    let mut entries: Vec<(u16, u16, u16, u16, Vec<u8>)> = Vec::new();
    for (name_id, value) in &names {
        let utf16be: Vec<u8> = value
            .encode_utf16()
            .flat_map(|u| u.to_be_bytes())
            .collect();
        entries.push((3, 1, 0x409, *name_id, utf16be));
        // mac-roman: ASCII-only here, so bytes == ascii bytes.
        entries.push((1, 0, 0, *name_id, value.as_bytes().to_vec()));
    }
    let mut records = Vec::new();
    let mut strings: Vec<u8> = Vec::new();
    for (plat, enc, lang, name_id, data) in &entries {
        pu16(&mut records, *plat);
        pu16(&mut records, *enc);
        pu16(&mut records, *lang);
        pu16(&mut records, *name_id);
        pu16(&mut records, data.len() as u16);
        pu16(&mut records, strings.len() as u16);
        strings.extend_from_slice(data);
    }
    let mut name = Vec::new();
    pu16(&mut name, 0); // format
    pu16(&mut name, entries.len() as u16); // count
    pu16(&mut name, (6 + 12 * entries.len()) as u16); // stringOffset
    name.extend_from_slice(&records);
    name.extend_from_slice(&strings);

    // ---- head ----
    let mut head = Vec::new();
    pu32(&mut head, 0x0001_0000); // version
    pu32(&mut head, 0x0001_0000); // fontRevision
    pu32(&mut head, 0); // checkSumAdjustment (patched later)
    pu32(&mut head, 0x5F0F_3CF5); // magic
    pu16(&mut head, 0b11); // flags
    pu16(&mut head, upem);
    pu32(&mut head, 0); // created (hi)
    pu32(&mut head, 0); // created (lo)
    pu32(&mut head, 0); // modified (hi)
    pu32(&mut head, 0); // modified (lo)
    pi16(&mut head, box_.0); // xMin
    pi16(&mut head, box_.1); // yMin
    pi16(&mut head, box_.2); // xMax
    pi16(&mut head, box_.3); // yMax
    pu16(&mut head, 0); // macStyle: regular
    pu16(&mut head, 8); // lowestRecPPEM
    pi16(&mut head, 2); // fontDirectionHint
    pi16(&mut head, 1); // indexToLocFormat: long
    pi16(&mut head, 0); // glyphDataFormat

    // ---- hhea ----
    let mut hhea = Vec::new();
    pu32(&mut hhea, 0x0001_0000);
    pi16(&mut hhea, ascent);
    pi16(&mut hhea, -descent);
    pi16(&mut hhea, 0); // lineGap
    pu16(&mut hhea, advance); // advanceWidthMax
    pi16(&mut hhea, 0); // minLeftSideBearing
    pi16(&mut hhea, 0); // minRightSideBearing
    pi16(&mut hhea, advance as i16); // xMaxExtent
    pi16(&mut hhea, 1); // caretSlopeRise
    pi16(&mut hhea, 1); // caretSlopeRun  (python passes 1,1,0)
    pi16(&mut hhea, 0); // caretOffset
    pi16(&mut hhea, 0); // reserved
    pi16(&mut hhea, 0);
    pi16(&mut hhea, 0);
    pi16(&mut hhea, 0);
    pi16(&mut hhea, 0); // metricDataFormat
    pu16(&mut hhea, n_glyphs as u16); // numberOfHMetrics

    // ---- maxp ----
    let mut maxp = Vec::new();
    pu32(&mut maxp, 0x0001_0000);
    pu16(&mut maxp, n_glyphs as u16);
    pu16(&mut maxp, 4); // maxPoints
    pu16(&mut maxp, 1); // maxContours
    pu16(&mut maxp, 0);
    pu16(&mut maxp, 0);
    pu16(&mut maxp, 2); // maxZones
    pu16(&mut maxp, 0);
    pu16(&mut maxp, 0);
    pu16(&mut maxp, 0);
    pu16(&mut maxp, 0);
    pu16(&mut maxp, 0);
    pu16(&mut maxp, 0);
    pu16(&mut maxp, 0);
    pu16(&mut maxp, 0);

    // ---- OS/2 (version 4) ----
    let mut os2 = Vec::new();
    pu16(&mut os2, 4); // version
    pi16(&mut os2, advance as i16); // xAvgCharWidth
    pu16(&mut os2, 400); // usWeightClass
    pu16(&mut os2, 5); // usWidthClass
    pu16(&mut os2, 0); // fsType
    let half_adv = (advance / 2) as i16;
    let half_asc = ascent / 2;
    pi16(&mut os2, half_adv); // ySubscriptXSize
    pi16(&mut os2, half_asc); // ySubscriptYSize
    pi16(&mut os2, half_adv); // ySubscriptXOffset
    pi16(&mut os2, half_asc); // ySubscriptYOffset
    pi16(&mut os2, half_adv); // ySuperscriptXSize
    pi16(&mut os2, half_asc); // ySuperscriptYSize
    pi16(&mut os2, 0); // ySuperscriptXOffset
    pi16(&mut os2, half_asc); // ySuperscriptYOffset
    pi16(&mut os2, ascent / 20); // yStrikeoutSize
    pi16(&mut os2, half_asc); // yStrikeoutPosition
    pi16(&mut os2, 0); // sFamilyClass
    os2.extend_from_slice(&[2, 11, 6, 9, 2, 2, 2, 2, 2, 4]); // PANOSE
    pu32(&mut os2, 1); // ulUnicodeRange1
    pu32(&mut os2, 0);
    pu32(&mut os2, 0);
    pu32(&mut os2, 0);
    os2.extend_from_slice(b"AZUL"); // achVendID
    pu16(&mut os2, 0x0040); // fsSelection = REGULAR
    pu16(&mut os2, lo as u16); // usFirstCharIndex
    pu16(&mut os2, hi as u16); // usLastCharIndex
    pi16(&mut os2, ascent); // sTypoAscender
    pi16(&mut os2, -descent); // sTypoDescender
    pi16(&mut os2, 0); // sTypoLineGap
    pi16(&mut os2, ascent); // usWinAscent (stored as signed in python struct)
    pi16(&mut os2, descent); // usWinDescent
    pu32(&mut os2, 0); // ulCodePageRange1
    pu32(&mut os2, 0); // ulCodePageRange2
    pi16(&mut os2, half_asc); // sxHeight
    pi16(&mut os2, (ascent as f32 * 0.9) as i16); // sCapHeight
    pi16(&mut os2, 0); // usDefaultChar
    pu16(&mut os2, 0); // usBreakChar
    pu16(&mut os2, 0); // usMaxContext (python has usDefault..usMaxContext trio)
    pu16(&mut os2, 2);

    // ---- post (format 3.0) ----
    let mut post = Vec::new();
    pu32(&mut post, 0x0003_0000);
    pu32(&mut post, 0); // italicAngle
    pi16(&mut post, 0); // underlinePosition
    pi16(&mut post, 0); // underlineThickness
    pu32(&mut post, 1); // isFixedPitch
    pu32(&mut post, 0);
    pu32(&mut post, 0);
    pu32(&mut post, 0);
    pu32(&mut post, 0);

    // ---- assemble sfnt ----
    let tables: Vec<(&[u8; 4], Vec<u8>)> = {
        let mut v: Vec<(&[u8; 4], Vec<u8>)> = vec![
            (b"OS/2", os2),
            (b"cmap", cmap),
            (b"glyf", glyf),
            (b"head", head),
            (b"hhea", hhea),
            (b"hmtx", hmtx),
            (b"loca", loca_tbl),
            (b"maxp", maxp),
            (b"name", name),
            (b"post", post),
        ];
        v.sort_by(|a, b| a.0.cmp(b.0));
        v
    };
    let num = tables.len() as u16;
    let entry_selector: u16 = if num == 0 { 0 } else { 15 - num.leading_zeros() as u16 };
    let search_range: u16 = (1u16 << entry_selector) * 16;
    let mut font = Vec::new();
    pu32(&mut font, 0x0001_0000); // sfnt version
    pu16(&mut font, num);
    pu16(&mut font, search_range);
    pu16(&mut font, entry_selector);
    pu16(&mut font, num * 16 - search_range);

    let mut offset: u32 = 12 + 16 * num as u32;
    let mut dir_entries = Vec::new();
    let mut body = Vec::new();
    let mut head_table_index = 0usize;
    for (i, (tag, data)) in tables.iter().enumerate() {
        if *tag == b"head" {
            head_table_index = i;
        }
        dir_entries.extend_from_slice(*tag);
        pu32(&mut dir_entries, checksum(data));
        pu32(&mut dir_entries, offset);
        pu32(&mut dir_entries, data.len() as u32);
        let padded = pad4(data.clone());
        offset += padded.len() as u32;
        body.extend_from_slice(&padded);
    }
    font.extend_from_slice(&dir_entries);
    font.extend_from_slice(&body);

    // head.checkSumAdjustment
    let head_off = 12 + 16 * head_table_index;
    let head_data_off = u32::from_be_bytes([
        font[head_off + 8],
        font[head_off + 9],
        font[head_off + 10],
        font[head_off + 11],
    ]) as usize;
    let adj = 0xB1B0_AFBAu32.wrapping_sub(checksum(&font));
    font[head_data_off + 8..head_data_off + 12].copy_from_slice(&adj.to_be_bytes());
    font
}

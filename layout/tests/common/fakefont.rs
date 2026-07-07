//! Deterministic TrueType (sfnt) font builder for tests — pure `std`, no deps.
//!
//! The point of this module is to let text tests run the *real* allsorts +
//! ttf-parser pipeline against fonts whose every metric is known exactly. No
//! system fonts, no fixtures on disk, no floating-point surprises: you declare
//! glyphs with integer FUnit metrics and get back a byte-for-byte reproducible
//! `.ttf` in memory.
//!
//! ## Format choices (and why)
//!
//! * **sfnt version `0x00010000`** — plain TrueType outlines (`glyf`/`loca`),
//!   never CFF. Every mapped glyph is a filled axis-aligned rectangle so the
//!   real rasterizer has something to fill.
//! * **`loca` LONG (`indexToLocFormat = 1`)** — 32-bit byte offsets. Uniform
//!   and unambiguous; no ×2 scaling games, and empty glyphs are encoded by
//!   `loca[i] == loca[i+1]`.
//! * **`cmap` format 4, platform 3 / encoding 1** — the Windows BMP Unicode
//!   subtable every shaper understands. One segment per mapped codepoint plus
//!   the mandatory `0xFFFF` sentinel. All codepoints must be `<= 0xFFFF`
//!   (asserted); astral characters are intentionally out of scope.
//! * **`hhea.numberOfHMetrics == numGlyphs`** — every glyph carries a full
//!   `(advance, lsb)` pair in `hmtx`, so there is no trailing left-side-bearing
//!   array to reason about.
//! * **`maxp` v1, `post` v3, `OS/2` v4, `name` format 0** — the smallest
//!   widely-accepted versions that carry the fields allsorts/ttf-parser read.
//!   `post` v3 means no glyph-name table; `name` is Windows/Unicode UTF-16BE.
//! * **Rectangle winding** — points are emitted `(xMin,yMin) → (xMin,yMax) →
//!   (xMax,yMax) → (xMax,yMin)`, i.e. *clockwise* in y-up FUnit space, which is
//!   the TrueType convention for a filled (non-hole) outer contour. All four
//!   points are on-curve and use 16-bit signed coordinate deltas (the x/y-short
//!   and x/y-same flag bits are deliberately left clear).
//! * **Checksums** — every table checksum is the u32 wrapping sum over the
//!   table zero-padded to a 4-byte multiple. `head.checkSumAdjustment` is
//!   written as `0`, the whole-font wrapping sum is taken, and the field is
//!   patched to `0xB1B0AFBA - sum` per the OpenType spec.
//! * **Table directory** — records are sorted by tag; the physical table data
//!   is laid out in that same sorted order, each table 4-byte aligned. Optional
//!   tables (`kern`, `fpgm`, `prep`, `cvt `) are emitted only when non-empty.
//!
//! ## Ambiguities resolved
//!
//! * The auto-added `.notdef` (gid 0) has `lsb == xMin == upem/10` so its side
//!   bearing is self-consistent. User glyphs may deliberately disagree
//!   (`lsb != xMin`); `hmtx` stores the given `lsb` and `glyf` stores the bbox
//!   `xMin` independently, exactly as declared.
//! * `hhea.minLeftSideBearing` / `minRightSideBearing` / `xMaxExtent` are the
//!   "safe" computed values described in the builder contract: min lsb,
//!   `min(advance - (lsb + width))`, and `max(lsb + width)` respectively, where
//!   `width = xMax - xMin` (`0` for a glyph with no bbox). These are advisory
//!   fields; parsers read but do not fill from them.
//! * `name` ID 6 (PostScript name) is the family with ASCII whitespace
//!   stripped; IDs 1 and 4 keep the family verbatim; ID 2 is `"Regular"`.

#![allow(dead_code)]

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// A single glyph's metrics + optional outline for [`FakeFontBuilder`].
///
/// `advance` and `lsb` are FUnits (design units, i.e. `unitsPerEm`-relative).
/// `bbox` is `Some((xMin, yMin, xMax, yMax))` for a filled rectangle glyph, or
/// `None` for an empty (zero-contour) glyph such as a space. `instructions`
/// are raw TrueType bytecode bytes for the glyph program (usually empty).
#[derive(Clone, Debug, Default)]
pub struct FakeGlyph {
    pub advance: u16,
    pub lsb: i16,
    pub bbox: Option<(i16, i16, i16, i16)>,
    pub instructions: Vec<u8>,
}

/// A single glyph slot inside the builder (public metrics + its cmap char).
struct GlyphEntry {
    ch: Option<char>,
    advance: u16,
    lsb: i16,
    bbox: Option<(i16, i16, i16, i16)>,
    instructions: Vec<u8>,
}

/// Builder that emits a complete, self-consistent `.ttf` byte vector.
pub struct FakeFontBuilder {
    family_name: String,
    upem: u16,
    ascent: i16,
    descent: i16,
    line_gap: i16,
    glyphs: Vec<GlyphEntry>,
    kern_pairs: Vec<(u16, u16, i16)>,
    fpgm: Vec<u8>,
    prep: Vec<u8>,
    cvt: Vec<i16>,
}

impl FakeFontBuilder {
    /// Start a new font with the given family name and units-per-em.
    ///
    /// Automatically inserts glyph 0 (`.notdef`): advance `upem/2`, bounding box
    /// `(upem/10, 0, upem/2, upem*7/10)`, `lsb == xMin == upem/10`, no cmap entry.
    pub fn new(family_name: &str, upem: u16) -> Self {
        let upem_i = upem as i32;
        let notdef = GlyphEntry {
            ch: None,
            advance: upem / 2,
            lsb: (upem_i / 10) as i16,
            bbox: Some((
                (upem_i / 10) as i16,
                0,
                (upem_i / 2) as i16,
                (upem_i * 7 / 10) as i16,
            )),
            instructions: Vec::new(),
        };
        FakeFontBuilder {
            family_name: family_name.to_string(),
            upem,
            // Sensible defaults; overridden by `metrics`.
            ascent: (upem_i * 800 / 1000) as i16,
            descent: -((upem_i * 200 / 1000) as i16),
            line_gap: 0,
            glyphs: vec![notdef],
            kern_pairs: Vec::new(),
            fpgm: Vec::new(),
            prep: Vec::new(),
            cvt: Vec::new(),
        }
    }

    /// Set the vertical metrics (FUnits). `descent` is conventionally NEGATIVE.
    pub fn metrics(mut self, ascent: i16, descent: i16, line_gap: i16) -> Self {
        self.ascent = ascent;
        self.descent = descent;
        self.line_gap = line_gap;
        self
    }

    /// Append a glyph, returning its glyph id (the insertion index).
    ///
    /// `ch = None` adds the glyph without a `cmap` entry (unmapped).
    pub fn add_glyph(&mut self, ch: Option<char>, g: FakeGlyph) -> u16 {
        let gid = self.glyphs.len() as u16;
        self.glyphs.push(GlyphEntry {
            ch,
            advance: g.advance,
            lsb: g.lsb,
            bbox: g.bbox,
            instructions: g.instructions,
        });
        gid
    }

    /// Add a `kern` format-0 pair (FUnits, negative = tighter).
    pub fn add_kern(&mut self, left_gid: u16, right_gid: u16, value: i16) {
        self.kern_pairs.push((left_gid, right_gid, value));
    }

    /// Set the `fpgm` (font program) bytecode. Emitted only when non-empty.
    pub fn set_fpgm(&mut self, b: Vec<u8>) {
        self.fpgm = b;
    }

    /// Set the `prep` (control-value program) bytecode. Emitted only when non-empty.
    pub fn set_prep(&mut self, b: Vec<u8>) {
        self.prep = b;
    }

    /// Set the `cvt ` (control value table) entries. Emitted only when non-empty.
    pub fn set_cvt(&mut self, v: Vec<i16>) {
        self.cvt = v;
    }

    /// Serialize the whole font to a `.ttf` byte vector.
    pub fn build(&self) -> Vec<u8> {
        let gbbox = self.global_bbox();
        let (glyf, loca) = self.build_glyf_loca();

        // Collect (tag, data) for every present table.
        let mut tables: Vec<([u8; 4], Vec<u8>)> = vec![
            (*b"head", self.build_head(gbbox)),
            (*b"hhea", self.build_hhea()),
            (*b"maxp", self.build_maxp()),
            (*b"hmtx", self.build_hmtx()),
            (*b"cmap", self.build_cmap()),
            (*b"glyf", glyf),
            (*b"loca", loca),
            (*b"post", self.build_post()),
            (*b"OS/2", self.build_os2()),
            (*b"name", self.build_name()),
        ];
        if !self.kern_pairs.is_empty() {
            tables.push((*b"kern", self.build_kern()));
        }
        if !self.fpgm.is_empty() {
            tables.push((*b"fpgm", self.fpgm.clone()));
        }
        if !self.prep.is_empty() {
            tables.push((*b"prep", self.prep.clone()));
        }
        if !self.cvt.is_empty() {
            tables.push((*b"cvt ", self.build_cvt()));
        }

        // Directory records are sorted by tag; we lay the data out in the same
        // order for reproducibility.
        tables.sort_by(|a, b| a.0.cmp(&b.0));

        let num_tables = tables.len();
        let offset_table_size = 12 + 16 * num_tables;

        // Physically lay out tables (4-byte aligned) and record checksums.
        let mut records: Vec<([u8; 4], u32, u32, u32)> = Vec::new(); // tag, checksum, offset, length
        let mut body: Vec<u8> = Vec::new();
        let mut offset = offset_table_size;
        for (tag, data) in &tables {
            let length = data.len() as u32;
            let mut padded = data.clone();
            pad4(&mut padded);
            let checksum = table_checksum(&padded);
            records.push((*tag, checksum, offset as u32, length));
            body.extend_from_slice(&padded);
            offset += padded.len();
        }

        // Offset table (sfnt header + table directory).
        let mut font: Vec<u8> = Vec::with_capacity(offset_table_size + body.len());
        push_u32(&mut font, 0x0001_0000); // sfnt version
        push_u16(&mut font, num_tables as u16);
        let p2 = pow2_floor(num_tables as u32);
        push_u16(&mut font, (16 * p2) as u16); // searchRange
        push_u16(&mut font, floor_log2(num_tables as u32) as u16); // entrySelector
        push_u16(&mut font, (16 * num_tables as u32 - 16 * p2) as u16); // rangeShift
        for (tag, checksum, off, length) in &records {
            font.extend_from_slice(tag);
            push_u32(&mut font, *checksum);
            push_u32(&mut font, *off);
            push_u32(&mut font, *length);
        }
        font.extend_from_slice(&body);

        // head.checkSumAdjustment = 0xB1B0AFBA - (whole-font wrapping sum).
        let font_sum = table_checksum(&font);
        let adjustment = 0xB1B0_AFBAu32.wrapping_sub(font_sum);
        let head_off = records
            .iter()
            .find(|(t, _, _, _)| t == b"head")
            .map(|(_, _, o, _)| *o)
            .expect("head table always present") as usize;
        font[head_off + 8..head_off + 12].copy_from_slice(&adjustment.to_be_bytes());

        font
    }

    // -- individual table builders ------------------------------------------

    /// Global bounding box (min/max over every glyph that has one).
    fn global_bbox(&self) -> (i16, i16, i16, i16) {
        let mut it = self.glyphs.iter().filter_map(|g| g.bbox);
        let first = it.next().unwrap_or((0, 0, 0, 0));
        let (mut xmin, mut ymin, mut xmax, mut ymax) = first;
        for (a, b, c, d) in it {
            xmin = xmin.min(a);
            ymin = ymin.min(b);
            xmax = xmax.max(c);
            ymax = ymax.max(d);
        }
        (xmin, ymin, xmax, ymax)
    }

    fn build_head(&self, gbbox: (i16, i16, i16, i16)) -> Vec<u8> {
        let (xmin, ymin, xmax, ymax) = gbbox;
        let mut b = Vec::new();
        push_u32(&mut b, 0x0001_0000); // version 1.0
        push_u32(&mut b, 0x0001_0000); // fontRevision 1.0
        push_u32(&mut b, 0); // checkSumAdjustment (patched in build())
        push_u32(&mut b, 0x5F0F_3CF5); // magicNumber
        push_u16(&mut b, 0x0003); // flags
        push_u16(&mut b, self.upem); // unitsPerEm
        push_i64(&mut b, 3_000_000_000); // created
        push_i64(&mut b, 3_000_000_000); // modified
        push_i16(&mut b, xmin);
        push_i16(&mut b, ymin);
        push_i16(&mut b, xmax);
        push_i16(&mut b, ymax);
        push_u16(&mut b, 0); // macStyle
        push_u16(&mut b, 6); // lowestRecPPEM
        push_i16(&mut b, 2); // fontDirectionHint
        push_i16(&mut b, 1); // indexToLocFormat = LONG
        push_i16(&mut b, 0); // glyphDataFormat
        b
    }

    fn build_hhea(&self) -> Vec<u8> {
        let num_glyphs = self.glyphs.len() as u16;
        let advance_width_max = self.glyphs.iter().map(|g| g.advance).max().unwrap_or(0);
        let min_lsb = self.glyphs.iter().map(|g| g.lsb).min().unwrap_or(0);

        let mut min_rsb = i32::MAX;
        let mut x_max_extent = i32::MIN;
        for g in &self.glyphs {
            let width = match g.bbox {
                Some((xmin, _, xmax, _)) => xmax as i32 - xmin as i32,
                None => 0,
            };
            let rsb = g.advance as i32 - (g.lsb as i32 + width);
            let ext = g.lsb as i32 + width;
            min_rsb = min_rsb.min(rsb);
            x_max_extent = x_max_extent.max(ext);
        }

        let mut b = Vec::new();
        push_u32(&mut b, 0x0001_0000); // version 1.0
        push_i16(&mut b, self.ascent);
        push_i16(&mut b, self.descent);
        push_i16(&mut b, self.line_gap);
        push_u16(&mut b, advance_width_max);
        push_i16(&mut b, min_lsb);
        push_i16(&mut b, min_rsb as i16);
        push_i16(&mut b, x_max_extent as i16);
        push_i16(&mut b, 1); // caretSlopeRise
        push_i16(&mut b, 0); // caretSlopeRun
        push_i16(&mut b, 0); // caretOffset
        push_i16(&mut b, 0); // reserved
        push_i16(&mut b, 0);
        push_i16(&mut b, 0);
        push_i16(&mut b, 0);
        push_i16(&mut b, 0); // metricDataFormat
        push_u16(&mut b, num_glyphs); // numberOfHMetrics
        b
    }

    fn build_maxp(&self) -> Vec<u8> {
        let num_glyphs = self.glyphs.len() as u16;
        let mut max_instr = 64u16;
        for g in &self.glyphs {
            max_instr = max_instr.max(g.instructions.len() as u16);
        }
        max_instr = max_instr
            .max(self.fpgm.len() as u16)
            .max(self.prep.len() as u16);

        let mut b = Vec::new();
        push_u32(&mut b, 0x0001_0000); // version 1.0
        push_u16(&mut b, num_glyphs);
        push_u16(&mut b, 4); // maxPoints
        push_u16(&mut b, 1); // maxContours
        push_u16(&mut b, 0); // maxCompositePoints
        push_u16(&mut b, 0); // maxCompositeContours
        push_u16(&mut b, 2); // maxZones
        push_u16(&mut b, 64); // maxTwilightPoints
        push_u16(&mut b, 64); // maxStorage
        push_u16(&mut b, 64); // maxFunctionDefs
        push_u16(&mut b, 0); // maxInstructionDefs
        push_u16(&mut b, 512); // maxStackElements
        push_u16(&mut b, max_instr); // maxSizeOfInstructions
        push_u16(&mut b, 0); // maxComponentElements
        push_u16(&mut b, 0); // maxComponentDepth
        b
    }

    fn build_hmtx(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(self.glyphs.len() * 4);
        for g in &self.glyphs {
            push_u16(&mut b, g.advance);
            push_i16(&mut b, g.lsb);
        }
        b
    }

    fn build_cmap(&self) -> Vec<u8> {
        // (codepoint, gid) for every mapped glyph, sorted by codepoint.
        let mut mappings: Vec<(u32, u16)> = Vec::new();
        for (gid, g) in self.glyphs.iter().enumerate() {
            if let Some(c) = g.ch {
                let cp = c as u32;
                assert!(
                    cp <= 0xFFFF,
                    "FakeFont cmap format 4 requires codepoint <= 0xFFFF (got U+{cp:04X})"
                );
                mappings.push((cp, gid as u16));
            }
        }
        mappings.sort_by_key(|&(cp, _)| cp);

        // One segment per mapping, plus the mandatory 0xFFFF terminator segment.
        let seg_count = mappings.len() + 1;
        let p2 = pow2_floor(seg_count as u32);

        let mut sub = Vec::new();
        push_u16(&mut sub, 4); // format
        push_u16(&mut sub, (16 + 8 * seg_count) as u16); // length
        push_u16(&mut sub, 0); // language
        push_u16(&mut sub, (2 * seg_count) as u16); // segCountX2
        push_u16(&mut sub, (2 * p2) as u16); // searchRange
        push_u16(&mut sub, floor_log2(seg_count as u32) as u16); // entrySelector
        push_u16(&mut sub, (2 * seg_count as u32 - 2 * p2) as u16); // rangeShift

        // endCode[]
        for &(cp, _) in &mappings {
            push_u16(&mut sub, cp as u16);
        }
        push_u16(&mut sub, 0xFFFF);
        // reservedPad
        push_u16(&mut sub, 0);
        // startCode[]
        for &(cp, _) in &mappings {
            push_u16(&mut sub, cp as u16);
        }
        push_u16(&mut sub, 0xFFFF);
        // idDelta[]
        for &(cp, gid) in &mappings {
            let delta = ((gid as i32 - cp as i32) & 0xFFFF) as u16;
            push_u16(&mut sub, delta);
        }
        push_u16(&mut sub, 1);
        // idRangeOffset[]
        for _ in &mappings {
            push_u16(&mut sub, 0);
        }
        push_u16(&mut sub, 0);

        // cmap header: one encoding record pointing at the format-4 subtable.
        let mut b = Vec::new();
        push_u16(&mut b, 0); // version
        push_u16(&mut b, 1); // numTables
        push_u16(&mut b, 3); // platformID (Windows)
        push_u16(&mut b, 1); // encodingID (Unicode BMP)
        push_u32(&mut b, 12); // offset to subtable
        b.extend_from_slice(&sub);
        b
    }

    fn build_glyf_loca(&self) -> (Vec<u8>, Vec<u8>) {
        let mut glyf: Vec<u8> = Vec::new();
        let mut offsets: Vec<u32> = Vec::with_capacity(self.glyphs.len() + 1);

        for g in &self.glyphs {
            offsets.push(glyf.len() as u32);
            if let Some((xmin, ymin, xmax, ymax)) = g.bbox {
                let mut gb: Vec<u8> = Vec::new();
                push_i16(&mut gb, 1); // numberOfContours
                push_i16(&mut gb, xmin);
                push_i16(&mut gb, ymin);
                push_i16(&mut gb, xmax);
                push_i16(&mut gb, ymax);
                push_u16(&mut gb, 3); // endPtsOfContours[0] (4 points, last index = 3)
                push_u16(&mut gb, g.instructions.len() as u16); // instructionLength
                gb.extend_from_slice(&g.instructions);
                // 4 on-curve points (flag 0x01: ON_CURVE, no short/same/repeat bits).
                gb.push(0x01);
                gb.push(0x01);
                gb.push(0x01);
                gb.push(0x01);
                // x deltas (from previous point, first from 0), clockwise for y-up:
                //   (xMin,yMin) -> (xMin,yMax) -> (xMax,yMax) -> (xMax,yMin)
                push_i16(&mut gb, xmin);
                push_i16(&mut gb, 0);
                push_i16(&mut gb, (xmax as i32 - xmin as i32) as i16);
                push_i16(&mut gb, 0);
                // y deltas
                push_i16(&mut gb, ymin);
                push_i16(&mut gb, (ymax as i32 - ymin as i32) as i16);
                push_i16(&mut gb, 0);
                push_i16(&mut gb, (ymin as i32 - ymax as i32) as i16);
                pad4(&mut gb);
                glyf.extend_from_slice(&gb);
            }
            // No bbox => empty glyph => loca[i] == loca[i+1] (nothing appended).
        }
        offsets.push(glyf.len() as u32);

        let mut loca = Vec::with_capacity(offsets.len() * 4);
        for o in &offsets {
            push_u32(&mut loca, *o);
        }
        (glyf, loca)
    }

    fn build_post(&self) -> Vec<u8> {
        let mut b = Vec::new();
        push_u32(&mut b, 0x0003_0000); // version 3.0 (no glyph names)
        push_i32(&mut b, 0); // italicAngle
        push_i16(&mut b, -100); // underlinePosition
        push_i16(&mut b, 50); // underlineThickness
        push_u32(&mut b, 0); // isFixedPitch
        push_u32(&mut b, 0); // minMemType42
        push_u32(&mut b, 0); // maxMemType42
        push_u32(&mut b, 0); // minMemType1
        push_u32(&mut b, 0); // maxMemType1
        b
    }

    fn build_os2(&self) -> Vec<u8> {
        let mapped: Vec<u32> = self
            .glyphs
            .iter()
            .filter_map(|g| g.ch.map(|c| c as u32))
            .collect();
        let first_char = mapped.iter().copied().min().unwrap_or(0) as u16;
        let last_char = mapped
            .iter()
            .copied()
            .max()
            .map(|m| m.min(0xFFFF))
            .unwrap_or(0) as u16;

        let mut b = Vec::new();
        push_u16(&mut b, 4); // version
        push_i16(&mut b, 500); // xAvgCharWidth
        push_u16(&mut b, 400); // usWeightClass
        push_u16(&mut b, 5); // usWidthClass
        push_u16(&mut b, 0); // fsType
        push_i16(&mut b, 650); // ySubscriptXSize
        push_i16(&mut b, 600); // ySubscriptYSize
        push_i16(&mut b, 0); // ySubscriptXOffset
        push_i16(&mut b, 75); // ySubscriptYOffset
        push_i16(&mut b, 650); // ySuperscriptXSize
        push_i16(&mut b, 600); // ySuperscriptYSize
        push_i16(&mut b, 0); // ySuperscriptXOffset
        push_i16(&mut b, 350); // ySuperscriptYOffset
        push_i16(&mut b, 50); // yStrikeoutSize
        push_i16(&mut b, 300); // yStrikeoutPosition
        push_i16(&mut b, 0); // sFamilyClass
        b.extend_from_slice(&[2, 0, 5, 3, 0, 0, 0, 0, 0, 0]); // panose
        push_u32(&mut b, 1); // ulUnicodeRange1
        push_u32(&mut b, 0); // ulUnicodeRange2
        push_u32(&mut b, 0); // ulUnicodeRange3
        push_u32(&mut b, 0); // ulUnicodeRange4
        b.extend_from_slice(b"AZUL"); // achVendID
        push_u16(&mut b, 0x0040); // fsSelection (REGULAR)
        push_u16(&mut b, first_char); // usFirstCharIndex
        push_u16(&mut b, last_char); // usLastCharIndex
        push_i16(&mut b, self.ascent); // sTypoAscender
        push_i16(&mut b, self.descent); // sTypoDescender
        push_i16(&mut b, self.line_gap); // sTypoLineGap
        push_u16(&mut b, self.ascent as u16); // usWinAscent
        push_u16(&mut b, (-(self.descent as i32)) as u16); // usWinDescent
        push_u32(&mut b, 1); // ulCodePageRange1
        push_u32(&mut b, 0); // ulCodePageRange2
        push_i16(&mut b, 500); // sxHeight
        push_i16(&mut b, 700); // sCapHeight
        push_u16(&mut b, 0); // usDefaultChar
        push_u16(&mut b, 32); // usBreakChar
        push_u16(&mut b, 2); // usMaxContext
        b
    }

    fn build_name(&self) -> Vec<u8> {
        let family = self.family_name.clone();
        let family_no_spaces: String = family.chars().filter(|c| !c.is_whitespace()).collect();

        // (nameID, string) — MUST stay sorted by nameID.
        let records: [(u16, &str); 4] = [
            (1, &family),           // Font Family
            (2, "Regular"),         // Font Subfamily
            (4, &family),           // Full name
            (6, &family_no_spaces), // PostScript name (no spaces)
        ];

        // Build UTF-16BE storage and per-record (offset, length).
        let mut storage = Vec::new();
        let mut recs: Vec<(u16, u16, u16)> = Vec::new(); // (nameID, offset, byte-length)
        for &(id, s) in records.iter() {
            let offset = storage.len() as u16;
            for u in s.encode_utf16() {
                push_u16(&mut storage, u);
            }
            let len = storage.len() as u16 - offset;
            recs.push((id, offset, len));
        }

        let count = recs.len() as u16;
        let string_offset = 6 + count * 12;

        let mut b = Vec::new();
        push_u16(&mut b, 0); // format 0
        push_u16(&mut b, count);
        push_u16(&mut b, string_offset);
        for &(id, offset, len) in &recs {
            push_u16(&mut b, 3); // platformID (Windows)
            push_u16(&mut b, 1); // encodingID (Unicode BMP)
            push_u16(&mut b, 0x0409); // languageID (en-US)
            push_u16(&mut b, id);
            push_u16(&mut b, len);
            push_u16(&mut b, offset);
        }
        b.extend_from_slice(&storage);
        b
    }

    fn build_kern(&self) -> Vec<u8> {
        let mut pairs = self.kern_pairs.clone();
        pairs.sort_by_key(|&(l, r, _)| ((l as u32) << 16) | (r as u32));

        let n_pairs = pairs.len() as u32;
        let p2 = pow2_floor(n_pairs);
        let search_range = 6 * p2;
        let entry_selector = floor_log2(n_pairs);
        let range_shift = 6 * n_pairs - search_range;
        let subtable_len = 14 + 6 * n_pairs;

        let mut b = Vec::new();
        push_u16(&mut b, 0); // table version
        push_u16(&mut b, 1); // nTables
        // subtable
        push_u16(&mut b, 0); // subtable version
        push_u16(&mut b, subtable_len as u16); // length (incl. this header)
        push_u16(&mut b, 0x0001); // coverage: horizontal, format 0
        push_u16(&mut b, n_pairs as u16); // nPairs
        push_u16(&mut b, search_range as u16);
        push_u16(&mut b, entry_selector as u16);
        push_u16(&mut b, range_shift as u16);
        for &(l, r, v) in &pairs {
            push_u16(&mut b, l);
            push_u16(&mut b, r);
            push_i16(&mut b, v);
        }
        b
    }

    fn build_cvt(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(self.cvt.len() * 2);
        for v in &self.cvt {
            push_i16(&mut b, *v);
        }
        b
    }
}

// ---------------------------------------------------------------------------
// Ready-made font
// ---------------------------------------------------------------------------

/// The family name used by [`simple_test_font`].
pub const FAKE_FAMILY: &str = "FakeTest";

/// A 1000-upem test font with Latin, digits, punctuation, CJK, Hebrew, a
/// combining mark, and an `A`/`V` kern pair. See the glyph-id table in the
/// module tests / builder docs.
///
/// Glyph ids (insertion order):
/// * `0` — `.notdef`
/// * `1..=26` — `'a'..='z'`
/// * `27..=52` — `'A'..='Z'`   (`'A'` = 27, `'V'` = 48)
/// * `53..=62` — `'0'..='9'`
/// * `63` — `' '`, `64` — U+00A0 NBSP
/// * `65` — `'-'`, `66` — `'.'`, `67` — `','`
/// * `68..=71` — `你 好 世 界`
/// * `72..=74` — `א ב ג`
/// * `75` — U+0301 combining acute accent
/// * `76` — `'/'` solidus (300u => 6px)
/// * `77` — `'@'` wide at-sign (900u => 18px)
/// * `78` — U+200B ZERO WIDTH SPACE (0 advance, empty)
/// * `79` — U+00AD SOFT HYPHEN (0 advance, empty)
/// * `80` — `'\t'` TAB (250u => 5px, empty)
pub fn simple_test_font() -> Vec<u8> {
    let mut b = FakeFontBuilder::new(FAKE_FAMILY, 1000).metrics(800, -200, 0);

    // 'a'..='z'
    for c in 'a'..='z' {
        b.add_glyph(
            Some(c),
            FakeGlyph {
                advance: 600,
                lsb: 50,
                bbox: Some((50, 0, 550, 700)),
                instructions: Vec::new(),
            },
        );
    }

    // 'A'..='Z' (capture the ids for 'A' and 'V' to build the kern pair)
    let mut gid_a = 0u16;
    let mut gid_v = 0u16;
    for c in 'A'..='Z' {
        let gid = b.add_glyph(
            Some(c),
            FakeGlyph {
                advance: 700,
                lsb: 50,
                bbox: Some((50, 0, 650, 700)),
                instructions: Vec::new(),
            },
        );
        if c == 'A' {
            gid_a = gid;
        }
        if c == 'V' {
            gid_v = gid;
        }
    }

    // '0'..='9'
    for c in '0'..='9' {
        b.add_glyph(
            Some(c),
            FakeGlyph {
                advance: 500,
                lsb: 50,
                bbox: Some((50, 0, 450, 700)),
                instructions: Vec::new(),
            },
        );
    }

    // space + NBSP (empty glyphs)
    b.add_glyph(
        Some(' '),
        FakeGlyph {
            advance: 250,
            lsb: 0,
            bbox: None,
            instructions: Vec::new(),
        },
    );
    b.add_glyph(
        Some('\u{00A0}'),
        FakeGlyph {
            advance: 250,
            lsb: 0,
            bbox: None,
            instructions: Vec::new(),
        },
    );

    // hyphen
    b.add_glyph(
        Some('-'),
        FakeGlyph {
            advance: 300,
            lsb: 50,
            bbox: Some((50, 250, 250, 350)),
            instructions: Vec::new(),
        },
    );

    // period + comma (identical metrics)
    for c in &['.', ','] {
        b.add_glyph(
            Some(*c),
            FakeGlyph {
                advance: 250,
                lsb: 50,
                bbox: Some((75, 0, 175, 150)),
                instructions: Vec::new(),
            },
        );
    }

    // CJK: 你 好 世 界 (full-width, tall)
    for &c in &['\u{4F60}', '\u{597D}', '\u{4E16}', '\u{754C}'] {
        b.add_glyph(
            Some(c),
            FakeGlyph {
                advance: 1000,
                lsb: 0,
                bbox: Some((50, -100, 950, 800)),
                instructions: Vec::new(),
            },
        );
    }

    // Hebrew: א ב ג
    for &c in &['\u{05D0}', '\u{05D1}', '\u{05D2}'] {
        b.add_glyph(
            Some(c),
            FakeGlyph {
                advance: 550,
                lsb: 50,
                bbox: Some((50, 0, 500, 600)),
                instructions: Vec::new(),
            },
        );
    }

    // U+0301 combining acute accent: zero advance, sits above and to the left.
    b.add_glyph(
        Some('\u{0301}'),
        FakeGlyph {
            advance: 0,
            lsb: 0,
            bbox: Some((-300, 720, -100, 780)),
            instructions: Vec::new(),
        },
    );

    // '/' solidus (300u => 6px). A visible break-after candidate (UAX#14 class SY).
    b.add_glyph(
        Some('/'),
        FakeGlyph {
            advance: 300,
            lsb: 50,
            bbox: Some((100, 0, 200, 700)),
            instructions: Vec::new(),
        },
    );

    // '@' wide at-sign (900u => 18px) — a broad glyph distinct from every other width.
    b.add_glyph(
        Some('@'),
        FakeGlyph {
            advance: 900,
            lsb: 50,
            bbox: Some((50, -100, 850, 700)),
            instructions: Vec::new(),
        },
    );

    // U+200B ZERO WIDTH SPACE: zero advance, empty. An explicit soft-wrap opportunity
    // (UAX#14 class ZW) that is honored even under word-break: keep-all.
    b.add_glyph(
        Some('\u{200B}'),
        FakeGlyph {
            advance: 0,
            lsb: 0,
            bbox: None,
            instructions: Vec::new(),
        },
    );

    // U+00AD SOFT HYPHEN: zero advance, empty. A conditional (manual) break opportunity
    // (UAX#14 class BA) active only when the `hyphens` property is not `none`.
    b.add_glyph(
        Some('\u{00AD}'),
        FakeGlyph {
            advance: 0,
            lsb: 0,
            bbox: None,
            instructions: Vec::new(),
        },
    );

    // '\t' TAB (250u => 5px, empty). The text3 model routes real tab stops through
    // `InlineContent::Tab`; this cmap entry just lets a literal tab shape without
    // falling back to `.notdef`.
    b.add_glyph(
        Some('\t'),
        FakeGlyph {
            advance: 250,
            lsb: 0,
            bbox: None,
            instructions: Vec::new(),
        },
    );

    // Kern pairs A/V and V/A (tighter).
    b.add_kern(gid_a, gid_v, -100);
    b.add_kern(gid_v, gid_a, -100);

    b.build()
}

/// The family name used by [`simple_fallback_font`].
pub const FAKE_FALLBACK_FAMILY: &str = "FakeFallback";

/// A second, independent 1000-upem test font whose cmap is DISJOINT from
/// [`simple_test_font`]: it maps Greek `α β γ δ` and `'#'`, none of which the
/// primary font covers. Every mapped glyph advances 800u (=> 16px at font-size
/// 20), a width shared by no glyph in the primary font, so a fallback glyph is
/// unmistakable in the measured output.
///
/// Glyph ids: `0` .notdef, `1..=4` `α β γ δ` (U+03B1..=U+03B4), `5` `'#'`.
pub fn simple_fallback_font() -> Vec<u8> {
    let mut b = FakeFontBuilder::new(FAKE_FALLBACK_FAMILY, 1000).metrics(800, -200, 0);

    for &c in &['\u{03B1}', '\u{03B2}', '\u{03B3}', '\u{03B4}'] {
        b.add_glyph(
            Some(c),
            FakeGlyph {
                advance: 800,
                lsb: 50,
                bbox: Some((50, 0, 750, 700)),
                instructions: Vec::new(),
            },
        );
    }

    b.add_glyph(
        Some('#'),
        FakeGlyph {
            advance: 800,
            lsb: 50,
            bbox: Some((50, 0, 750, 700)),
            instructions: Vec::new(),
        },
    );

    b.build()
}

// ---------------------------------------------------------------------------
// Byte helpers (all big-endian)
// ---------------------------------------------------------------------------

#[inline]
fn push_u16(b: &mut Vec<u8>, v: u16) {
    b.extend_from_slice(&v.to_be_bytes());
}

#[inline]
fn push_i16(b: &mut Vec<u8>, v: i16) {
    b.extend_from_slice(&v.to_be_bytes());
}

#[inline]
fn push_u32(b: &mut Vec<u8>, v: u32) {
    b.extend_from_slice(&v.to_be_bytes());
}

#[inline]
fn push_i32(b: &mut Vec<u8>, v: i32) {
    b.extend_from_slice(&v.to_be_bytes());
}

#[inline]
fn push_i64(b: &mut Vec<u8>, v: i64) {
    b.extend_from_slice(&v.to_be_bytes());
}

/// Zero-pad `v` up to the next multiple of 4 bytes.
fn pad4(v: &mut Vec<u8>) {
    while v.len() % 4 != 0 {
        v.push(0);
    }
}

/// `floor(log2(n))` for `n >= 1`.
#[inline]
fn floor_log2(n: u32) -> u32 {
    debug_assert!(n >= 1);
    31 - n.leading_zeros()
}

/// Largest power of two `<= n` for `n >= 1`.
#[inline]
fn pow2_floor(n: u32) -> u32 {
    1u32 << floor_log2(n)
}

/// OpenType table checksum: u32 wrapping sum over the data, zero-padded to a
/// multiple of 4 bytes.
fn table_checksum(data: &[u8]) -> u32 {
    let mut sum: u32 = 0;
    let mut i = 0;
    while i < data.len() {
        let mut word = [0u8; 4];
        let take = core::cmp::min(4, data.len() - i);
        word[..take].copy_from_slice(&data[i..i + take]);
        sum = sum.wrapping_add(u32::from_be_bytes(word));
        i += 4;
    }
    sum
}

// ---------------------------------------------------------------------------
// Self-checks
// ---------------------------------------------------------------------------

#[cfg(test)]
mod selfcheck {
    use super::*;

    #[test]
    fn builds_nonempty_and_aligned() {
        let font = simple_test_font();
        assert!(font.len() > 4);
        assert_eq!(font.len() % 4, 0, "font length must be 4-byte aligned");
        // sfnt version 0x00010000
        assert_eq!(&font[0..4], &[0x00, 0x01, 0x00, 0x00]);
    }

    #[test]
    fn whole_font_checksum_is_magic() {
        // After patching checkSumAdjustment the whole-font wrapping sum must
        // equal 0xB1B0AFBA.
        let font = simple_test_font();
        assert_eq!(table_checksum(&font), 0xB1B0_AFBA);
    }

    #[test]
    fn expected_glyph_count() {
        // .notdef + 26 + 26 + 10 + space + nbsp + '-' + '.' + ',' + 4 CJK
        // + 3 Hebrew + combining + '/' + '@' + ZWSP + SHY + TAB = 81 glyphs.
        let b = FakeFontBuilder::new(FAKE_FAMILY, 1000);
        assert_eq!(b.glyphs.len(), 1); // just .notdef so far
    }
}

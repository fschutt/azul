//! TrueType bytecode interpreter — executes font programs (fpgm, prep) and
//! per-glyph instructions to grid-fit outlines.
//!
//! # Specification references
//!
//! - **MS OpenType**: <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions>
//! - **Apple TrueType RM**: <https://developer.apple.com/fonts/TrueType-Reference-Manual/RM05/Chap5.html>
//!
//! # Key instructions
//!
//! | Opcode | Name   | Description |
//! |--------|--------|-------------|
//! | 0x2E   | MDAP   | Move Direct Absolute Point (touch + optional round) |
//! | 0x3E   | MIAP   | Move Indirect Absolute Point (CVT-based, key for twilight zone init) |
//! | 0xC0+  | MDRP   | Move Direct Relative Point (measured original distance) |
//! | 0xE0+  | MIRP   | Move Indirect Relative Point (CVT-based distance from reference) |
//! | 0x39   | IP     | Interpolate Point (preserve relative position between rp1/rp2) |
//! | 0x30   | IUP    | Interpolate Untouched Points (final pass, per-contour) |
//! | 0x5D+  | DELTAP | Delta Exception Point (ppem-specific pixel tuning) |
//! | 0x73+  | DELTAC | Delta Exception CVT (ppem-specific CVT modification) |
//!
//! # Twilight zone (zone 0)
//!
//! The `prep` program uses MIAP on zone 0 to create reference points that
//! encode key font measurements (cap height, x-height, stem widths, etc.).
//! These points must be properly initialized (original + current coordinates)
//! so that glyph programs can reference them via MIRP/IP for grid-fitting.

use std::fmt;

use super::f26dot6::{compute_scale, F2Dot14, F26Dot6};
use super::graphics_state::{GraphicsState, RoundState};

// ── Error type ───────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HintError {
    StackOverflow,
    StackUnderflow,
    InvalidOpcode(u8),
    UndefinedFunction(u32),
    CallStackOverflow,
    InvalidPointIndex(u32),
    InvalidCvtIndex(u32),
    InvalidStorageIndex(u32),
    InvalidZone(u32),
    DivideByZero,
    UnexpectedEndOfBytecode,
    InvalidJump,
    ExceededMaxInstructions,
    FontNotReady,
}

impl fmt::Display for HintError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HintError::StackOverflow => write!(f, "hinting: stack overflow"),
            HintError::StackUnderflow => write!(f, "hinting: stack underflow"),
            HintError::InvalidOpcode(op) => write!(f, "hinting: invalid opcode 0x{:02X}", op),
            HintError::UndefinedFunction(id) => {
                write!(f, "hinting: undefined function {}", id)
            }
            HintError::CallStackOverflow => write!(f, "hinting: call stack overflow"),
            HintError::InvalidPointIndex(i) => {
                write!(f, "hinting: invalid point index {}", i)
            }
            HintError::InvalidCvtIndex(i) => write!(f, "hinting: invalid CVT index {}", i),
            HintError::InvalidStorageIndex(i) => {
                write!(f, "hinting: invalid storage index {}", i)
            }
            HintError::InvalidZone(z) => write!(f, "hinting: invalid zone {}", z),
            HintError::DivideByZero => write!(f, "hinting: divide by zero"),
            HintError::UnexpectedEndOfBytecode => {
                write!(f, "hinting: unexpected end of bytecode")
            }
            HintError::InvalidJump => write!(f, "hinting: invalid jump target"),
            HintError::ExceededMaxInstructions => {
                write!(f, "hinting: exceeded maximum instruction count")
            }
            HintError::FontNotReady => write!(f, "hinting: font not ready (fpgm not executed)"),
        }
    }
}

impl std::error::Error for HintError {}

// ── FreeType-compatible signed multiply-divide ──────────────────────
//
// Matches FreeType's `FT_MulDiv`: compute `a * b / c` with correct
// rounding for any sign combination.  The trick is to take absolute
// values, add a half-divisor for rounding, divide, then reapply sign.

#[inline]
fn ft_muldiv(a: i64, b: i64, c: i64) -> i64 {
    if c == 0 {
        return 0;
    }
    let mut s: i64 = 1;
    let mut aa = a;
    let mut bb = b;
    let mut cc = c;
    if aa < 0 { aa = -aa; s = -s; }
    if bb < 0 { bb = -bb; s = -s; }
    if cc < 0 { cc = -cc; s = -s; }
    let d = (aa * bb + (cc >> 1)) / cc;
    if s > 0 { d } else { -d }
}

/// FreeType's FT_DivFix: compute `(a << 16) / b` with signed rounding.
/// Returns a 16.16 fixed-point scale factor.
#[inline]
fn ft_divfix(a: i64, b: i64) -> i64 {
    if b == 0 {
        return 0x7FFFFFFF;
    }
    let mut s: i64 = 1;
    let mut aa = a;
    let mut bb = b;
    if aa < 0 { aa = -aa; s = -s; }
    if bb < 0 { bb = -bb; s = -s; }
    let q = ((aa << 16) + (bb >> 1)) / bb;
    if s > 0 { q } else { -q }
}

/// FreeType's FT_MulFix: compute `(a * b + 0x8000) >> 16` with signed rounding.
/// Multiplies a value by a 16.16 fixed-point factor.
#[inline]
fn ft_mulfix(a: i64, b: i64) -> i64 {
    let mut s: i64 = 1;
    let mut aa = a;
    let mut bb = b;
    if aa < 0 { aa = -aa; s = -s; }
    if bb < 0 { bb = -bb; s = -s; }
    let c = (aa * bb + 0x8000) >> 16;
    if s > 0 { c } else { -c }
}

// ── Point / Zone types ───────────────────────────────────────────────

#[derive(Copy, Clone, Debug, Default)]
pub struct Point {
    pub x: i32, // F26Dot6
    pub y: i32, // F26Dot6
}

bitflags::bitflags! {
    #[derive(Copy, Clone, Debug, Default)]
    pub struct PointFlags: u8 {
        const TOUCHED_X = 0x01;
        const TOUCHED_Y = 0x02;
        const ON_CURVE  = 0x04;
    }
}

#[derive(Clone, Debug)]
pub struct Zone {
    pub original: Vec<Point>,
    pub current: Vec<Point>,
    /// Unscaled original coordinates in font units (FreeType's "orus").
    /// Used by IUP for more precise interpolation — integer font-unit
    /// coordinates avoid the rounding errors present in scaled F26Dot6.
    pub orus: Vec<Point>,
    pub flags: Vec<PointFlags>,
    pub contour_ends: Vec<u16>,
}

impl Zone {
    pub fn new(n_points: usize) -> Self {
        Zone {
            original: vec![Point::default(); n_points],
            current: vec![Point::default(); n_points],
            orus: vec![Point::default(); n_points],
            flags: vec![PointFlags::empty(); n_points],
            contour_ends: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.current.len()
    }

    pub fn resize(&mut self, n: usize) {
        self.original.resize(n, Point::default());
        self.current.resize(n, Point::default());
        self.orus.resize(n, Point::default());
        self.flags.resize(n, PointFlags::empty());
    }

    /// Ensure the zone can hold at least `n` points, growing if necessary.
    #[inline]
    pub fn ensure_capacity(&mut self, n: usize) {
        if n > self.current.len() && n <= 10_000 {
            self.resize(n);
        }
    }
}

// ── Function definition ──────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct FuncDef {
    pub bytecode: Vec<u8>,
}

// ── Maximum instruction count to prevent infinite loops ──────────────

const MAX_INSTRUCTIONS: u64 = 1_000_000;
const MAX_CALL_DEPTH: u32 = 64;

// ── Interpreter ──────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct Interpreter {
    // Stack
    pub(crate) stack: Vec<i32>,
    max_stack: usize,

    // CVT (F26Dot6 values stored as i32)
    pub(crate) cvt: Vec<i32>,
    // Original scaled CVT values (before prep/WCVTP modifications).
    // DELTAC applies adjustments to these originals, not WCVTP-modified values.
    pub(crate) cvt_original: Vec<i32>,
    // Accumulated DELTAC adjustments per CVT entry. Multiple DELTACs
    // targeting the same CVT at the same ppem accumulate their deltas.
    cvt_deltac_accum: Vec<i32>,

    // Storage area
    pub(crate) storage: Vec<i32>,

    // Function/instruction definitions
    pub(crate) fdefs: Vec<Option<FuncDef>>,
    pub(crate) idefs: Vec<Option<FuncDef>>,

    // Graphics state
    pub(crate) gs: GraphicsState,
    pub(crate) default_gs: GraphicsState,

    // Zones: 0 = twilight, 1 = glyph
    pub(crate) zones: [Zone; 2],

    // Font metrics
    pub(crate) ppem: u16,
    pub(crate) point_size: i32, // F26Dot6
    pub(crate) units_per_em: u16,
    pub(crate) scale: i64, // 16.16 fixed-point

    // Execution state
    instruction_count: u64,
    call_depth: u32,

    /// When true, dump every instruction to stderr (for debugging)
    pub trace_mode: bool,
    /// When true, log move_point calls on glyph zone to stderr
    pub debug_trace_points: bool,

    // ── Hinting mode flags (toggle to find correct behavior) ────────

    /// Flag A: Reset X coordinates to original after glyph program.
    /// Matches FreeType v40 / Chrome which only applies Y-axis hinting.
    /// Without this: full X+Y hinting (v35 mode), causes SHPIX spacing issues.
    /// With this: Y-only hinting, but curves that depend on X/Y coordination
    /// (e.g., 'u' bottom curve at ppem=20) may distort.
    pub suppress_x_axis: bool,

    /// Flag B: Snapshot Y after IUP[Y], discard post-IUP Y modifications.
    /// Some glyph programs modify Y after IUP via function calls (e.g.,
    /// 'o' at ppem=12: pt5 moves from 150→77). FreeType v40 may suppress these.
    /// Without this: post-IUP moves apply, causing diamond shapes at small ppem.
    /// With this: clean IUP interpolation preserved, but some legitimate
    /// post-IUP adjustments are also discarded.
    pub snapshot_iup_y: bool,

    /// Flag C: Use orig_dist sign for minimum_distance when dist rounds to zero.
    /// Without this: small negative distances get +min_dist (wrong direction).
    /// With this: preserves original direction, matching FreeType behavior.
    pub fix_min_distance_sign: bool,

    /// Snapshot of Y coordinates taken right after IUP[Y] runs.
    iup_y_snapshot: Option<Vec<i32>>,
}

impl Interpreter {
    /// Create a new interpreter from maxp table values.
    pub fn new(
        max_stack_elements: u16,
        max_storage: u16,
        max_function_defs: u16,
        max_instruction_defs: u16,
        max_twilight_points: u16,
        units_per_em: u16,
    ) -> Self {
        // FreeType sizes the stack to maxStackElements + 32 so under-declared
        // fonts (whose real high-water mark exceeds maxp) don't lose all hinting.
        let max_stack = max_stack_elements as usize + 32;
        Interpreter {
            stack: Vec::with_capacity(max_stack),
            max_stack,
            cvt: Vec::new(),
            cvt_original: Vec::new(),
            cvt_deltac_accum: Vec::new(),
            storage: vec![0i32; max_storage as usize],
            fdefs: vec![None; max_function_defs as usize],
            idefs: vec![None; max_instruction_defs as usize],
            gs: GraphicsState::default(),
            default_gs: GraphicsState::default(),
            zones: [
                Zone::new(max_twilight_points as usize), // twilight
                Zone::new(0),                            // glyph (resized per glyph)
            ],
            ppem: 0,
            point_size: 0,
            units_per_em,
            scale: 0,
            instruction_count: 0,
            call_depth: 0,
            trace_mode: false,
            debug_trace_points: false,
            suppress_x_axis: false,    // Flag A: OFF — full X+Y hinting
            snapshot_iup_y: false,    // Flag B: OFF
            fix_min_distance_sign: true, // Flag C: preserve direction on zero-round
            iup_y_snapshot: None,
        }
    }

    /// Returns the current CVT values (F26Dot6 stored as i32).
    pub fn cvt(&self) -> &[i32] {
        &self.cvt
    }

    /// Returns the max stack size.
    pub fn max_stack(&self) -> usize {
        self.max_stack
    }

    /// Returns the graphics state (for debugging).
    pub fn graphics_state(&self) -> &GraphicsState {
        &self.gs
    }

    /// Returns the storage area size.
    pub fn storage_len(&self) -> usize {
        self.storage.len()
    }

    /// Read a storage area value (for debugging).
    pub fn read_storage(&self, idx: usize) -> Option<i32> {
        self.storage.get(idx).copied()
    }

    /// Returns the number of function definitions.
    pub fn fdef_count(&self) -> usize {
        self.fdefs.len()
    }

    /// Returns the number of twilight zone points.
    pub fn twilight_point_count(&self) -> usize {
        self.zones[0].original.len()
    }

    /// Returns the ppem value.
    pub fn ppem(&self) -> u16 {
        self.ppem
    }

    /// Returns the scale value (16.16 fixed-point).
    pub fn scale(&self) -> i64 {
        self.scale
    }

    /// Returns units_per_em.
    pub fn units_per_em(&self) -> u16 {
        self.units_per_em
    }

    /// Execute the font program (`fpgm`) to populate function definitions.
    pub fn execute_fpgm(&mut self, fpgm: &[u8]) -> Result<(), HintError> {
        self.stack.clear();
        self.gs = GraphicsState::default();
        self.instruction_count = 0;
        self.call_depth = 0;
        self.execute(fpgm)
    }

    /// Set the ppem size and execute the prep program.
    pub fn execute_prep(&mut self, prep: &[u8], ppem: u16, point_size: f64) -> Result<(), HintError> {
        self.ppem = ppem;
        self.point_size = F26Dot6::from_f64(point_size).to_bits();
        self.scale = compute_scale(ppem, self.units_per_em);

        self.stack.clear();
        self.gs = GraphicsState::default();
        // Note: storage is NOT cleared between prep runs.
        // The prep program is responsible for setting all values it needs.
        self.instruction_count = 0;
        self.call_depth = 0;
        self.execute(prep)?;

        // Save the modified graphics state as the default for glyph programs
        self.default_gs = self.gs.clone();
        Ok(())
    }

    /// Scale CVT values from FUnits to F26Dot6 pixels.
    pub fn scale_cvt(&mut self, cvt_funits: &[i16]) {
        self.cvt.clear();
        self.cvt.reserve(cvt_funits.len());
        for &funit in cvt_funits {
            self.cvt
                .push(F26Dot6::from_funits(funit as i32, self.scale).to_bits());
        }
        // Save original scaled CVT values. DELTAC applies adjustments
        // to these originals, not to WCVTP-modified values. Without this,
        // the prep's WCVTP rounds CVT[0] from 1422→1408, then DELTAC(-40)
        // gives 1368 (rounds to 1344=21px). With originals, DELTAC(-40)
        // applies to 1422 giving 1382 (rounds to 1408=22px, correct).
        self.cvt_original = self.cvt.clone();
        self.cvt_deltac_accum = vec![0i32; self.cvt.len()];
    }

    /// Hint a glyph outline by executing its bytecode instructions.
    ///
    /// `points` are pre-scaled to F26Dot6 coordinates.
    /// `on_curve` flags indicate whether each point is on-curve.
    /// `contour_ends` gives the index of the last point in each contour.
    /// `instructions` is the per-glyph bytecode from the glyf table.
    ///
    /// After execution, the hinted point positions can be read from
    /// `self.zones[1].current`.
    pub fn hint_glyph(
        &mut self,
        points: &[Point],
        on_curve: &[bool],
        contour_ends: &[u16],
        instructions: &[u8],
    ) -> Result<(), HintError> {
        // Set up the glyph zone with empty orus (no unscaled data)
        self.hint_glyph_with_orus(points, None, on_curve, contour_ends, instructions)
    }

    /// Hint a glyph with optional unscaled original coordinates (orus).
    ///
    /// If `orus` is provided, IUP interpolation uses these exact integer
    /// coordinates instead of the scaled F26Dot6 `points`. This matches
    /// FreeType's approach and avoids ±1 F26Dot6 rounding errors in IUP.
    pub fn hint_glyph_with_orus(
        &mut self,
        points: &[Point],
        orus: Option<&[Point]>,
        on_curve: &[bool],
        contour_ends: &[u16],
        instructions: &[u8],
    ) -> Result<(), HintError> {
        // Set up the glyph zone
        let n = points.len();
        let zone = &mut self.zones[1];
        zone.resize(n);
        for i in 0..n {
            zone.original[i] = points[i];
            zone.current[i] = points[i];
            // Store unscaled coordinates if provided, otherwise use scaled
            zone.orus[i] = if let Some(orus) = orus {
                orus.get(i).copied().unwrap_or(points[i])
            } else {
                points[i]
            };
            let mut flags = PointFlags::empty();
            if on_curve.get(i).copied().unwrap_or(false) {
                flags |= PointFlags::ON_CURVE;
            }
            zone.flags[i] = flags;
        }
        zone.contour_ends = contour_ends.to_vec();

        // Reset graphics state to defaults (set by prep), then override
        // per TrueType spec: projection/freedom vectors reset to x-axis,
        // reference points to 0, loop to 1, zone pointers to 1.
        self.gs = self.default_gs.clone();
        self.gs.projection_vector = (F2Dot14::ONE, F2Dot14::ZERO);
        self.gs.freedom_vector = (F2Dot14::ONE, F2Dot14::ZERO);
        self.gs.dual_projection_vector = (F2Dot14::ONE, F2Dot14::ZERO);
        self.gs.rp0 = 0;
        self.gs.rp1 = 0;
        self.gs.rp2 = 0;
        self.gs.loop_value = 1;
        self.gs.zp0 = 1;
        self.gs.zp1 = 1;
        self.gs.zp2 = 1;
        // FreeType TT_Run_Context resets round_state to RTG (1) before every
        // glyph, so a non-grid round mode left active by prep does not leak in.
        self.gs.round_state = RoundState::Grid;

        self.stack.clear();
        self.instruction_count = 0;
        self.call_depth = 0;

        self.execute(instructions)?;

        // v40 backward compatibility: undo X-axis movements AND post-IUP
        // Y-axis modifications.
        //
        // FreeType v40 (DEFAULT mode) and Chrome/Core Text on macOS:
        // - Suppress all X-axis grid-fitting (subpixel rendering handles X)
        // - After IUP[Y], some glyph programs apply post-IUP function calls
        //   that modify Y coordinates (e.g., Times New Roman 'o' at ppem=12
        //   moves pt5.y from 150→77 via a called function). FreeType v40
        //   suppresses these post-IUP modifications.
        //
        // We handle this by saving Y values right after IUP[Y] runs
        // (stored in `iup_y_snapshot`) and restoring them after execution.
        // Flag A: suppress X-axis movements
        if self.suppress_x_axis {
            let zone = &mut self.zones[1];
            for i in 0..zone.current.len() {
                zone.current[i].x = zone.original[i].x;
            }
        }

        // Flag B: restore Y from IUP snapshot (discard post-IUP Y mods)
        if self.snapshot_iup_y {
            if let Some(snap) = &self.iup_y_snapshot {
                let zone = &mut self.zones[1];
                for i in 0..zone.current.len().min(snap.len()) {
                    zone.current[i].y = snap[i];
                }
            }
            self.iup_y_snapshot = None;
        }

        Ok(())
    }

    // ── Core execution loop ──────────────────────────────────────────

    fn execute(&mut self, bytecode: &[u8]) -> Result<(), HintError> {
        let mut ip: usize = 0;
        while ip < bytecode.len() {
            self.instruction_count += 1;
            if self.instruction_count > MAX_INSTRUCTIONS {
                return Err(HintError::ExceededMaxInstructions);
            }

            let opcode = bytecode[ip];
            ip += 1;

            self.dispatch(opcode, bytecode, &mut ip)?;
        }
        Ok(())
    }

    fn dispatch(
        &mut self,
        opcode: u8,
        bytecode: &[u8],
        ip: &mut usize,
    ) -> Result<(), HintError> {
        match opcode {
            // ── Vector setting ───────────────────────────────────
            0x00 => {
                // SVTCA[y] - set both vectors to y-axis
                self.gs.projection_vector = (F2Dot14::ZERO, F2Dot14::ONE);
                self.gs.freedom_vector = (F2Dot14::ZERO, F2Dot14::ONE);
                self.gs.dual_projection_vector = (F2Dot14::ZERO, F2Dot14::ONE);
            }
            0x01 => {
                // SVTCA[x] - set both vectors to x-axis
                self.gs.projection_vector = (F2Dot14::ONE, F2Dot14::ZERO);
                self.gs.freedom_vector = (F2Dot14::ONE, F2Dot14::ZERO);
                self.gs.dual_projection_vector = (F2Dot14::ONE, F2Dot14::ZERO);
            }
            0x02 => {
                // SPVTCA[y] - set projection vector to y-axis
                self.gs.projection_vector = (F2Dot14::ZERO, F2Dot14::ONE);
                self.gs.dual_projection_vector = (F2Dot14::ZERO, F2Dot14::ONE);
            }
            0x03 => {
                // SPVTCA[x] - set projection vector to x-axis
                self.gs.projection_vector = (F2Dot14::ONE, F2Dot14::ZERO);
                self.gs.dual_projection_vector = (F2Dot14::ONE, F2Dot14::ZERO);
            }
            0x04 => {
                // SFVTCA[y] - set freedom vector to y-axis
                self.gs.freedom_vector = (F2Dot14::ZERO, F2Dot14::ONE);
            }
            0x05 => {
                // SFVTCA[x] - set freedom vector to x-axis
                self.gs.freedom_vector = (F2Dot14::ONE, F2Dot14::ZERO);
            }
            0x06..=0x07 => {
                // SPVTL[a] - set projection vector to line
                let a = opcode & 1;
                self.op_spvtl(a != 0)?;
            }
            0x08..=0x09 => {
                // SFVTL[a] - set freedom vector to line
                let a = opcode & 1;
                self.op_sfvtl(a != 0)?;
            }
            0x0A => {
                // SPVFS - set projection vector from stack
                let y = self.pop()? as i16 as i32;
                let x = self.pop()? as i16 as i32;
                self.gs.projection_vector = (F2Dot14::from_bits(x), F2Dot14::from_bits(y));
                self.gs.dual_projection_vector = self.gs.projection_vector;
            }
            0x0B => {
                // SFVFS - set freedom vector from stack
                let y = self.pop()? as i16 as i32;
                let x = self.pop()? as i16 as i32;
                self.gs.freedom_vector = (F2Dot14::from_bits(x), F2Dot14::from_bits(y));
            }
            0x0C => {
                // GPV - get projection vector
                self.push(self.gs.projection_vector.0.to_bits())?;
                self.push(self.gs.projection_vector.1.to_bits())?;
            }
            0x0D => {
                // GFV - get freedom vector
                self.push(self.gs.freedom_vector.0.to_bits())?;
                self.push(self.gs.freedom_vector.1.to_bits())?;
            }
            0x0E => {
                // SFVTPV - set freedom vector to projection vector
                self.gs.freedom_vector = self.gs.projection_vector;
            }
            0x0F => {
                // ISECT - move point to intersection
                self.op_isect()?;
            }

            // ── Reference point / zone setting ───────────────────
            0x10 => {
                // SRP0
                self.gs.rp0 = self.pop()? as u32;
            }
            0x11 => {
                // SRP1
                self.gs.rp1 = self.pop()? as u32;
            }
            0x12 => {
                // SRP2
                self.gs.rp2 = self.pop()? as u32;
            }
            0x13 => {
                // SZP0
                let zone = self.pop()? as u32;
                if zone > 1 {
                    return Err(HintError::InvalidZone(zone));
                }
                self.gs.zp0 = zone;
            }
            0x14 => {
                // SZP1
                let zone = self.pop()? as u32;
                if zone > 1 {
                    return Err(HintError::InvalidZone(zone));
                }
                self.gs.zp1 = zone;
            }
            0x15 => {
                // SZP2
                let zone = self.pop()? as u32;
                if zone > 1 {
                    return Err(HintError::InvalidZone(zone));
                }
                self.gs.zp2 = zone;
            }
            0x16 => {
                // SZPS - set all zone pointers
                let zone = self.pop()? as u32;
                if zone > 1 {
                    return Err(HintError::InvalidZone(zone));
                }
                self.gs.zp0 = zone;
                self.gs.zp1 = zone;
                self.gs.zp2 = zone;
            }
            0x17 => {
                // SLOOP
                let n = self.pop()?;
                self.gs.loop_value = n.max(1) as u32;
            }

            // ── Rounding mode ────────────────────────────────────
            0x18 => {
                // RTG - round to grid
                self.gs.round_state = RoundState::Grid;
            }
            0x19 => {
                // RTHG - round to half grid
                self.gs.round_state = RoundState::HalfGrid;
            }

            // ── Distances ────────────────────────────────────────
            0x1A => {
                // SMD - set minimum distance
                let d = self.pop()?;
                self.gs.minimum_distance = F26Dot6::from_bits(d);
            }

            // ── Control flow ─────────────────────────────────────
            0x1B => {
                // ELSE - skip to EIF
                self.skip_else(bytecode, ip)?;
            }
            0x1C => {
                // JMPR - jump relative
                let offset = self.pop()?;
                // offset is relative to the JMPR opcode position (*ip - 1)
                let opcode_pos = *ip as i64 - 1;
                let new_ip = opcode_pos + offset as i64;
                if new_ip < 0 || new_ip > bytecode.len() as i64 {
                    return Err(HintError::InvalidJump);
                }
                *ip = new_ip as usize;
            }
            0x1D => {
                // SCVTCI - set CVT cut-in
                let v = self.pop()?;
                self.gs.control_value_cut_in = F26Dot6::from_bits(v);
            }
            0x1E => {
                // SSWCI - set single width cut-in
                let v = self.pop()?;
                self.gs.single_width_cut_in = F26Dot6::from_bits(v);
            }
            0x1F => {
                // SSW - set single width value (FUnits -> F26Dot6)
                let v = self.pop()?;
                self.gs.single_width_value =
                    F26Dot6::from_funits(v, self.scale);
            }

            // ── Stack manipulation ───────────────────────────────
            0x20 => {
                // DUP
                let v = self.peek()?;
                self.push(v)?;
            }
            0x21 => {
                // POP
                self.pop()?;
            }
            0x22 => {
                // CLEAR
                self.stack.clear();
            }
            0x23 => {
                // SWAP
                let len = self.stack.len();
                if len < 2 {
                    return Err(HintError::StackUnderflow);
                }
                self.stack.swap(len - 1, len - 2);
            }
            0x24 => {
                // DEPTH
                let d = self.stack.len() as i32;
                self.push(d)?;
            }
            0x25 => {
                // CINDEX - copy indexed element
                let idx = self.pop()? as usize;
                let len = self.stack.len();
                if idx == 0 || idx > len {
                    return Err(HintError::StackUnderflow);
                }
                let v = self.stack[len - idx];
                self.push(v)?;
            }
            0x26 => {
                // MINDEX - move indexed element to top
                let idx = self.pop()? as usize;
                let len = self.stack.len();
                if idx == 0 || idx > len {
                    return Err(HintError::StackUnderflow);
                }
                let pos = len - idx;
                let v = self.stack.remove(pos);
                self.stack.push(v);
            }

            // ── Point alignment ──────────────────────────────────
            0x27 => {
                // ALIGNPTS
                self.op_alignpts()?;
            }

            0x29 => {
                // UTP - untouch point
                let p = self.pop()? as u32;
                let zp0 = self.gs.zp0 as usize;
                if let Some(flags) = self.zones[zp0].flags.get_mut(p as usize) {
                    // Clear touched flags based on freedom vector
                    if self.gs.freedom_vector.0.to_bits() != 0 {
                        flags.remove(PointFlags::TOUCHED_X);
                    }
                    if self.gs.freedom_vector.1.to_bits() != 0 {
                        flags.remove(PointFlags::TOUCHED_Y);
                    }
                }
            }

            // ── Function calls ───────────────────────────────────
            0x2A => {
                // LOOPCALL
                let fn_id = self.pop()? as u32;
                let count = self.pop()? as u32;
                if self.call_depth >= MAX_CALL_DEPTH {
                    return Err(HintError::CallStackOverflow);
                }
                if count > 10000 {
                    return Err(HintError::ExceededMaxInstructions);
                }
                let func = self
                    .fdefs
                    .get(fn_id as usize)
                    .and_then(|f| f.as_ref())
                    .ok_or(HintError::UndefinedFunction(fn_id))?
                    .bytecode
                    .clone();
                for _ in 0..count {
                    self.call_depth += 1;
                    self.execute(&func)?;
                    self.call_depth -= 1;
                }
            }
            0x2B => {
                // CALL
                let fn_id = self.pop()? as u32;
                if self.call_depth >= MAX_CALL_DEPTH {
                    return Err(HintError::CallStackOverflow);
                }
                let func = self
                    .fdefs
                    .get(fn_id as usize)
                    .and_then(|f| f.as_ref())
                    .ok_or(HintError::UndefinedFunction(fn_id))?
                    .bytecode
                    .clone();
                self.call_depth += 1;
                self.execute(&func)?;
                self.call_depth -= 1;
            }
            0x2C => {
                // FDEF - function definition
                let fn_id = self.pop()? as u32;
                let start = *ip;
                // Scan forward to find ENDF (0x2D), handling nested FDEF/ENDF
                let end = self.find_endf(bytecode, ip)?;
                let func_bytecode = bytecode[start..end].to_vec();
                if (fn_id as usize) < self.fdefs.len() {
                    self.fdefs[fn_id as usize] = Some(FuncDef {
                        bytecode: func_bytecode,
                    });
                }
                // ip now points past ENDF
            }
            0x2D => {
                // ENDF - end of function
                // In normal execution flow (inside CALL), this returns from execute()
                return Ok(());
            }

            // ── Point movement ───────────────────────────────────
            0x2E..=0x2F => {
                // MDAP[r] - move direct absolute point
                let round = opcode & 1 != 0;
                self.op_mdap(round)?;
            }
            0x30..=0x31 => {
                // IUP[a] - interpolate untouched points
                let axis = opcode & 1; // 0 = y, 1 = x
                self.op_iup(axis)?;
                // In v40 mode, snapshot Y values right after IUP[Y] runs
                // so post-IUP function calls can't modify them.
                if axis == 0 && self.snapshot_iup_y {
                    self.iup_y_snapshot = Some(
                        self.zones[1].current.iter().map(|p| p.y).collect()
                    );
                }
            }
            0x32..=0x33 => {
                // SHP[a] - shift point
                // SHP[0] (0x32) uses rp2/zp1, SHP[1] (0x33) uses rp1/zp0
                let use_rp1 = opcode & 1 != 0;
                self.op_shp(use_rp1)?;
            }
            0x34..=0x35 => {
                // SHC[a] - shift contour
                // SHC[0] (0x34) uses rp2/zp1, SHC[1] (0x35) uses rp1/zp0
                let use_rp1 = opcode & 1 != 0;
                self.op_shc(use_rp1)?;
            }
            0x36..=0x37 => {
                // SHZ[a] - shift zone
                // SHZ[0] (0x36) uses rp2/zp1, SHZ[1] (0x37) uses rp1/zp0
                let use_rp1 = opcode & 1 != 0;
                self.op_shz(use_rp1)?;
            }
            0x38 => {
                // SHPIX - shift point by pixel amount
                self.op_shpix()?;
            }
            0x39 => {
                // IP - interpolate point
                self.op_ip()?;
            }
            0x3A..=0x3B => {
                // MSIRP[a] - move stack indirect relative point
                let set_rp0 = opcode & 1 != 0;
                self.op_msirp(set_rp0)?;
            }
            0x3C => {
                // ALIGNRP - align relative point
                self.op_alignrp()?;
            }
            0x3D => {
                // RTDG - round to double grid
                self.gs.round_state = RoundState::DoubleGrid;
            }
            0x3E..=0x3F => {
                // MIAP[r] - move indirect absolute point
                let round = opcode & 1 != 0;
                self.op_miap(round)?;
            }

            // ── Push instructions ────────────────────────────────
            0x40 => {
                // NPUSHB - push n bytes
                if *ip >= bytecode.len() {
                    return Err(HintError::UnexpectedEndOfBytecode);
                }
                let n = bytecode[*ip] as usize;
                *ip += 1;
                if *ip + n > bytecode.len() {
                    return Err(HintError::UnexpectedEndOfBytecode);
                }
                for i in 0..n {
                    self.push(bytecode[*ip + i] as i32)?;
                }
                *ip += n;
            }
            0x41 => {
                // NPUSHW - push n words
                if *ip >= bytecode.len() {
                    return Err(HintError::UnexpectedEndOfBytecode);
                }
                let n = bytecode[*ip] as usize;
                *ip += 1;
                if *ip + n * 2 > bytecode.len() {
                    return Err(HintError::UnexpectedEndOfBytecode);
                }
                for i in 0..n {
                    // Cast high byte to i8 for proper sign extension.
                    let hi = bytecode[*ip + i * 2] as i8;
                    let lo = bytecode[*ip + i * 2 + 1] as u8;
                    let val = ((hi as i32) << 8) | (lo as i32);
                    self.push(val)?;
                }
                *ip += n * 2;
            }

            // ── Storage / CVT ────────────────────────────────────
            0x42 => {
                // WS - write storage
                let val = self.pop()?;
                let idx = self.pop()? as u32;
                let i = idx as usize;
                // Dynamically grow storage if needed (fonts may exceed maxp limits)
                if i >= self.storage.len() {
                    if i > 10_000 {
                        return Err(HintError::InvalidStorageIndex(idx));
                    }
                    self.storage.resize(i + 1, 0);
                }
                self.storage[i] = val;
            }
            0x43 => {
                // RS - read storage
                let idx = self.pop()? as u32;
                let i = idx as usize;
                // Dynamically grow storage if needed (reads default to 0)
                if i >= self.storage.len() {
                    if i > 10_000 {
                        return Err(HintError::InvalidStorageIndex(idx));
                    }
                    self.storage.resize(i + 1, 0);
                }
                self.push(self.storage[i])?;
            }
            0x44 => {
                // WCVTP - write CVT in pixel units (F26Dot6)
                let val = self.pop()?;
                let idx = self.pop()? as u32;
                let i = idx as usize;
                if i >= self.cvt.len() {
                    // Bounds-check like WS/RS: an out-of-range (negative/huge)
                    // index must not trigger a multi-GB resize/OOM.
                    if i > 10_000 {
                        return Err(HintError::InvalidCvtIndex(idx));
                    }
                    self.cvt.resize(i + 1, 0);
                }
                self.cvt[i] = val;
            }
            0x45 => {
                // RCVT - read CVT
                let idx = self.pop()? as u32;
                let val = self.read_cvt(idx)?;
                self.push(val)?;
            }
            0x46..=0x47 => {
                // GC[a] - get coordinate (0=current, 1=original)
                let use_original = opcode & 1 != 0;
                self.op_gc(use_original)?;
            }
            0x48 => {
                // SCFS - set coordinate from stack
                self.op_scfs()?;
            }
            0x49..=0x4A => {
                // MD[a] - measure distance
                // 0x49 = MD[0]: use current (grid-fitted) positions
                // 0x4A = MD[1]: use original (unhinted) positions
                // 0x49 = MD[0]: use current (grid-fitted) positions
                // 0x4A = MD[1]: use original (unhinted) positions
                let use_original = opcode == 0x4A;
                self.op_md(use_original)?;
            }
            0x4B => {
                // MPPEM - measure pixels per em
                self.push(self.ppem as i32)?;
            }
            0x4C => {
                // MPS - measure point size (F26Dot6)
                self.push(self.point_size)?;
            }

            // ── Boolean / flip ───────────────────────────────────
            0x4D => {
                // FLIPON
                self.gs.auto_flip = true;
            }
            0x4E => {
                // FLIPOFF
                self.gs.auto_flip = false;
            }
            0x4F => {
                // DEBUG - no-op
            }

            // ── Comparison ───────────────────────────────────────
            0x50 => {
                // LT
                let b = self.pop()?;
                let a = self.pop()?;
                self.push(if a < b { 1 } else { 0 })?;
            }
            0x51 => {
                // LTEQ
                let b = self.pop()?;
                let a = self.pop()?;
                self.push(if a <= b { 1 } else { 0 })?;
            }
            0x52 => {
                // GT
                let b = self.pop()?;
                let a = self.pop()?;
                self.push(if a > b { 1 } else { 0 })?;
            }
            0x53 => {
                // GTEQ
                let b = self.pop()?;
                let a = self.pop()?;
                self.push(if a >= b { 1 } else { 0 })?;
            }
            0x54 => {
                // EQ
                let b = self.pop()?;
                let a = self.pop()?;
                self.push(if a == b { 1 } else { 0 })?;
            }
            0x55 => {
                // NEQ
                let b = self.pop()?;
                let a = self.pop()?;
                self.push(if a != b { 1 } else { 0 })?;
            }
            0x56 => {
                // ODD
                let v = self.pop()?;
                let rounded = self.gs.round(F26Dot6::from_bits(v));
                self.push(if (rounded.to_i32() & 1) != 0 { 1 } else { 0 })?;
            }
            0x57 => {
                // EVEN
                let v = self.pop()?;
                let rounded = self.gs.round(F26Dot6::from_bits(v));
                self.push(if (rounded.to_i32() & 1) == 0 { 1 } else { 0 })?;
            }

            // ── IF / ELSE / EIF ──────────────────────────────────
            0x58 => {
                // IF
                let cond = self.pop()?;
                if cond == 0 {
                    self.skip_to_else_or_eif(bytecode, ip)?;
                }
            }
            0x59 => {
                // EIF - end if (no-op when reached normally)
            }

            // ── Logic ────────────────────────────────────────────
            0x5A => {
                // AND
                let b = self.pop()?;
                let a = self.pop()?;
                self.push(if a != 0 && b != 0 { 1 } else { 0 })?;
            }
            0x5B => {
                // OR
                let b = self.pop()?;
                let a = self.pop()?;
                self.push(if a != 0 || b != 0 { 1 } else { 0 })?;
            }
            0x5C => {
                // NOT
                let v = self.pop()?;
                self.push(if v == 0 { 1 } else { 0 })?;
            }

            // ── Delta instructions ───────────────────────────────
            0x5D => self.op_deltap(1, bytecode, ip)?, // DELTAP1
            0x5E => {
                // SDB - set delta base
                let new_base = self.pop()? as u16;
                self.gs.delta_base = new_base;
            }
            0x5F => {
                // SDS - set delta shift
                self.gs.delta_shift = self.pop()? as u16;
            }

            // ── Arithmetic ───────────────────────────────────────
            0x60 => {
                // ADD
                let b = self.pop()?;
                let a = self.pop()?;
                self.push(a.wrapping_add(b))?;
            }
            0x61 => {
                // SUB
                let b = self.pop()?;
                let a = self.pop()?;
                self.push(a.wrapping_sub(b))?;
            }
            0x62 => {
                // DIV
                let b = self.pop()?;
                if b == 0 {
                    return Err(HintError::DivideByZero);
                }
                let a = self.pop()?;
                // F26Dot6 division: a * 64 / b with FreeType-compatible rounding
                let result = ft_muldiv(a as i64, 64, b as i64);
                self.push(result as i32)?;
            }
            0x63 => {
                // MUL
                let b = self.pop()?;
                let a = self.pop()?;
                // F26Dot6 multiplication: a * b / 64 with FreeType-compatible rounding
                let result = ft_muldiv(a as i64, b as i64, 64);
                self.push(result as i32)?;
            }
            0x64 => {
                // ABS
                let v = self.pop()?;
                self.push(v.abs())?;
            }
            0x65 => {
                // NEG
                let v = self.pop()?;
                self.push(-v)?;
            }
            0x66 => {
                // FLOOR
                let v = self.pop()?;
                self.push(v & !63)?;
            }
            0x67 => {
                // CEILING
                let v = self.pop()?;
                self.push((v + 63) & !63)?;
            }

            // ── ROUND / NROUND ───────────────────────────────────
            0x68..=0x6B => {
                // ROUND[ab] - round value
                let v = self.pop()?;
                let result = self.gs.round(F26Dot6::from_bits(v));
                self.push(result.to_bits())?;
            }
            0x6C..=0x6F => {
                // NROUND[ab] - no-round (pass through)
                // Value stays on stack as-is
            }

            // ── More CVT / Delta ─────────────────────────────────
            0x70 => {
                // WCVTF - write CVT in FUnits
                let val = self.pop()?; // value in FUnits
                let idx = self.pop()? as u32;
                let scaled = F26Dot6::from_funits(val, self.scale).to_bits();
                let i = idx as usize;
                if i >= self.cvt.len() {
                    if i > 10_000 {
                        return Err(HintError::InvalidCvtIndex(idx));
                    }
                    self.cvt.resize(i + 1, 0);
                }
                self.cvt[i] = scaled;
            }
            0x71 => self.op_deltap(2, bytecode, ip)?, // DELTAP2
            0x72 => self.op_deltap(3, bytecode, ip)?, // DELTAP3
            0x73 => self.op_deltac(1)?,               // DELTAC1
            0x74 => self.op_deltac(2)?,               // DELTAC2
            0x75 => self.op_deltac(3)?,               // DELTAC3

            // ── Super rounding ───────────────────────────────────
            0x76 => {
                // SROUND
                let n = self.pop()? as u32;
                self.gs.set_super_round(n, false);
                self.gs.round_state = RoundState::Super;
            }
            0x77 => {
                // S45ROUND
                let n = self.pop()? as u32;
                self.gs.set_super_round(n, true);
                self.gs.round_state = RoundState::Super45;
            }

            // ── Conditional jumps ────────────────────────────────
            0x78 => {
                // JROT - jump relative on true
                let cond = self.pop()?;
                let offset = self.pop()?;
                if cond != 0 {
                    // offset is relative to the JROT opcode position (*ip - 1)
                    let opcode_pos = *ip as i64 - 1;
                    let new_ip = opcode_pos + offset as i64;
                    if new_ip < 0 || new_ip > bytecode.len() as i64 {
                        return Err(HintError::InvalidJump);
                    }
                    *ip = new_ip as usize;
                }
            }
            0x79 => {
                // JROF - jump relative on false
                let cond = self.pop()?;
                let offset = self.pop()?;
                if cond == 0 {
                    // offset is relative to the JROF opcode position (*ip - 1)
                    let opcode_pos = *ip as i64 - 1;
                    let new_ip = opcode_pos + offset as i64;
                    if new_ip < 0 || new_ip > bytecode.len() as i64 {
                        return Err(HintError::InvalidJump);
                    }
                    *ip = new_ip as usize;
                }
            }

            0x7A => {
                // ROFF - round off
                self.gs.round_state = RoundState::Off;
            }

            0x7C => {
                // RUTG - round up to grid
                self.gs.round_state = RoundState::UpToGrid;
            }
            0x7D => {
                // RDTG - round down to grid
                self.gs.round_state = RoundState::DownToGrid;
            }
            0x7E => {
                // SANGW (obsolete) - no-op
                self.pop()?;
            }
            0x7F => {
                // AA (obsolete) - no-op
                self.pop()?;
            }

            // ── Flip instructions ────────────────────────────────
            0x80 => {
                // FLIPPT
                self.op_flippt()?;
            }
            0x81 => {
                // FLIPRGON
                let hi = self.pop()? as usize;
                let lo = self.pop()? as usize;
                // Clamp the range to the glyph zone so a huge/negative index
                // (e.g. -1 -> usize::MAX) does not spin an unbounded loop.
                let len = self.zones[1].flags.len();
                if lo < len {
                    let hi = hi.min(len - 1);
                    for i in lo..=hi {
                        self.zones[1].flags[i].insert(PointFlags::ON_CURVE);
                    }
                }
            }
            0x82 => {
                // FLIPRGOFF
                let hi = self.pop()? as usize;
                let lo = self.pop()? as usize;
                let len = self.zones[1].flags.len();
                if lo < len {
                    let hi = hi.min(len - 1);
                    for i in lo..=hi {
                        self.zones[1].flags[i].remove(PointFlags::ON_CURVE);
                    }
                }
            }

            0x85 => {
                // SCANCTRL
                self.gs.scan_control = self.pop()? as u32;
            }
            0x86..=0x87 => {
                // SDPVTL[a] - set dual projection vector to line
                let a = opcode & 1;
                self.op_sdpvtl(a != 0)?;
            }
            0x88 => {
                // GETINFO
                self.op_getinfo()?;
            }
            0x89 => {
                // IDEF - instruction definition
                let instr_id = self.pop()? as u32;
                let start = *ip;
                let end = self.find_endf(bytecode, ip)?;
                let func_bytecode = bytecode[start..end].to_vec();
                if (instr_id as usize) < self.idefs.len() {
                    self.idefs[instr_id as usize] = Some(FuncDef {
                        bytecode: func_bytecode,
                    });
                }
            }
            0x8A => {
                // ROLL - roll top 3 stack elements
                let len = self.stack.len();
                if len < 3 {
                    return Err(HintError::StackUnderflow);
                }
                let a = self.stack[len - 1]; // top
                let b = self.stack[len - 2]; // second
                let c = self.stack[len - 3]; // third
                // Move third to top: [a,b,c] → [c,a,b]
                self.stack[len - 1] = c;
                self.stack[len - 2] = a;
                self.stack[len - 3] = b;
            }
            0x8B => {
                // MAX
                let b = self.pop()?;
                let a = self.pop()?;
                self.push(a.max(b))?;
            }
            0x8C => {
                // MIN
                let b = self.pop()?;
                let a = self.pop()?;
                self.push(a.min(b))?;
            }
            0x8D => {
                // SCANTYPE
                self.gs.scan_type = self.pop()?;
            }
            0x8E => {
                // INSTCTRL - pops selector (s) then value (v)
                let s = self.pop()? as u32;
                let v = self.pop()? as u32;
                if s >= 1 && s <= 3 {
                    let mask = 1u8 << (s - 1);
                    if v != 0 {
                        self.gs.instruct_control |= mask;
                    } else {
                        self.gs.instruct_control &= !mask;
                    }
                }
            }

            // ── PUSHB / PUSHW ────────────────────────────────────
            0xB0..=0xB7 => {
                // PUSHB[n] - push n+1 bytes
                let count = (opcode - 0xB0 + 1) as usize;
                if *ip + count > bytecode.len() {
                    return Err(HintError::UnexpectedEndOfBytecode);
                }
                for i in 0..count {
                    self.push(bytecode[*ip + i] as i32)?;
                }
                *ip += count;
            }
            0xB8..=0xBF => {
                // PUSHW[n] - push n+1 words (signed 16-bit)
                let count = (opcode - 0xB8 + 1) as usize;
                if *ip + count * 2 > bytecode.len() {
                    return Err(HintError::UnexpectedEndOfBytecode);
                }
                for i in 0..count {
                    let hi = bytecode[*ip + i * 2] as i8;
                    let lo = bytecode[*ip + i * 2 + 1];
                    let val = ((hi as i32) << 8) | (lo as i32);
                    self.push(val)?;
                }
                *ip += count * 2;
            }

            // ── MDRP ─────────────────────────────────────────────
            0xC0..=0xDF => {
                // MDRP[abcde] - move direct relative point
                self.op_mdrp(opcode)?;
            }

            // ── MIRP ─────────────────────────────────────────────
            0xE0..=0xFF => {
                // MIRP[abcde] - move indirect relative point
                self.op_mirp(opcode)?;
            }

            // Unused / reserved opcodes
            _ => {
                // Check IDEFs for this opcode
                if let Some(Some(idef)) = self.idefs.get(opcode as usize) {
                    let func = idef.bytecode.clone();
                    self.call_depth += 1;
                    self.execute(&func)?;
                    self.call_depth -= 1;
                }
                // Otherwise silently ignore (compatibility)
            }
        }
        Ok(())
    }

    // ── Stack helpers ────────────────────────────────────────────────

    fn push(&mut self, val: i32) -> Result<(), HintError> {
        if self.stack.len() >= self.max_stack {
            return Err(HintError::StackOverflow);
        }
        self.stack.push(val);
        Ok(())
    }

    fn pop(&mut self) -> Result<i32, HintError> {
        self.stack.pop().ok_or(HintError::StackUnderflow)
    }

    fn peek(&self) -> Result<i32, HintError> {
        self.stack.last().copied().ok_or(HintError::StackUnderflow)
    }

    fn read_cvt(&self, idx: u32) -> Result<i32, HintError> {
        self.cvt
            .get(idx as usize)
            .copied()
            .ok_or(HintError::InvalidCvtIndex(idx))
    }

    // ── Control flow helpers ─────────────────────────────────────────

    /// Skip bytecode until matching ELSE or EIF for an IF whose condition was false.
    fn skip_to_else_or_eif(
        &self,
        bytecode: &[u8],
        ip: &mut usize,
    ) -> Result<(), HintError> {
        let mut depth = 1u32;
        while *ip < bytecode.len() {
            let op = bytecode[*ip];
            *ip += 1;
            match op {
                0x58 => depth += 1, // nested IF
                0x59 => {
                    // EIF
                    depth -= 1;
                    if depth == 0 {
                        return Ok(());
                    }
                }
                0x1B => {
                    // ELSE
                    if depth == 1 {
                        return Ok(());
                    }
                }
                // Skip inline data for push instructions
                0x40 => {
                    // NPUSHB
                    if *ip >= bytecode.len() {
                        return Err(HintError::UnexpectedEndOfBytecode);
                    }
                    let n = bytecode[*ip] as usize;
                    *ip += 1 + n;
                }
                0x41 => {
                    // NPUSHW
                    if *ip >= bytecode.len() {
                        return Err(HintError::UnexpectedEndOfBytecode);
                    }
                    let n = bytecode[*ip] as usize;
                    *ip += 1 + n * 2;
                }
                0xB0..=0xB7 => *ip += (op - 0xB0 + 1) as usize,
                0xB8..=0xBF => *ip += ((op - 0xB8 + 1) * 2) as usize,
                _ => {}
            }
        }
        Err(HintError::UnexpectedEndOfBytecode)
    }

    /// Skip from ELSE to matching EIF.
    fn skip_else(
        &self,
        bytecode: &[u8],
        ip: &mut usize,
    ) -> Result<(), HintError> {
        let mut depth = 1u32;
        while *ip < bytecode.len() {
            let op = bytecode[*ip];
            *ip += 1;
            match op {
                0x58 => depth += 1, // nested IF
                0x59 => {
                    // EIF
                    depth -= 1;
                    if depth == 0 {
                        return Ok(());
                    }
                }
                // Skip inline data
                0x40 => {
                    if *ip >= bytecode.len() {
                        return Err(HintError::UnexpectedEndOfBytecode);
                    }
                    let n = bytecode[*ip] as usize;
                    *ip += 1 + n;
                }
                0x41 => {
                    if *ip >= bytecode.len() {
                        return Err(HintError::UnexpectedEndOfBytecode);
                    }
                    let n = bytecode[*ip] as usize;
                    *ip += 1 + n * 2;
                }
                0xB0..=0xB7 => *ip += (op - 0xB0 + 1) as usize,
                0xB8..=0xBF => *ip += ((op - 0xB8 + 1) * 2) as usize,
                _ => {}
            }
        }
        Err(HintError::UnexpectedEndOfBytecode)
    }

    /// Find matching ENDF for a FDEF/IDEF, advancing ip past it.
    fn find_endf(&self, bytecode: &[u8], ip: &mut usize) -> Result<usize, HintError> {
        let mut depth = 1u32;
        while *ip < bytecode.len() {
            let op = bytecode[*ip];
            *ip += 1;
            match op {
                0x2C | 0x89 => depth += 1, // nested FDEF or IDEF
                0x2D => {
                    // ENDF
                    depth -= 1;
                    if depth == 0 {
                        return Ok(*ip - 1); // position of ENDF
                    }
                }
                // Skip inline data
                0x40 => {
                    if *ip >= bytecode.len() {
                        return Err(HintError::UnexpectedEndOfBytecode);
                    }
                    let n = bytecode[*ip] as usize;
                    *ip += 1 + n;
                }
                0x41 => {
                    if *ip >= bytecode.len() {
                        return Err(HintError::UnexpectedEndOfBytecode);
                    }
                    let n = bytecode[*ip] as usize;
                    *ip += 1 + n * 2;
                }
                0xB0..=0xB7 => *ip += (op - 0xB0 + 1) as usize,
                0xB8..=0xBF => *ip += ((op - 0xB8 + 1) * 2) as usize,
                _ => {}
            }
        }
        Err(HintError::UnexpectedEndOfBytecode)
    }

    // ── Projection / freedom vector helpers ──────────────────────────

    /// Project a point onto the projection vector, returning F26Dot6 distance.
    fn project(&self, p: Point) -> i32 {
        let (px, py) = self.gs.projection_vector;
        ((p.x as i64 * px.to_bits() as i64 + p.y as i64 * py.to_bits() as i64 + 0x2000) >> 14)
            as i32
    }

    /// Project using the dual projection vector (for measuring original distances).
    fn dual_project(&self, p: Point) -> i32 {
        let (px, py) = self.gs.dual_projection_vector;
        ((p.x as i64 * px.to_bits() as i64 + p.y as i64 * py.to_bits() as i64 + 0x2000) >> 14)
            as i32
    }

    /// Move a point along the freedom vector by a given F26Dot6 distance.
    fn move_point(&mut self, zone: usize, point: usize, distance: i32) {
        // Dynamically grow zone if needed, with cap to prevent OOM
        if point >= self.zones[zone].current.len() {
            if point > 10_000 {
                return; // silently ignore bogus point indices
            }
            self.zones[zone].resize(point + 1);
        }

        let (fx, fy) = self.gs.freedom_vector;
        let (px, py) = self.gs.projection_vector;

        // Compute the dot product of freedom and projection vectors
        let dot = (fx.to_bits() as i64 * px.to_bits() as i64
            + fy.to_bits() as i64 * py.to_bits() as i64
            + 0x2000)
            >> 14;

        // FreeType guarantees F_dot_P is never zero: when it computes to 0 it
        // substitutes 0x4000 (1.0) so the point is still displaced and touched.
        let dot = if dot == 0 { 0x4000 } else { dot };

        // displacement = distance * freedom_vector / (freedom_vector · projection_vector)
        // Use FreeType-compatible signed division: convert to absolute values,
        // round with positive bias, then re-apply sign. This prevents
        // truncation-toward-zero errors that cause negative displacements
        // to under-apply by 1 F26Dot6 unit.
        let dx = ft_muldiv(distance as i64, fx.to_bits() as i64, dot) as i32;
        let dy = ft_muldiv(distance as i64, fy.to_bits() as i64, dot) as i32;

        self.zones[zone].current[point].x += dx;
        self.zones[zone].current[point].y += dy;

        // Set touched flags
        if fx.to_bits() != 0 {
            self.zones[zone].flags[point].insert(PointFlags::TOUCHED_X);
        }
        if fy.to_bits() != 0 {
            self.zones[zone].flags[point].insert(PointFlags::TOUCHED_Y);
        }
    }

    // ── Point / zone access helpers ──────────────────────────────────

    fn get_point(&mut self, zone: usize, index: u32) -> Result<Point, HintError> {
        let z = self.zones.get_mut(zone).ok_or(HintError::InvalidPointIndex(index))?;
        let i = index as usize;
        // Dynamically grow zone if needed (twilight zone may exceed maxp limits)
        // Cap at reasonable limit to prevent OOM from corrupted indices
        if i >= z.current.len() {
            if i > 10_000 {
                return Err(HintError::InvalidPointIndex(index));
            }
            z.resize(i + 1);
        }
        Ok(z.current[i])
    }

    fn get_original_point(&mut self, zone: usize, index: u32) -> Result<Point, HintError> {
        let z = self.zones.get_mut(zone).ok_or(HintError::InvalidPointIndex(index))?;
        let i = index as usize;
        if i >= z.original.len() {
            if i > 10_000 {
                return Err(HintError::InvalidPointIndex(index));
            }
            z.resize(i + 1);
        }
        Ok(z.original[i])
    }

    // ── Vector-from-line instructions ────────────────────────────────

    fn op_spvtl(&mut self, perpendicular: bool) -> Result<(), HintError> {
        // FreeType convention: top→p1 with zp2, lower→p2 with zp1
        // Vector direction: zp1[p2] - zp2[p1] (matching FreeType's DO_SPVTL)
        let p1_idx = self.pop()? as u32; // top of stack
        let p2_idx = self.pop()? as u32; // second
        let p2 = self.get_point(self.gs.zp1 as usize, p2_idx)?;
        let p1 = self.get_point(self.gs.zp2 as usize, p1_idx)?;
        let v = self.compute_vector_from_line(p1, p2, perpendicular);
        self.gs.projection_vector = v;
        self.gs.dual_projection_vector = v;
        Ok(())
    }

    fn op_sfvtl(&mut self, perpendicular: bool) -> Result<(), HintError> {
        // Same convention as SPVTL: top→p1/zp2, lower→p2/zp1
        let p1_idx = self.pop()? as u32;
        let p2_idx = self.pop()? as u32;
        let p2 = self.get_point(self.gs.zp1 as usize, p2_idx)?;
        let p1 = self.get_point(self.gs.zp2 as usize, p1_idx)?;
        self.gs.freedom_vector = self.compute_vector_from_line(p1, p2, perpendicular);
        Ok(())
    }

    fn op_sdpvtl(&mut self, perpendicular: bool) -> Result<(), HintError> {
        // Same convention as SPVTL: top→p1/zp2, lower→p2/zp1
        let p1_idx = self.pop()? as u32;
        let p2_idx = self.pop()? as u32;

        // Use current points for projection vector
        let p2 = self.get_point(self.gs.zp1 as usize, p2_idx)?;
        let p1 = self.get_point(self.gs.zp2 as usize, p1_idx)?;
        self.gs.projection_vector = self.compute_vector_from_line(p1, p2, perpendicular);

        // Use original points for dual projection vector
        let op2 = self.get_original_point(self.gs.zp1 as usize, p2_idx)?;
        let op1 = self.get_original_point(self.gs.zp2 as usize, p1_idx)?;
        self.gs.dual_projection_vector = self.compute_vector_from_line(op1, op2, perpendicular);

        Ok(())
    }

    fn compute_vector_from_line(
        &self,
        p1: Point,
        p2: Point,
        perpendicular: bool,
    ) -> (F2Dot14, F2Dot14) {
        let dx = (p2.x - p1.x) as f64;
        let dy = (p2.y - p1.y) as f64;
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1.0e-10 {
            return (F2Dot14::ONE, F2Dot14::ZERO);
        }
        let (nx, ny) = if perpendicular {
            (-dy / len, dx / len)
        } else {
            (dx / len, dy / len)
        };
        (F2Dot14::from_f64(nx), F2Dot14::from_f64(ny))
    }

    // ── Point movement instructions ──────────────────────────────────

    /// MDAP[a] — Move Direct Absolute Point.
    /// Spec: MS OpenType §tt_instructions, Apple TrueType RM §5.
    /// Touches point `p` in zp0; if `round` is set, rounds the projected
    /// coordinate to grid.  Sets rp0 = rp1 = p.
    ///
    /// **Twilight zone**: if zp0 == 0, the original coordinate is copied
    /// from the current coordinate *before* any movement so that later
    /// instructions (MIRP, IP) that read the original get a meaningful
    /// value instead of the initial zero.
    fn op_mdap(&mut self, round: bool) -> Result<(), HintError> {
        let p = self.pop()? as u32;
        let zp0 = self.gs.zp0 as usize;

        // Twilight zone: set original = current before projecting.
        // TODO: investigate correct twilight zone initialization
        if zp0 == 0 {
            let i = p as usize;
            self.zones[0].ensure_capacity(i + 1);
            self.zones[0].original[i] = self.zones[0].current[i];
        }

        let point = self.get_point(zp0, p)?;
        let cur_dist = self.project(point);

        let distance = if round {
            let rounded = self.gs.round(F26Dot6::from_bits(cur_dist));
            rounded.to_bits() - cur_dist
        } else {
            0
        };

        self.move_point(zp0, p as usize, distance);
        self.gs.rp0 = p;
        self.gs.rp1 = p;
        Ok(())
    }

    /// MIAP[a] — Move Indirect Absolute Point.
    /// Spec: MS OpenType §tt_instructions, Apple TrueType RM §5.
    /// Moves point `p` in zp0 to the CVT value (rounded if `a` bit set,
    /// subject to control_value_cut_in).  Sets rp0 = rp1 = p.
    ///
    /// **Twilight zone**: if zp0 == 0, the point's original *and* current
    /// coordinates are initialized from the CVT value along the freedom
    /// vector.  This is how the `prep` program creates twilight reference
    /// points that encode key font measurements (cap height, x-height,
    /// ascender, etc.).  Without this, twilight points stay at (0,0) and
    /// glyph programs that reference them via MIRP/IP get wrong distances.
    fn op_miap(&mut self, round: bool) -> Result<(), HintError> {
        let cvt_idx = self.pop()? as u32;
        let p = self.pop()? as u32;
        let zp0 = self.gs.zp0 as usize;

        let cvt_val = self.read_cvt(cvt_idx)?;

        // Twilight zone: initialize point from CVT value along freedom vector.
        // Per TrueType spec, MIAP in twilight zone sets original and current
        // coordinates from the CVT value. Disabled pending further investigation
        // as it regresses HelveticaNeue hinting (the prep program's twilight zone
        // setup interacts differently than expected).
        // TODO: investigate correct twilight zone initialization
        if zp0 == 0 {
            let (fx, fy) = self.gs.freedom_vector;
            let i = p as usize;
            self.zones[0].ensure_capacity(i + 1);
            let ox = ft_muldiv(cvt_val as i64, fx.to_bits() as i64, 0x4000) as i32;
            let oy = ft_muldiv(cvt_val as i64, fy.to_bits() as i64, 0x4000) as i32;
            self.zones[0].original[i] = Point { x: ox, y: oy };
            self.zones[0].current[i] = Point { x: ox, y: oy };
        }

        let point = self.get_point(zp0, p)?;
        let cur_dist = self.project(point);

        let distance = if round {
            let diff = (cvt_val - cur_dist).abs();
            let target = if diff <= self.gs.control_value_cut_in.to_bits() {
                cvt_val
            } else {
                cur_dist
            };
            let rounded = self.gs.round(F26Dot6::from_bits(target));
            rounded.to_bits() - cur_dist
        } else {
            cvt_val - cur_dist
        };

        self.move_point(zp0, p as usize, distance);
        self.gs.rp0 = p;
        self.gs.rp1 = p;
        Ok(())
    }

    /// MDRP[abcde] — Move Direct Relative Point.
    /// Spec: MS OpenType §tt_instructions, Apple TrueType RM §5.
    /// Moves point `p` (in zp1) relative to rp0 (in zp0) by the *measured*
    /// original distance (no CVT lookup), with optional rounding, minimum
    /// distance, and single-width overrides.
    fn op_mdrp(&mut self, opcode: u8) -> Result<(), HintError> {
        let p = self.pop()? as u32;
        let zp0 = self.gs.zp0 as usize;
        let zp1 = self.gs.zp1 as usize;

        // Flags from opcode bits: 0xC0 + [set_rp0, respect_min_dist, round, _, _]
        let set_rp0 = (opcode >> 4) & 1 != 0;
        let respect_min_dist = (opcode >> 3) & 1 != 0;
        let do_round = (opcode >> 2) & 1 != 0;

        // Measure original distance between rp0 and point (before any adjustments)
        let rp0_orig = self.get_original_point(zp0, self.gs.rp0)?;
        let p_orig = self.get_original_point(zp1, p)?;
        let orig_dist = self.dual_project(Point {
            x: p_orig.x - rp0_orig.x,
            y: p_orig.y - rp0_orig.y,
        });
        let mut dist = orig_dist;

        // Apply single width
        let swv = self.gs.single_width_value.to_bits();
        if swv != 0 {
            let swci = self.gs.single_width_cut_in.to_bits();
            if (dist - swv).abs() < swci {
                dist = if dist >= 0 { swv } else { -swv };
            }
        }

        if do_round {
            dist = self.gs.round(F26Dot6::from_bits(dist)).to_bits();
        }

        if respect_min_dist {
            let min_dist = self.gs.minimum_distance.to_bits();
            if dist >= 0 {
                if dist < min_dist {
                    // When dist rounds to zero from a negative original distance,
                    // apply minimum in the ORIGINAL direction to avoid sign flip.
                    dist = if self.fix_min_distance_sign && dist == 0 && orig_dist < 0 { -min_dist } else { min_dist };
                }
            } else if dist > -min_dist {
                dist = -min_dist;
            }
        }

        // Current position of reference point
        let rp0_cur = self.get_point(zp0, self.gs.rp0)?;
        let p_cur = self.get_point(zp1, p)?;
        let cur_dist = self.project(Point {
            x: p_cur.x - rp0_cur.x,
            y: p_cur.y - rp0_cur.y,
        });

        self.move_point(zp1, p as usize, dist - cur_dist);

        self.gs.rp1 = self.gs.rp0;
        self.gs.rp2 = p;
        if set_rp0 {
            self.gs.rp0 = p;
        }
        Ok(())
    }

    /// MIRP[abcde] — Move Indirect Relative Point.
    /// Spec: MS OpenType §tt_instructions, Apple TrueType RM §5.
    /// Moves point `p` (in zp1) relative to rp0 (in zp0) by a CVT distance,
    /// with optional rounding, minimum distance, and auto-flip.
    ///
    /// **Twilight zone**: if zp1 == 0, the target point's original and current
    /// coordinates are set to rp0_orig + CVT distance along the freedom vector.
    /// This initializes the twilight point so that orig_dist calculations work.
    fn op_mirp(&mut self, opcode: u8) -> Result<(), HintError> {
        let cvt_idx = self.pop()? as u32;
        let p = self.pop()? as u32;
        let zp0 = self.gs.zp0 as usize;
        let zp1 = self.gs.zp1 as usize;

        let set_rp0 = (opcode >> 4) & 1 != 0;
        let respect_min_dist = (opcode >> 3) & 1 != 0;
        let do_round = (opcode >> 2) & 1 != 0;

        let cvt_val = if cvt_idx < self.cvt.len() as u32 {
            self.cvt[cvt_idx as usize]
        } else {
            0
        };

        // Twilight zone: initialize point from rp0_orig + CVT along freedom vector.
        // TODO: investigate correct twilight zone initialization
        if zp1 == 0 {
            let (fx, fy) = self.gs.freedom_vector;
            let rp0_orig = self.get_original_point(zp0, self.gs.rp0)?;
            let i = p as usize;
            self.zones[0].ensure_capacity(i + 1);
            let ox = rp0_orig.x + ft_muldiv(cvt_val as i64, fx.to_bits() as i64, 0x4000) as i32;
            let oy = rp0_orig.y + ft_muldiv(cvt_val as i64, fy.to_bits() as i64, 0x4000) as i32;
            self.zones[0].original[i] = Point { x: ox, y: oy };
            self.zones[0].current[i] = Point { x: ox, y: oy };
        }

        // Measure original distance
        let rp0_orig = self.get_original_point(zp0, self.gs.rp0)?;
        let p_orig = self.get_original_point(zp1, p)?;
        let orig_dist = self.dual_project(Point {
            x: p_orig.x - rp0_orig.x,
            y: p_orig.y - rp0_orig.y,
        });

        let mut dist = cvt_val;

        // Auto flip
        if self.gs.auto_flip {
            if (orig_dist >= 0) != (dist >= 0) {
                dist = -dist;
            }
        }

        // Apply single width
        let swv = self.gs.single_width_value.to_bits();
        if swv != 0 {
            let swci = self.gs.single_width_cut_in.to_bits();
            if (dist - swv).abs() < swci {
                dist = if dist >= 0 { swv } else { -swv };
            }
        }

        // CVT cut-in: if actual distance is too far from CVT value, use actual.
        // FreeType applies this only when reference and target share a zone
        // (gep0 == gep1) and independently of the rounding bit.
        let cvt_ci = self.gs.control_value_cut_in.to_bits();
        if zp0 == zp1 && (dist - orig_dist).abs() > cvt_ci {
            dist = orig_dist;
        }

        if do_round {
            dist = self.gs.round(F26Dot6::from_bits(dist)).to_bits();
        }

        if respect_min_dist {
            let min_dist = self.gs.minimum_distance.to_bits();
            if dist >= 0 {
                if dist < min_dist {
                    // When dist rounds to zero from a negative original distance,
                    // apply minimum in the ORIGINAL direction to avoid sign flip.
                    dist = if self.fix_min_distance_sign && dist == 0 && orig_dist < 0 { -min_dist } else { min_dist };
                }
            } else if dist > -min_dist {
                dist = -min_dist;
            }
        }

        // Move point
        let rp0_cur = self.get_point(zp0, self.gs.rp0)?;
        let p_cur = self.get_point(zp1, p)?;
        let cur_dist = self.project(Point {
            x: p_cur.x - rp0_cur.x,
            y: p_cur.y - rp0_cur.y,
        });

        self.move_point(zp1, p as usize, dist - cur_dist);

        self.gs.rp1 = self.gs.rp0;
        self.gs.rp2 = p;
        if set_rp0 {
            self.gs.rp0 = p;
        }
        Ok(())
    }

    /// MSIRP[a] — Move Stack Indirect Relative Point.
    /// Spec: MS OpenType §tt_instructions.
    /// Moves point `p` (in zp1) so its distance from rp0 (in zp0) along
    /// the projection vector equals `dist` (popped from stack in F26Dot6).
    ///
    /// **Twilight zone**: if zp1 == 0, initialize point from rp0_orig.
    fn op_msirp(&mut self, set_rp0: bool) -> Result<(), HintError> {
        let dist = self.pop()?; // F26Dot6
        let p = self.pop()? as u32;
        let zp0 = self.gs.zp0 as usize;
        let zp1 = self.gs.zp1 as usize;

        // Twilight zone: initialize point from rp0 original position
        // TODO: investigate correct twilight zone initialization
        if zp1 == 0 {
            let rp0_orig = self.get_original_point(zp0, self.gs.rp0)?;
            let i = p as usize;
            self.zones[0].ensure_capacity(i + 1);
            self.zones[0].original[i] = rp0_orig;
            self.zones[0].current[i] = rp0_orig;
        }

        let rp0_cur = self.get_point(zp0, self.gs.rp0)?;
        let p_cur = self.get_point(zp1, p)?;
        let cur_dist = self.project(Point {
            x: p_cur.x - rp0_cur.x,
            y: p_cur.y - rp0_cur.y,
        });

        self.move_point(zp1, p as usize, dist - cur_dist);

        self.gs.rp1 = self.gs.rp0;
        self.gs.rp2 = p;
        if set_rp0 {
            self.gs.rp0 = p;
        }
        Ok(())
    }

    fn op_alignrp(&mut self) -> Result<(), HintError> {
        let loop_count = self.gs.loop_value;
        self.gs.loop_value = 1;

        let zp0 = self.gs.zp0 as usize;
        let zp1 = self.gs.zp1 as usize;
        let rp0 = self.gs.rp0;

        for _ in 0..loop_count {
            let p = self.pop()? as u32;
            let rp0_cur = self.get_point(zp0, rp0)?;
            let p_cur = self.get_point(zp1, p)?;
            let cur_dist = self.project(Point {
                x: p_cur.x - rp0_cur.x,
                y: p_cur.y - rp0_cur.y,
            });
            self.move_point(zp1, p as usize, -cur_dist);
        }
        Ok(())
    }

    fn op_alignpts(&mut self) -> Result<(), HintError> {
        let p2 = self.pop()? as u32;
        let p1 = self.pop()? as u32;
        let zp0 = self.gs.zp0 as usize;
        let zp1 = self.gs.zp1 as usize;

        let p1_cur = self.get_point(zp1, p1)?;
        let p2_cur = self.get_point(zp0, p2)?;
        let dist = self.project(Point {
            x: p2_cur.x - p1_cur.x,
            y: p2_cur.y - p1_cur.y,
        });

        let half = dist / 2;
        self.move_point(zp1, p1 as usize, half);
        self.move_point(zp0, p2 as usize, -half);
        Ok(())
    }

    fn op_shp(&mut self, use_rp1: bool) -> Result<(), HintError> {
        let loop_count = self.gs.loop_value;
        self.gs.loop_value = 1;

        let (rp, rp_zone) = if use_rp1 {
            (self.gs.rp1, self.gs.zp0 as usize)
        } else {
            (self.gs.rp2, self.gs.zp1 as usize)
        };
        let zp2 = self.gs.zp2 as usize;

        // Compute displacement: difference between current and original of rp
        let rp_cur = self.get_point(rp_zone, rp)?;
        let rp_orig = self.get_original_point(rp_zone, rp)?;
        let displacement = self.project(Point {
            x: rp_cur.x - rp_orig.x,
            y: rp_cur.y - rp_orig.y,
        });

        for _ in 0..loop_count {
            let p = self.pop()? as u32;
            self.move_point(zp2, p as usize, displacement);
        }
        Ok(())
    }

    fn op_shc(&mut self, use_rp1: bool) -> Result<(), HintError> {
        let contour = self.pop()? as usize;

        let (rp, rp_zone) = if use_rp1 {
            (self.gs.rp1, self.gs.zp0 as usize)
        } else {
            (self.gs.rp2, self.gs.zp1 as usize)
        };
        let zp2 = self.gs.zp2 as usize;

        let rp_cur = self.get_point(rp_zone, rp)?;
        let rp_orig = self.get_original_point(rp_zone, rp)?;
        let displacement = self.project(Point {
            x: rp_cur.x - rp_orig.x,
            y: rp_cur.y - rp_orig.y,
        });

        // Get contour point range
        let start = if contour == 0 {
            0
        } else {
            self.zones[zp2]
                .contour_ends
                .get(contour - 1)
                .map(|&e| e as usize + 1)
                .unwrap_or(0)
        };
        let end = self.zones[zp2]
            .contour_ends
            .get(contour)
            .map(|&e| e as usize + 1)
            .unwrap_or(self.zones[zp2].len());

        for i in start..end {
            if i as u32 != rp {
                self.move_point(zp2, i, displacement);
            }
        }
        Ok(())
    }

    fn op_shz(&mut self, use_rp1: bool) -> Result<(), HintError> {
        let zone_idx = self.pop()? as u32;
        if zone_idx > 1 {
            return Err(HintError::InvalidZone(zone_idx));
        }

        let (rp, rp_zone) = if use_rp1 {
            (self.gs.rp1, self.gs.zp0 as usize)
        } else {
            (self.gs.rp2, self.gs.zp1 as usize)
        };

        let rp_cur = self.get_point(rp_zone, rp)?;
        let rp_orig = self.get_original_point(rp_zone, rp)?;
        let displacement = self.project(Point {
            x: rp_cur.x - rp_orig.x,
            y: rp_cur.y - rp_orig.y,
        });

        // FreeType's Ins_SHZ shifts the whole zone via Move_Zp2_Point(..., FALSE):
        // the points are moved but NOT marked touched, so a later IUP still
        // re-interpolates them. Mirror move_point's freedom-vector math inline
        // (F_dot_P clamped like FreeType) without inserting the touch flags.
        let (fx, fy) = self.gs.freedom_vector;
        let (px, py) = self.gs.projection_vector;
        let dot = {
            let d = (fx.to_bits() as i64 * px.to_bits() as i64
                + fy.to_bits() as i64 * py.to_bits() as i64
                + 0x2000)
                >> 14;
            if d == 0 {
                0x4000
            } else {
                d
            }
        };
        let dx = ft_muldiv(displacement as i64, fx.to_bits() as i64, dot) as i32;
        let dy = ft_muldiv(displacement as i64, fy.to_bits() as i64, dot) as i32;

        let z = zone_idx as usize;
        let n = self.zones[z].len();
        for i in 0..n {
            self.zones[z].current[i].x += dx;
            self.zones[z].current[i].y += dy;
        }
        Ok(())
    }

    fn op_shpix(&mut self) -> Result<(), HintError> {
        let dist = self.pop()?; // F26Dot6 pixels
        let loop_count = self.gs.loop_value;
        self.gs.loop_value = 1;

        let zp2 = self.gs.zp2 as usize;
        let (fx, fy) = self.gs.freedom_vector;

        for _ in 0..loop_count {
            let p = self.pop()? as u32;
            let i = p as usize;
            self.zones[zp2].ensure_capacity(i + 1);
            // Move directly along freedom vector (no projection)
            // Use ft_muldiv for correct signed rounding (FreeType's TT_MulFix14)
            let dx = ft_muldiv(dist as i64, fx.to_bits() as i64, 0x4000) as i32;
            let dy = ft_muldiv(dist as i64, fy.to_bits() as i64, 0x4000) as i32;
            self.zones[zp2].current[i].x += dx;
            self.zones[zp2].current[i].y += dy;

            if fx.to_bits() != 0 {
                self.zones[zp2].flags[i].insert(PointFlags::TOUCHED_X);
            }
            if fy.to_bits() != 0 {
                self.zones[zp2].flags[i].insert(PointFlags::TOUCHED_Y);
            }
        }
        Ok(())
    }

    /// IP — Interpolate Point.
    /// Spec: MS OpenType §tt_instructions, Apple TrueType RM §5.
    /// For each point popped from the stack, interpolates its position
    /// along the projection vector so that its relative placement between
    /// rp1 and rp2 is preserved from the original outline to the current
    /// (hinted) outline.  The interpolation factor is computed from
    /// original distances (using dual_project), applied to the current
    /// range, then the point is moved along the freedom vector.
    fn op_ip(&mut self) -> Result<(), HintError> {
        let loop_count = self.gs.loop_value;
        self.gs.loop_value = 1;

        let zp0 = self.gs.zp0 as usize;
        let zp1 = self.gs.zp1 as usize;
        let zp2 = self.gs.zp2 as usize;

        // Get reference points (original and current)
        let rp1_orig = self.get_original_point(zp0, self.gs.rp1)?;
        let rp2_orig = self.get_original_point(zp1, self.gs.rp2)?;
        let rp1_cur = self.get_point(zp0, self.gs.rp1)?;
        let rp2_cur = self.get_point(zp1, self.gs.rp2)?;

        let orig_range = self.dual_project(Point {
            x: rp2_orig.x - rp1_orig.x,
            y: rp2_orig.y - rp1_orig.y,
        });
        let cur_range = self.project(Point {
            x: rp2_cur.x - rp1_cur.x,
            y: rp2_cur.y - rp1_cur.y,
        });

        for _ in 0..loop_count {
            let p = self.pop()? as u32;
            let p_orig = self.get_original_point(zp2, p)?;
            let p_cur = self.get_point(zp2, p)?;

            let orig_dist = self.dual_project(Point {
                x: p_orig.x - rp1_orig.x,
                y: p_orig.y - rp1_orig.y,
            });

            let new_dist = if orig_range != 0 {
                ft_muldiv(cur_range as i64, orig_dist as i64, orig_range as i64) as i32
            } else {
                orig_dist
            };

            let cur_dist = self.project(Point {
                x: p_cur.x - rp1_cur.x,
                y: p_cur.y - rp1_cur.y,
            });

            self.move_point(zp2, p as usize, new_dist - cur_dist);
        }
        Ok(())
    }

    /// IUP[a] — Interpolate Untouched Points.
    /// Spec: MS OpenType §tt_instructions, Apple TrueType RM §5.
    /// Final pass: for each contour, walks between consecutive touched
    /// points, interpolating untouched points to preserve their relative
    /// position in the original outline.  Uses `orus` (unscaled font-unit
    /// coordinates) for interpolation factors to avoid F26Dot6 rounding
    /// errors, matching FreeType's approach.
    ///
    /// Points outside the touched range are shifted by the nearest touched
    /// point's delta; points between two touched points are linearly
    /// interpolated using FreeType's FT_DivFix/FT_MulFix for precision.
    fn op_iup(&mut self, axis: u8) -> Result<(), HintError> {
        let n_points = self.zones[1].len();
        if n_points == 0 {
            return Ok(());
        }

        let touched_flag = if axis == 1 {
            PointFlags::TOUCHED_X
        } else {
            PointFlags::TOUCHED_Y
        };

        // Collect all (contour_start, contour_end, touched_points) first
        // to avoid borrowing self.zones[1] while calling self.iup_interp.
        let mut work: Vec<(usize, usize, Vec<usize>)> = Vec::new();
        let mut contour_start = 0usize;
        let contour_ends: Vec<u16> = self.zones[1].contour_ends.clone();

        for &contour_end_u16 in &contour_ends {
            let contour_end = contour_end_u16 as usize;
            if contour_end >= n_points {
                break;
            }

            let mut touched_points: Vec<usize> = Vec::new();
            for i in contour_start..=contour_end {
                if self.zones[1].flags[i].contains(touched_flag) {
                    touched_points.push(i);
                }
            }

            if !touched_points.is_empty() {
                work.push((contour_start, contour_end, touched_points));
            }
            contour_start = contour_end + 1;
        }

        // Now perform interpolation
        for (contour_start, contour_end, touched_points) in &work {
            let n_touched = touched_points.len();
            for t in 0..n_touched {
                let t1_idx = touched_points[t];
                let t2_idx = touched_points[(t + 1) % n_touched];
                self.iup_interp(*contour_start, *contour_end, t1_idx, t2_idx, axis);
            }
        }
        Ok(())
    }

    fn iup_interp(
        &mut self,
        contour_start: usize,
        contour_end: usize,
        t1_idx: usize,
        t2_idx: usize,
        axis: u8,
    ) {
        let touched_flag = if axis == 1 {
            PointFlags::TOUCHED_X
        } else {
            PointFlags::TOUCHED_Y
        };

        // Walk from t1 to t2 (wrapping around contour), interpolating untouched points
        let contour_len = contour_end - contour_start + 1;

        let get_coord = |p: &Point| -> i32 {
            if axis == 1 { p.x } else { p.y }
        };

        // Use unscaled coordinates (orus) for interpolation factors — matches FreeType.
        // Unscaled integers avoid F26Dot6 rounding errors in range/factor computation.
        let t1_orus = get_coord(&self.zones[1].orus[t1_idx]);
        let t1_cur = get_coord(&self.zones[1].current[t1_idx]);
        let t2_orus = get_coord(&self.zones[1].orus[t2_idx]);
        let t2_cur = get_coord(&self.zones[1].current[t2_idx]);
        let t1_orig = get_coord(&self.zones[1].original[t1_idx]);
        let t2_orig = get_coord(&self.zones[1].original[t2_idx]);

        let delta1 = t1_cur - t1_orig;
        let delta2 = t2_cur - t2_orig;

        // Walk from t1+1 to t2-1 (wrapping)
        if contour_len <= 1 {
            return;
        }

        let mut i = t1_idx;
        loop {
            // Advance to next point in contour (wrapping)
            i = if i == contour_end {
                contour_start
            } else {
                i + 1
            };

            if i == t2_idx {
                break;
            }

            if self.zones[1].flags[i].contains(touched_flag) {
                continue;
            }

            // Use unscaled coordinates for interpolation (FreeType uses orus)
            let orus_i = get_coord(&self.zones[1].orus[i]);

            let new_coord = if t1_orus == t2_orus {
                // Both reference points are at the same position: shift
                let cur = get_coord(&self.zones[1].current[i]);
                cur + delta1
            } else {
                // Interpolate using unscaled coordinates for range/factor
                let lo_orus = t1_orus.min(t2_orus);
                let hi_orus = t1_orus.max(t2_orus);
                let lo_cur = if t1_orus < t2_orus { t1_cur } else { t2_cur };
                let hi_cur = if t1_orus < t2_orus { t2_cur } else { t1_cur };
                let lo_delta = if t1_orus < t2_orus { delta1 } else { delta2 };
                let hi_delta = if t1_orus < t2_orus { delta2 } else { delta1 };

                if orus_i <= lo_orus {
                    let orig = get_coord(&self.zones[1].original[i]);
                    orig + lo_delta
                } else if orus_i >= hi_orus {
                    let orig = get_coord(&self.zones[1].original[i]);
                    orig + hi_delta
                } else {
                    let range = (hi_orus - lo_orus) as i64;
                    let factor = (orus_i - lo_orus) as i64;
                    let scale = ft_divfix((hi_cur - lo_cur) as i64, range);
                    lo_cur + ft_mulfix(factor, scale) as i32
                }
            };

            if axis == 1 {
                self.zones[1].current[i].x = new_coord;
            } else {
                self.zones[1].current[i].y = new_coord;
            }
        }
    }

    // ── Coordinate / measurement ─────────────────────────────────────

    fn op_gc(&mut self, use_original: bool) -> Result<(), HintError> {
        let p = self.pop()? as u32;
        let zp2 = self.gs.zp2 as usize;
        let point = if use_original {
            self.get_original_point(zp2, p)?
        } else {
            self.get_point(zp2, p)?
        };
        let val = self.project(point);
        self.push(val)?;
        Ok(())
    }

    fn op_scfs(&mut self) -> Result<(), HintError> {
        let val = self.pop()?; // F26Dot6
        let p = self.pop()? as u32;
        let zp2 = self.gs.zp2 as usize;

        let point = self.get_point(zp2, p)?;
        let cur = self.project(point);
        self.move_point(zp2, p as usize, val - cur);
        Ok(())
    }

    fn op_md(&mut self, use_original: bool) -> Result<(), HintError> {
        let p2 = self.pop()? as u32;
        let p1 = self.pop()? as u32;

        let dist = if use_original {
            let p1_pt = self.get_original_point(self.gs.zp0 as usize, p1)?;
            let p2_pt = self.get_original_point(self.gs.zp1 as usize, p2)?;
            self.dual_project(Point {
                x: p2_pt.x - p1_pt.x,
                y: p2_pt.y - p1_pt.y,
            })
        } else {
            let p1_pt = self.get_point(self.gs.zp0 as usize, p1)?;
            let p2_pt = self.get_point(self.gs.zp1 as usize, p2)?;
            self.project(Point {
                x: p2_pt.x - p1_pt.x,
                y: p2_pt.y - p1_pt.y,
            })
        };

        self.push(dist)?;
        Ok(())
    }

    // ── Intersection ─────────────────────────────────────────────────

    fn op_isect(&mut self) -> Result<(), HintError> {
        let b1 = self.pop()? as u32;
        let b0 = self.pop()? as u32;
        let a1 = self.pop()? as u32;
        let a0 = self.pop()? as u32;
        let p = self.pop()? as u32;

        // Per TrueType spec: a0,a1 define line A using zp0; b0,b1 define line B using zp1
        let pa0 = self.get_point(self.gs.zp0 as usize, a0)?;
        let pa1 = self.get_point(self.gs.zp0 as usize, a1)?;
        let pb0 = self.get_point(self.gs.zp1 as usize, b0)?;
        let pb1 = self.get_point(self.gs.zp1 as usize, b1)?;

        // Line A: pa0 to pa1, Line B: pb0 to pb1
        let dax = (pa1.x - pa0.x) as i64;
        let day = (pa1.y - pa0.y) as i64;
        let dbx = (pb1.x - pb0.x) as i64;
        let dby = (pb1.y - pb0.y) as i64;

        let denom = dax * dby - day * dbx;
        // FreeType Ins_ISECT rejects grazing (near-parallel) intersections with
        // a RELATIVE test: discriminant ∝ |da||db|sin(angle) (== denom here) and
        // dotproduct ∝ |da||db|cos(angle). When 19*|discriminant| <= |dotproduct|
        // the angle is too shallow, so snap to the midpoint instead of
        // extrapolating a far-off point from a tiny nonzero determinant.
        let dotproduct = dax * dbx + day * dby;

        let zp2 = self.gs.zp2 as usize;
        let i = p as usize;
        self.zones[zp2].ensure_capacity(i + 1);

        if 19 * denom.abs() <= dotproduct.abs() {
            // Lines are parallel or near-parallel; use midpoint of endpoints
            self.zones[zp2].current[i].x = (pa0.x + pa1.x + pb0.x + pb1.x) / 4;
            self.zones[zp2].current[i].y = (pa0.y + pa1.y + pb0.y + pb1.y) / 4;
        } else {
            let dpx = (pb0.x - pa0.x) as i64;
            let dpy = (pb0.y - pa0.y) as i64;
            let numer = dpx * dby - dpy * dbx;
            let t = ft_muldiv(numer, 64, denom);

            self.zones[zp2].current[i].x = pa0.x + ft_muldiv(dax, t, 64) as i32;
            self.zones[zp2].current[i].y = pa0.y + ft_muldiv(day, t, 64) as i32;
        }

        self.zones[zp2].flags[i].insert(PointFlags::TOUCHED_X | PointFlags::TOUCHED_Y);
        Ok(())
    }

    // ── Flip ─────────────────────────────────────────────────────────

    fn op_flippt(&mut self) -> Result<(), HintError> {
        let loop_count = self.gs.loop_value;
        self.gs.loop_value = 1;

        for _ in 0..loop_count {
            let p = self.pop()? as usize;
            // FLIPPT uses the glyph zone (zone 1), not zp0 - per TrueType spec
            // "Flips a point to on-curve or off-curve" in the glyph outline zone
            if let Some(flags) = self.zones[1].flags.get_mut(p) {
                flags.toggle(PointFlags::ON_CURVE);
            }
        }
        Ok(())
    }

    // ── Delta instructions ───────────────────────────────────────────
    //
    // Spec: Apple TrueType Reference Manual §5, Table 5 (magnitude encoding).
    // Stack pop order: n, then n pairs — the point/cvt index is on TOP (popped
    // first), the arg (ppem/magnitude) byte is below it (FreeType Ins_DELTAP:
    // point = stack[args+1], spec byte = stack[args]).
    // Magnitude encoding: bits 3-0 of arg:
    //   0=-8 steps, 1=-7, ..., 7=-1, 8=+1, 9=+2, ..., 15=+8
    // Step size: 1 / (1 << delta_shift) pixels (typically 1/8 px at shift=3).
    // DELTAP1/2/3 differ by range offset: 0, +16, +32 added to delta_base.
    // DELTAC1/2/3 modify CVT entries instead of moving points.

    /// DELTAP[n] — Delta Exception P.
    /// Moves point by a small amount at a specific ppem, for pixel-level tuning.
    fn op_deltap(
        &mut self,
        range: u8,
        _bytecode: &[u8],
        _ip: &mut usize,
    ) -> Result<(), HintError> {
        let n = self.pop()? as u32;
        let zp0 = self.gs.zp0 as usize;
        let delta_base = self.gs.delta_base as i32;
        let delta_shift = self.gs.delta_shift as i32;

        let range_offset = match range {
            1 => 0,
            2 => 16,
            3 => 32,
            _ => 0,
        };

        for _ in 0..n {
            // Point index is on top of the stack (FreeType Ins_DELTAP), then arg.
            let point_idx = self.pop()? as u32;
            let arg = self.pop()? as u32;

            let ppem_offset = ((arg >> 4) & 0x0F) as i32;
            let target_ppem = delta_base + range_offset + ppem_offset;

            if target_ppem == self.ppem as i32 {
                let magnitude = (arg & 0x0F) as i32;
                let delta = if magnitude < 8 {
                    // Spec: selector 0=-8, 1=-7, ..., 7=-1
                    -(8 - magnitude)
                } else {
                    // Spec: selector 8=+1, 9=+2, ..., 15=+8
                    magnitude - 7
                };
                // Scale by 1 / (1 << delta_shift)
                let scaled = if delta_shift > 0 {
                    delta * 64 / (1 << delta_shift)
                } else {
                    delta * 64
                };
                self.move_point(zp0, point_idx as usize, scaled);
            }
        }
        Ok(())
    }

    fn op_deltac(&mut self, range: u8) -> Result<(), HintError> {
        let n = self.pop()? as u32;
        let delta_base = self.gs.delta_base as i32;
        let delta_shift = self.gs.delta_shift as i32;

        let range_offset = match range {
            1 => 0,
            2 => 16,
            3 => 32,
            _ => 0,
        };

        for _ in 0..n {
            // TEST: try reversed pop order (cvt_idx first, then arg)
            let cvt_idx = self.pop()? as u32;
            let arg = self.pop()? as u32;

            let ppem_offset = ((arg >> 4) & 0x0F) as i32;
            let target_ppem = delta_base + range_offset + ppem_offset;

            if target_ppem == self.ppem as i32 {
                let magnitude = (arg & 0x0F) as i32;
                let delta = if magnitude < 8 {
                    // Spec: selector 0=-8, 1=-7, ..., 7=-1
                    -(8 - magnitude)
                } else {
                    // Spec: selector 8=+1, 9=+2, ..., 15=+8
                    magnitude - 7
                };
                let scaled = if delta_shift > 0 {
                    delta * 64 / (1 << delta_shift)
                } else {
                    delta * 64
                };

                let i = cvt_idx as usize;
                if i < self.cvt.len() {
                    self.cvt[i] += scaled;
                }
            }
        }
        Ok(())
    }

    // ── GETINFO ──────────────────────────────────────────────────────

    fn op_getinfo(&mut self) -> Result<(), HintError> {
        let selector = self.pop()? as u32;
        let mut result = 0u32;

        // Bit 0: engine version
        if selector & 1 != 0 {
            // Return version 40 (Windows DirectWrite / modern rasterizer).
            result |= 40;
        }

        // Bit 1: glyph rotated (we don't rotate, so false)
        // Bit 2: glyph stretched (we don't stretch, so false)

        // Bit 3: font variations active
        // (not currently supported in our interpreter)

        // Bit 5: grayscale rendering — we DO grayscale AA
        if selector & (1 << 5) != 0 {
            result |= 1 << 12; // grayscale bit
        }

        // Bit 6: ClearType enabled
        if selector & (1 << 6) != 0 {
            result |= 1 << 13; // ClearType enabled
        }

        self.push(result as i32)?;
        Ok(())
    }
}

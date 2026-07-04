//! TrueType bytecode hinting (grid-fitting) interpreter.
//!
//! This module implements the TrueType instruction set interpreter that
//! executes bytecode embedded in font programs (`fpgm`, `prep`) and
//! individual glyph instructions to snap glyph outlines to the pixel grid.
//!
//! # Usage
//!
//! ```ignore
//! use allsorts::hinting::{HintInstance, HintedGlyph};
//! use allsorts::tables::FontTableProvider;
//!
//! // 1. Create a hint instance from font tables (runs fpgm)
//! let instance = HintInstance::new(&font_table_provider)?;
//!
//! // 2. Set size (runs prep, scales CVT)
//! instance.set_size(16.0, 96)?;
//!
//! // 3. Hint individual glyphs
//! let hinted = instance.hint_simple_glyph(&glyph, &outline)?;
//! // Use hinted.points for rasterization
//! ```

pub mod f26dot6;
pub mod graphics_state;
pub mod interpreter;

pub use f26dot6::{F26Dot6, F2Dot14};
pub use interpreter::{HintError, Interpreter, Point, Zone};

use crate::binary::read::ReadScope;
use crate::tables::{CvtTable, FontTableProvider, MaxpTable};
use crate::tag;

/// High-level hinting state for a font.
///
/// Created once per font. Runs `fpgm` at creation time to populate
/// function definitions. Call `set_size` to prepare for a specific ppem.
pub struct HintInstance {
    pub interpreter: Interpreter,
    fpgm_executed: bool,
    prep_bytecode: Vec<u8>,
    cvt_funits: Vec<i16>,
    current_ppem: Option<u16>,
}

impl HintInstance {
    /// Create a new hinting instance from font table data.
    ///
    /// Parses `maxp`, `cvt`, `fpgm`, and `prep` tables, then executes `fpgm`
    /// to populate function definitions.
    ///
    /// Returns `None` if the font has no TrueType hinting data.
    pub fn new(provider: &dyn FontTableProvider) -> Result<Option<Self>, HintError> {
        // Read maxp table
        let maxp = match provider.table_data(tag::MAXP) {
            Ok(Some(data)) => {
                let scope = ReadScope::new(&data);
                match scope.read::<MaxpTable>() {
                    Ok(maxp) => maxp,
                    Err(_) => return Ok(None),
                }
            }
            _ => return Ok(None),
        };

        let maxp_v1 = match &maxp.version1_sub_table {
            Some(v1) => v1,
            None => return Ok(None), // CFF font, no TrueType hinting
        };

        // Read CVT table (optional)
        let cvt_funits: Vec<i16> = match provider.table_data(tag::CVT) {
            Ok(Some(data)) => {
                let len = data.len() as u32;
                let scope = ReadScope::new(&data);
                match scope.ctxt().read_dep::<CvtTable<'_>>(len) {
                    Ok(cvt) => {
                        let mut values = Vec::with_capacity(cvt.values.len());
                        for i in 0..cvt.values.len() {
                            if let Some(v) = cvt.values.get_item(i) {
                                values.push(v);
                            }
                        }
                        values
                    }
                    Err(_) => Vec::new(),
                }
            }
            _ => Vec::new(),
        };

        // Read fpgm bytecode (optional)
        let fpgm_bytecode: Vec<u8> = match provider.table_data(tag::FPGM) {
            Ok(Some(data)) => data.into_owned(),
            _ => Vec::new(),
        };

        // Read prep bytecode (optional)
        let prep_bytecode: Vec<u8> = match provider.table_data(tag::PREP) {
            Ok(Some(data)) => data.into_owned(),
            _ => Vec::new(),
        };

        // Create interpreter
        let mut interpreter = Interpreter::new(
            maxp_v1.max_stack_elements,
            maxp_v1.max_storage,
            maxp_v1.max_function_defs,
            maxp_v1.max_instruction_defs,
            maxp_v1.max_twilight_points,
            0, // units_per_em set later
        );

        // Read head table for units_per_em
        if let Ok(Some(head_data)) = provider.table_data(tag::HEAD) {
            if head_data.len() >= 20 {
                let upem = u16::from_be_bytes([head_data[18], head_data[19]]);
                interpreter.units_per_em = upem;
            }
        }

        // Execute fpgm to populate function definitions
        let mut fpgm_executed = false;
        if !fpgm_bytecode.is_empty() {
            match interpreter.execute_fpgm(&fpgm_bytecode) {
                Ok(()) => fpgm_executed = true,
                Err(_e) => {
                    // fpgm execution failed; continue without hinting functions
                    // Some fonts have buggy fpgm programs
                }
            }
        } else {
            fpgm_executed = true; // No fpgm = nothing to execute
        }

        Ok(Some(HintInstance {
            interpreter,
            fpgm_executed,
            prep_bytecode,
            cvt_funits,
            current_ppem: None,
        }))
    }

    /// Prepare the interpreter for a specific point size and DPI.
    ///
    /// Scales CVT values and executes the `prep` program.
    pub fn set_size(&mut self, point_size: f64, dpi: u16) -> Result<(), HintError> {
        let ppem = ((point_size * dpi as f64) / 72.0).round() as u16;
        self.set_ppem(ppem, point_size)
    }

    /// Prepare the interpreter for a specific ppem value.
    pub fn set_ppem(&mut self, ppem: u16, point_size: f64) -> Result<(), HintError> {
        // Skip if already configured for this ppem
        if self.current_ppem == Some(ppem) {
            return Ok(());
        }

        // Scale CVT from FUnits to F26Dot6
        self.interpreter.ppem = ppem;
        self.interpreter.scale =
            f26dot6::compute_scale(ppem, self.interpreter.units_per_em);
        self.interpreter.scale_cvt(&self.cvt_funits);

        // Execute prep program
        if !self.prep_bytecode.is_empty() && self.fpgm_executed {
            // Ignore prep errors (some fonts have buggy prep programs)
            let _ = self
                .interpreter
                .execute_prep(&self.prep_bytecode, ppem, point_size);
        }

        self.current_ppem = Some(ppem);
        Ok(())
    }

    /// Hint a simple glyph outline.
    ///
    /// Takes points already scaled to F26Dot6 pixel coordinates, the on-curve
    /// flags, contour end indices, the per-glyph instruction bytecode, and the
    /// horizontal advance width in F26Dot6.
    ///
    /// Internally appends 4 TrueType phantom points (origin, advance, top, bottom)
    /// required by the bytecode interpreter, then strips them from the result.
    ///
    /// Returns the hinted outline point positions as (x, y) pairs in F26Dot6
    /// (same length as `points_f26dot6`).
    pub fn hint_glyph(
        &mut self,
        points_f26dot6: &[(i32, i32)],
        on_curve: &[bool],
        contour_ends: &[u16],
        instructions: &[u8],
        advance_width_f26dot6: i32,
    ) -> Result<Vec<(i32, i32)>, HintError> {
        self.hint_glyph_with_orus(points_f26dot6, None, on_curve, contour_ends, instructions, advance_width_f26dot6)
    }

    /// Hint a glyph with optional unscaled original coordinates.
    ///
    /// `raw_points_funits` provides the original font-unit coordinates (before scaling).
    /// FreeType uses these for IUP interpolation factors, avoiding F26Dot6 rounding errors.
    ///
    /// `vert_origin_f26dot6` and `vert_advance_f26dot6` set the Y coordinates of
    /// phantom points 2 (top) and 3 (bottom).  FreeType computes these from vertical
    /// metrics: phantom[2].y = vertBearingY, phantom[3].y = vertBearingY - height.
    /// Pass `None` to use (0, 0) as before.
    pub fn hint_glyph_with_orus(
        &mut self,
        points_f26dot6: &[(i32, i32)],
        raw_points_funits: Option<&[(i16, i16)]>,
        on_curve: &[bool],
        contour_ends: &[u16],
        instructions: &[u8],
        advance_width_f26dot6: i32,
    ) -> Result<Vec<(i32, i32)>, HintError> {
        self.hint_glyph_full(
            points_f26dot6, raw_points_funits, on_curve, contour_ends,
            instructions, advance_width_f26dot6, None, None,
        )
    }

    /// Full hinting with vertical phantom point support.
    ///
    /// `phantom_top_y` and `phantom_bottom_y` are F26Dot6 Y coordinates for
    /// phantom points 2 and 3, computed from vertical metrics.
    pub fn hint_glyph_full(
        &mut self,
        points_f26dot6: &[(i32, i32)],
        raw_points_funits: Option<&[(i16, i16)]>,
        on_curve: &[bool],
        contour_ends: &[u16],
        instructions: &[u8],
        advance_width_f26dot6: i32,
        phantom_top_y: Option<i32>,
        phantom_bottom_y: Option<i32>,
    ) -> Result<Vec<(i32, i32)>, HintError> {
        if instructions.is_empty() || !self.fpgm_executed {
            return Ok(points_f26dot6.to_vec());
        }

        let real_count = points_f26dot6.len();

        let top_y = phantom_top_y.unwrap_or(0);
        let bottom_y = phantom_bottom_y.unwrap_or(0);

        let mut points: Vec<Point> = points_f26dot6
            .iter()
            .map(|&(x, y)| Point { x, y })
            .collect();
        // Don't round phantom points — let the glyph program handle them.
        // Rounding changes the MD measurements used by conditional logic,
        // causing wrong code paths (e.g., ClearType SHPIX on phantom points).
        points.push(Point { x: 0, y: 0 });                            // phantom[0]: origin
        points.push(Point { x: advance_width_f26dot6, y: 0 });        // phantom[1]: advance
        points.push(Point { x: 0, y: top_y });                        // phantom[2]: top
        points.push(Point { x: 0, y: bottom_y });                     // phantom[3]: bottom

        let mut on_curve_ext: Vec<bool> = on_curve.to_vec();
        on_curve_ext.extend_from_slice(&[true, true, true, true]);

        // Build unscaled orus points if raw coordinates are provided
        let orus: Option<Vec<Point>> = raw_points_funits.map(|raw| {
            let mut orus_pts: Vec<Point> = raw
                .iter()
                .map(|&(x, y)| Point { x: x as i32, y: y as i32 })
                .collect();
            // Phantom points in font units
            orus_pts.push(Point { x: 0, y: 0 });
            orus_pts.push(Point { x: 0, y: 0 }); // advance in funits not needed for IUP
            orus_pts.push(Point { x: 0, y: 0 });
            orus_pts.push(Point { x: 0, y: 0 });
            orus_pts
        });

        self.interpreter.hint_glyph_with_orus(
            &points,
            orus.as_deref(),
            &on_curve_ext,
            contour_ends,
            instructions,
        )?;

        let result: Vec<(i32, i32)> = self.interpreter.zones[1]
            .current
            .iter()
            .take(real_count)
            .map(|p| (p.x, p.y))
            .collect();

        Ok(result)
    }

    /// After a successful `hint_glyph_with_orus` call, returns per-point debug info:
    /// `(current, original, orus, touched_x, touched_y)` for each real point.
    pub fn zone_debug_info(&self, real_count: usize) -> Vec<((i32,i32),(i32,i32),(i32,i32),bool,bool)> {
        let z = &self.interpreter.zones[1];
        (0..real_count.min(z.current.len())).map(|i| {
            let cur = (z.current[i].x, z.current[i].y);
            let orig = (z.original[i].x, z.original[i].y);
            let orus = (z.orus[i].x, z.orus[i].y);
            let tx = z.flags[i].contains(interpreter::PointFlags::TOUCHED_X);
            let ty = z.flags[i].contains(interpreter::PointFlags::TOUCHED_Y);
            (cur, orig, orus, tx, ty)
        }).collect()
    }

    /// Hint a glyph and return the hinted advance width in F26Dot6.
    ///
    /// This extracts the advance from the hinted phantom point (index n+1),
    /// which is what FreeType uses for glyph positioning.
    pub fn hinted_advance_f26dot6(
        &mut self,
        points_f26dot6: &[(i32, i32)],
        raw_points_funits: Option<&[(i16, i16)]>,
        on_curve: &[bool],
        contour_ends: &[u16],
        instructions: &[u8],
        advance_width_f26dot6: i32,
    ) -> Result<i32, HintError> {
        if instructions.is_empty() || !self.fpgm_executed {
            return Ok(advance_width_f26dot6);
        }

        let real_count = points_f26dot6.len();

        let mut points: Vec<Point> = points_f26dot6
            .iter()
            .map(|&(x, y)| Point { x, y })
            .collect();
        points.push(Point { x: 0, y: 0 });
        points.push(Point { x: advance_width_f26dot6, y: 0 });
        points.push(Point { x: 0, y: 0 });
        points.push(Point { x: 0, y: 0 });

        let mut on_curve_ext: Vec<bool> = on_curve.to_vec();
        on_curve_ext.extend_from_slice(&[true, true, true, true]);

        let orus: Option<Vec<Point>> = raw_points_funits.map(|raw| {
            let mut orus_pts: Vec<Point> = raw
                .iter()
                .map(|&(x, y)| Point { x: x as i32, y: y as i32 })
                .collect();
            orus_pts.push(Point { x: 0, y: 0 });
            orus_pts.push(Point { x: 0, y: 0 });
            orus_pts.push(Point { x: 0, y: 0 });
            orus_pts.push(Point { x: 0, y: 0 });
            orus_pts
        });

        self.interpreter.hint_glyph_with_orus(
            &points,
            orus.as_deref(),
            &on_curve_ext,
            contour_ends,
            instructions,
        )?;

        // Phantom point at index real_count+1 is the hinted advance width
        let hinted_advance = self.interpreter.zones[1]
            .current
            .get(real_count + 1)
            .map(|p| p.x)
            .unwrap_or(advance_width_f26dot6);

        Ok(hinted_advance)
    }

    /// Hint a glyph and return both hinted coordinates AND post-hinting on-curve flags.
    ///
    /// FLIPPT/FLIPRGON/FLIPRGOFF instructions can change on-curve flags during hinting.
    /// The path builder MUST use the returned flags, not the original raw_on_curve.
    pub fn hint_glyph_with_flags(
        &mut self,
        points_f26dot6: &[(i32, i32)],
        on_curve: &[bool],
        contour_ends: &[u16],
        instructions: &[u8],
        advance_width_f26dot6: i32,
    ) -> Result<(Vec<(i32, i32)>, Vec<bool>), HintError> {
        if instructions.is_empty() || !self.fpgm_executed {
            return Ok((points_f26dot6.to_vec(), on_curve.to_vec()));
        }

        let real_count = points_f26dot6.len();

        let mut points: Vec<Point> = points_f26dot6
            .iter()
            .map(|&(x, y)| Point { x, y })
            .collect();
        points.push(Point { x: 0, y: 0 });
        points.push(Point { x: advance_width_f26dot6, y: 0 });
        points.push(Point { x: 0, y: 0 });
        points.push(Point { x: 0, y: 0 });

        let mut on_curve_ext: Vec<bool> = on_curve.to_vec();
        on_curve_ext.extend_from_slice(&[true, true, true, true]);

        self.interpreter
            .hint_glyph(&points, &on_curve_ext, contour_ends, instructions)?;

        let coords: Vec<(i32, i32)> = self.interpreter.zones[1]
            .current
            .iter()
            .take(real_count)
            .map(|p| (p.x, p.y))
            .collect();

        use interpreter::PointFlags;
        let flags: Vec<bool> = self.interpreter.zones[1]
            .flags
            .iter()
            .take(real_count)
            .map(|f| f.contains(PointFlags::ON_CURVE))
            .collect();

        Ok((coords, flags))
    }

    /// Returns the prep bytecode.
    pub fn prep_bytecode(&self) -> &[u8] {
        &self.prep_bytecode
    }

    /// Returns the CVT values in font units (before scaling).
    pub fn cvt_funits(&self) -> &[i16] {
        &self.cvt_funits
    }

    /// Returns whether fpgm was executed successfully.
    pub fn fpgm_executed(&self) -> bool {
        self.fpgm_executed
    }
}

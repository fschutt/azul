#![deny(missing_docs)]

//! Access glyphs outlines. Requires the `outline` cargo feature (enabled by default).
//!
//! This module is used to access the outlines of glyphs as a series of foundational drawing
//! instruction callbacks on implementors of the `OutlineSink` trait. Outlines from `glyf` and
//! `CFF` tables can be accessed.
//!
//! ### Example
//!
//! This is a fairly complete example of mapping some glyphs and then visiting their outlines with
//! support for TrueType and CFF fonts. It accumulates the drawing operations into a `String`.
//! In a real application you'd probably make calls to a graphics library instead.
//!
//! ```
//! use std::fmt::Write;
//!
//! use allsorts::binary::read::ReadScope;
//! use allsorts::cff::outline::CFFOutlines;
//! use allsorts::cff::CFF;
//! use allsorts::font::{GlyphTableFlags, MatchingPresentation};
//! use allsorts::font_data::FontData;
//! use allsorts::gsub::RawGlyph;
//! use allsorts::outline::{OutlineBuilder, OutlineSink};
//! use allsorts::pathfinder_geometry::line_segment::LineSegment2F;
//! use allsorts::pathfinder_geometry::vector::Vector2F;
//! use allsorts::tables::glyf::{GlyfVisitorContext, LocaGlyf};
//! use allsorts::tables::loca::{owned, LocaTable};
//! use allsorts::tables::{FontTableProvider, SfntVersion};
//! use allsorts::{tag, Font};
//!
//! struct DebugVisitor {
//!     outlines: String,
//! }
//!
//! impl OutlineSink for DebugVisitor {
//!     fn move_to(&mut self, to: Vector2F) {
//!         writeln!(&mut self.outlines, "move_to({}, {})", to.x(), to.y()).unwrap();
//!     }
//!
//!     fn line_to(&mut self, to: Vector2F) {
//!         writeln!(&mut self.outlines, "line_to({}, {})", to.x(), to.y()).unwrap();
//!     }
//!
//!     fn quadratic_curve_to(&mut self, control: Vector2F, to: Vector2F) {
//!         writeln!(
//!             &mut self.outlines,
//!             "quad_to({}, {}, {}, {})",
//!             control.x(),
//!             control.y(),
//!             to.x(),
//!             to.y()
//!         )
//!         .unwrap();
//!     }
//!
//!     fn cubic_curve_to(&mut self, control: LineSegment2F, to: Vector2F) {
//!         writeln!(
//!             &mut self.outlines,
//!             "curve_to({}, {}, {}, {}, {}, {})",
//!             control.from_x(),
//!             control.from_y(),
//!             control.to_x(),
//!             control.to_y(),
//!             to.x(),
//!             to.y()
//!         )
//!         .unwrap();
//!     }
//!
//!     fn close(&mut self) {
//!         writeln!(&mut self.outlines, "close()").unwrap();
//!     }
//! }
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let script = tag::LATN;
//!     let buffer = std::fs::read("tests/fonts/opentype/Klei.otf")?;
//!     let scope = ReadScope::new(&buffer);
//!     let font_file = scope.read::<FontData<'_>>()?;
//!     let provider = font_file.table_provider(0)?;
//!     let mut font = Font::new(provider)?;
//!     let mut sink = DebugVisitor {
//!         outlines: String::new(),
//!     };
//!
//!     // Map text to glyphs
//!     let glyphs = font.map_glyphs("+", script, MatchingPresentation::NotRequired);
//!
//!     // Visit the outlines of each glyph. Read tables depending on the type of font
//!     if font.glyph_table_flags.contains(GlyphTableFlags::CFF)
//!         && font.font_table_provider.sfnt_version() == tag::OTTO
//!     {
//!         let cff_data = font.font_table_provider.read_table_data(tag::CFF)?;
//!         let cff = ReadScope::new(&cff_data).read::<CFF<'_>>()?;
//!         let mut cff_outlines = CFFOutlines { table: &cff };
//!         sink.glyphs_to_path(&mut cff_outlines, &glyphs)?;
//!     } else if font.glyph_table_flags.contains(GlyphTableFlags::GLYF) {
//!         let loca_data = font.font_table_provider.read_table_data(tag::LOCA)?;
//!         let loca = ReadScope::new(&loca_data).read_dep::<LocaTable<'_>>((
//!             font.maxp_table.num_glyphs,
//!             font.head_table.index_to_loc_format,
//!         ))?;
//!         let glyf_data = font
//!             .font_table_provider
//!             .read_table_data(tag::GLYF)
//!             .map(Box::from)?;
//!         let mut loca_glyf = LocaGlyf::loaded(owned::LocaTable::from(&loca), glyf_data);
//!         let mut ctx = GlyfVisitorContext::new(&mut loca_glyf, None);
//!         sink.glyphs_to_path(&mut ctx, &glyphs)?;
//!     } else {
//!         return Err("no glyf or CFF table".into());
//!     }
//!
//!     let expected = "move_to(225, 152)
//! line_to(225, 269)
//! curve_to(225, 274, 228, 276, 232, 276)
//! line_to(341, 276)
//! curve_to(346, 276, 347, 285, 347, 295)
//! curve_to(347, 307, 345, 320, 341, 320)
//! line_to(232, 320)
//! curve_to(226, 320, 226, 325, 226, 328)
//! line_to(226, 432)
//! curve_to(220, 435, 214, 437, 206, 437)
//! curve_to(198, 437, 190, 435, 181, 432)
//! line_to(181, 329)
//! curve_to(181, 326, 180, 320, 172, 320)
//! line_to(68, 320)
//! curve_to(62, 320, 59, 311, 59, 300)
//! curve_to(59, 289, 62, 278, 68, 276)
//! line_to(174, 276)
//! curve_to(179, 276, 181, 271, 181, 267)
//! line_to(181, 152)
//! curve_to(181, 147, 193, 144, 204, 144)
//! curve_to(215, 144, 225, 147, 225, 152)
//! close()
//! ";
//!     assert_eq!(sink.outlines, expected);
//!     Ok(())
//! }
//!
//! impl DebugVisitor {
//!     pub fn glyphs_to_path<T>(
//!         &mut self,
//!         builder: &mut T,
//!         glyphs: &[RawGlyph<()>],
//!     ) -> Result<(), Box<dyn std::error::Error>>
//!     where
//!         T: OutlineBuilder,
//!         <T as OutlineBuilder>::Error: 'static,
//!     {
//!         for glyph in glyphs {
//!             builder.visit(glyph.glyph_index, None, self)?;
//!         }
//!
//!         Ok(())
//!     }
//! }
//! ```

use std::cmp::Ordering;

use pathfinder_geometry::vector::Vector2F;
use pathfinder_geometry::{line_segment::LineSegment2F, rect::RectF};
use tinyvec::{array_vec, ArrayVec};

use crate::tables::glyf::{BoundingBox, Point as GlyfPoint};
use crate::tables::variable_fonts::OwnedTuple;
use crate::TryNumFrom;

#[derive(Clone, Copy, Debug)]
pub(crate) struct BBox {
    rect: Option<RectF>,
}

/// Trait for visiting a glyph outline and delivering drawing commands to an `OutlineSink`.
pub trait OutlineBuilder {
    /// The error type returned by the `visit` method.
    type Error: std::error::Error;
    /// The output type returned by the `visit` method.
    type Output;

    /// Visit the glyph outlines in `self`.
    fn visit<S: OutlineSink>(
        &mut self,
        glyph_index: u16,
        tuple: Option<&OwnedTuple>,
        sink: &mut S,
    ) -> Result<Self::Output, Self::Error>;
}

// `OutlineSink` is from font-kit, font-kit/src/outline.rs:
//
// Copyright © 2020 The Pathfinder Project Developers.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/// A trait for visiting a glyph outline
pub trait OutlineSink {
    /// Moves the pen to a point.
    fn move_to(&mut self, to: Vector2F);
    /// Draws a line to a point.
    fn line_to(&mut self, to: Vector2F);
    /// Draws a quadratic Bézier curve to a point.
    fn quadratic_curve_to(&mut self, ctrl: Vector2F, to: Vector2F);
    /// Draws a cubic Bézier curve to a point.
    fn cubic_curve_to(&mut self, ctrl: LineSegment2F, to: Vector2F);
    /// Closes the path, returning to the first point in it.
    fn close(&mut self);
}

/// An [OutlineSink] that computes the bounding box of a glyph.
pub struct BoundingBoxSink {
    prev_point: Vector2F,
    bbox: BBox,
}

impl BBox {
    pub(crate) fn new() -> Self {
        BBox { rect: None }
    }

    pub(crate) fn is_default(&self) -> bool {
        self.rect.is_none()
    }

    pub(crate) fn extend_by_point(&mut self, point: Vector2F) {
        // Extend the existing rect or initialise it with an empty rect containing
        // only `point`.
        self.rect = self
            .rect
            .map(|rect| rect.union_point(point))
            .or_else(|| Some(RectF::from_points(point, point)))
    }

    pub(crate) fn to_bounding_box(&self) -> Option<BoundingBox> {
        match self.rect.map(RectF::round_out) {
            Some(rect) => Some(BoundingBox {
                x_min: i16::try_num_from(rect.min_x())?,
                y_min: i16::try_num_from(rect.min_y())?,
                x_max: i16::try_num_from(rect.max_x())?,
                y_max: i16::try_num_from(rect.max_y())?,
            }),
            None => None,
        }
    }
}

impl BoundingBoxSink {
    /// Construct a new `BoundingBoxSink` for use with [OutlineBuilder].
    pub fn new() -> Self {
        BoundingBoxSink {
            prev_point: Vector2F::zero(),
            bbox: BBox::new(),
        }
    }

    pub(crate) fn bbox(&self) -> BBox {
        self.bbox
    }

    /// Returns the calculated bounding box of the glyph outline.
    pub fn to_bounding_box(&self) -> Option<BoundingBox> {
        self.bbox.to_bounding_box()
    }
}

impl OutlineSink for BoundingBoxSink {
    fn move_to(&mut self, to: Vector2F) {
        self.bbox.extend_by_point(to);
        self.prev_point = to;
    }

    fn line_to(&mut self, to: Vector2F) {
        self.bbox.extend_by_point(to);
        self.prev_point = to;
    }

    fn quadratic_curve_to(&mut self, ctrl: Vector2F, to: Vector2F) {
        // https://iquilezles.org/articles/bezierbbox/
        let p0 = self.prev_point;
        let p1 = ctrl;
        let p2 = to;

        // If the box around the start point and end point contains the control point,
        // then that is the bounding box of the curve. Otherwise we need to find the
        // extrema of the curve.
        if !RectF::from_points(p0.min(p2), p0.max(p2)).contains_point(p1) {
            // Calculate where derivative is zero
            let denominator = p0 - (p1 * 2.0) + p2;
            if denominator.x() != 0.0 && denominator.y() != 0.0 {
                let t = ((p0 - p1) / denominator).clamp(Vector2F::splat(0.0), Vector2F::splat(1.0));

                // Feed that back into the bezier formula to get the point on the curve
                let s = Vector2F::splat(1.0) - t;
                let q = s * s * p0 + (s * 2.0) * t * p1 + t * t * p2;
                self.bbox.extend_by_point(q);
            }
        }

        self.bbox.extend_by_point(to);
        self.prev_point = to;
    }

    fn cubic_curve_to(&mut self, ctrl: LineSegment2F, to: Vector2F) {
        let from = self.prev_point;

        // If the box around the start point and end point contains both control points,
        // then that is the bounding box of the curve. Otherwise we need to find the
        // extrema of the curve.
        let rect = RectF::from_points(from.min(to), from.max(to));
        if !(rect.contains_point(ctrl.from()) && rect.contains_point(ctrl.to())) {
            let (x_roots, y_roots) = bezier_roots(from, ctrl.from(), ctrl.to(), to);
            let coefficients = BezierCoefficients::new(from, ctrl.from(), ctrl.to(), to);
            for t in x_roots.iter().chain(y_roots.iter()).copied() {
                let point = coefficients.bezier_point(t);
                self.bbox.extend_by_point(point);
            }
        }

        self.bbox.extend_by_point(to);
        self.prev_point = to;
    }

    fn close(&mut self) {
        // Nothing to do. Close returns the starting point, which is already contained in the bbox.
    }
}

// References:
//
// The Math Behind Bezier Cubic Splines
// http://www.tinaja.com/glib/cubemath.pdf
//
// Warping Text To Bézier curves
// http://www.planetclegg.com/projects/WarpingTextToSplines.html

/// A-H values in equation space
#[derive(Debug, Copy, Clone)]
struct BezierCoefficients {
    pub ae: Vector2F,
    pub bf: Vector2F,
    pub cg: Vector2F,
    pub dh: Vector2F,
}

impl BezierCoefficients {
    pub fn new(p0: Vector2F, p1: Vector2F, p2: Vector2F, p3: Vector2F) -> Self {
        let ae = p3 - p2 * 3.0 + p1 * 3.0 - p0;
        let bf = p2 * 3.0 - p1 * 6.0 + p0 * 3.0;
        let cg = p1 * 3.0 - p0 * 3.0;
        let dh = p0;

        Self { ae, bf, cg, dh }
    }

    #[cfg(test)]
    fn as_array(&self) -> [f32; 8] {
        [
            self.ae.x(),
            self.bf.x(),
            self.cg.x(),
            self.dh.x(),
            self.ae.y(),
            self.bf.y(),
            self.cg.y(),
            self.dh.y(),
        ]
    }

    fn bezier_point(&self, t: f32) -> Vector2F {
        self.ae * t.powi(3) + self.bf * t.powi(2) + self.cg * t + self.dh
    }
}

// Finding extremities: root finding
// https://pomax.github.io/bezierinfo/#extremities
fn bezier_roots(
    p1: Vector2F,
    p2: Vector2F,
    p3: Vector2F,
    p4: Vector2F,
) -> (ArrayVec<[f32; 2]>, ArrayVec<[f32; 2]>) {
    let x_roots = bezier_component_roots(p1.x(), p2.x(), p3.x(), p4.x());
    let y_roots = bezier_component_roots(p1.y(), p2.y(), p3.y(), p4.y());

    (x_roots, y_roots)
}

fn bezier_component_roots(p1: f32, p2: f32, p3: f32, p4: f32) -> ArrayVec<[f32; 2]> {
    let a = 3.0 * (-p1 + 3.0 * p2 - 3.0 * p3 + p4);
    let b = 6.0 * (p1 - 2.0 * p2 + p3);
    let c = 3.0 * (p2 - p1);

    let roots = if a == 0.0 {
        //  𝑓ʹ(𝑡) = 𝑎𝑡² + 𝑏𝑡 + 𝑐
        //      0 = 𝑏𝑡 + 𝑐
        //      𝑡 = -𝑐/𝑏
        if b == 0.0 {
            ArrayVec::new()
        } else {
            let t = -c / b;
            // If B is some very small value but not quite zero, T can end up as
            // a very large value risking overflow. Address if needed.
            array_vec!([f32; 2] => t)
        }
    } else {
        solve_quadratic(a, b, c)
    };

    roots
        .into_iter()
        .filter(|root| (0.0..=1.0).contains(root))
        .collect()
}

// https://en.wikipedia.org/wiki/Quadratic_formula
// https://apps.dtic.mil/sti/tr/pdf/AD0639052.pdf
// https://s3.amazonaws.com/nrbook.com/book_C210_pdf/chap10c.pdf
// 𝑎𝑥² + 𝑏𝑥 + 𝑐 = 0
fn solve_quadratic(a: f32, b: f32, c: f32) -> ArrayVec<[f32; 2]> {
    // The quantity Δ = b² − 4ac is known as the discriminant of the quadratic equation.
    let discriminant = b * b - 4.0 * a * c;
    match discriminant.total_cmp(&0.0) {
        // when Δ < 0, the equation has no real roots
        Ordering::Less => ArrayVec::new(),
        // when Δ = 0, the equation has one repeated real root
        Ordering::Equal => array_vec!([f32; 2] => -0.5 * b / a),
        // when Δ > 0, the equation has two distinct real roots
        Ordering::Greater => {
            let sqrt_d = discriminant.sqrt();

            // Avoid catastrophic cancellation
            // https://en.wikipedia.org/wiki/Quadratic_formula#Numerical_calculation
            // https://people.csail.mit.edu/bkph/articles/Quadratics.pdf
            match b.total_cmp(&0.0) {
                Ordering::Less => {
                    let q = -0.5 * (b - sqrt_d);
                    ArrayVec::from([q / a, c / q])
                }
                Ordering::Equal => {
                    let root = -0.5 * sqrt_d / a;
                    ArrayVec::from([root, -root])
                }
                Ordering::Greater => {
                    let q = -0.5 * (b + sqrt_d);
                    ArrayVec::from([q / a, c / q])
                }
            }
        }
    }
}

impl From<GlyfPoint> for Vector2F {
    fn from(point: GlyfPoint) -> Self {
        Vector2F::new(f32::from(point.0), f32::from(point.1))
    }
}

#[cfg(test)]
mod tests {
    use pathfinder_geometry::vector::vec2f;

    use crate::assert_close;

    use super::*;

    // First two test cases from:
    // https://apps.dtic.mil/sti/tr/pdf/AD0639052.pdf §6 p. 10
    #[test]
    fn test_solve_quadratic1() {
        let res = solve_quadratic(6.0, 5.0, -4.0);
        let &[root1, root2] = res.as_slice() else {
            panic!("Expected two roots");
        };
        assert_close!(root1, -1.33333333);
        assert_close!(root2, 0.5);
    }

    #[test]
    fn test_solve_quadratic2() {
        let res = solve_quadratic(1.0, 100000.0, 1.0);
        let &[root1, root2] = res.as_slice() else {
            panic!("Expected two roots");
        };
        assert_close!(root1, -100000.0);
        assert_close!(root2, -0.00001);
    }

    #[test]
    fn bezier_coefficients() {
        let actual = BezierCoefficients::new(
            vec2f(110., 150.),
            vec2f(25., 190.),
            vec2f(210., 250.),
            vec2f(210., 30.),
        );
        let expected = [-455.0_f32, 810.0, -255.0, 110.0, -300.0, 60.0, 120.0, 150.0];
        for (&a, &b) in actual.as_array().iter().zip(expected.iter()) {
            assert_close!(a, b);
        }
    }

    #[test]
    fn quadratic_bbox() {
        let mut sink = BoundingBoxSink::new();
        let start = vec2f(82.148931, 87.063829);
        sink.move_to(start);
        let ctrl = vec2f(91.627661, 5.968085099999996);
        let to = vec2f(103.56383, 64.595743);
        sink.quadratic_curve_to(ctrl, to);
        let bbox = sink.bbox.rect.unwrap();

        assert_close!(bbox.origin_x(), 82.148931);
        assert_close!(bbox.origin_y(), 39.995697);
        assert_close!(bbox.width(), 103.56383 - 82.148931);
        assert_close!(bbox.height(), 87.063829 - 39.995697);
    }
}

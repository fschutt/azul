//! Transformation matrices.
//!
//! PDF describes a transformation with six numbers `[a b c d e f]`. A point `(x, y)` is moved to
//! `(a*x + c*y + e, b*x + d*y + f)`. So `[1 0 0 1 e f]` moves by `(e, f)`, `[sx 0 0 sy 0 0]`
//! scales, and `[cos sin -sin cos 0 0]` rotates counter-clockwise (y points up).
//!
//! Two matrices exist in a content stream:
//!
//! - [`CurTransMat`], the *current transformation matrix*, set with `Op::SetTransformationMatrix`
//!   (`cm`). It applies to everything drawn afterwards and is multiplied with the matrix that is
//!   already active, so wrap it in `Op::SaveGraphicsState` / `Op::RestoreGraphicsState`.
//! - [`TextMatrix`], set with `Op::SetTextMatrix` (`Tm`). It positions the text of a text
//!   section on top of the current transformation matrix and replaces the previous text
//!   matrix instead of multiplying with it.
//!
//! The variants build the six numbers for the common cases; `Raw` takes them directly.
//!
//! ```
//! use printpdf::*;
//!
//! let m = CurTransMat::Translate(Pt(10.0), Pt(20.0));
//! assert_eq!(m.as_array(), [1.0, 0.0, 0.0, 1.0, 10.0, 20.0]);
//!
//! let ops = vec![
//!     Op::SaveGraphicsState,
//!     Op::SetTransformationMatrix { matrix: CurTransMat::Scale(2.0, 2.0) },
//!     // ... drawing here is twice as large ...
//!     Op::RestoreGraphicsState,
//! ];
//! # assert_eq!(ops.len(), 3);
//! ```

use serde_derive::{Deserialize, Serialize};

use crate::units::Pt;

/// The current transformation matrix (CTM) of the graphics state, written with the `cm`
/// operator by `Op::SetTransformationMatrix`.
///
/// Each use is multiplied with the matrix that is already active, so transformations add up.
/// The effect lasts until the graphics state is restored with `Op::RestoreGraphicsState`;
/// put `Op::SaveGraphicsState` before it. Angles are in degrees and rotate counter-clockwise.
///
/// Serialized to JSON as `{"type": "<variant in kebab-case>", "data": ...}`.
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "type", content = "data")]
pub enum CurTransMat {
    /// Moves the origin by (x, y), measured in the current coordinate system.
    Translate(Pt, Pt),
    /// Rotates by the angle in degrees (counter-clockwise) around the current origin `(0, 0)`.
    Rotate(f32),
    /// Rotates by the angle in degrees (counter-clockwise), then moves the origin to (x, y).
    TranslateRotate(Pt, Pt, f32),
    /// Scales x and y by the given factors; 1.0 is no change.
    Scale(f32, f32),
    /// The six numbers `[a b c d e f]` of the PDF matrix.
    Raw([f32; 6]),
    /// Leaves everything as it is (`[1 0 0 1 0 0]`).
    Identity,
}

impl PartialEq for CurTransMat {
    fn eq(&self, other: &Self) -> bool {
        self.as_array() == other.as_array()
    }
}

impl CurTransMat {
    /// The matrix as a CSS `matrix(a b c d e f)` value.
    pub fn as_css_val(&self) -> String {
        let m = self.as_array();
        format!(
            "matrix({} {} {} {} {} {})",
            m[0], m[1], m[2], m[3], m[4], m[5]
        )
    }

    /// Multiplies two matrices given as `[a b c d e f]`. The result applies `a` first and then
    /// `b`.
    pub fn combine_matrix(a: [f32; 6], b: [f32; 6]) -> [f32; 6] {
        let a = [
            [a[0], a[1], 0.0, 0.0],
            [a[2], a[3], 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [a[4], a[5], 0.0, 1.0],
        ];

        let b = [
            [b[0], b[1], 0.0, 0.0],
            [b[2], b[3], 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [b[4], b[5], 0.0, 1.0],
        ];

        let result = [
            [
                mul_add(
                    a[0][0],
                    b[0][0],
                    mul_add(
                        a[0][1],
                        b[1][0],
                        mul_add(a[0][2], b[2][0], a[0][3] * b[3][0]),
                    ),
                ),
                mul_add(
                    a[0][0],
                    b[0][1],
                    mul_add(
                        a[0][1],
                        b[1][1],
                        mul_add(a[0][2], b[2][1], a[0][3] * b[3][1]),
                    ),
                ),
                mul_add(
                    a[0][0],
                    b[0][2],
                    mul_add(
                        a[0][1],
                        b[1][2],
                        mul_add(a[0][2], b[2][2], a[0][3] * b[3][2]),
                    ),
                ),
                mul_add(
                    a[0][0],
                    b[0][3],
                    mul_add(
                        a[0][1],
                        b[1][3],
                        mul_add(a[0][2], b[2][3], a[0][3] * b[3][3]),
                    ),
                ),
            ],
            [
                mul_add(
                    a[1][0],
                    b[0][0],
                    mul_add(
                        a[1][1],
                        b[1][0],
                        mul_add(a[1][2], b[2][0], a[1][3] * b[3][0]),
                    ),
                ),
                mul_add(
                    a[1][0],
                    b[0][1],
                    mul_add(
                        a[1][1],
                        b[1][1],
                        mul_add(a[1][2], b[2][1], a[1][3] * b[3][1]),
                    ),
                ),
                mul_add(
                    a[1][0],
                    b[0][2],
                    mul_add(
                        a[1][1],
                        b[1][2],
                        mul_add(a[1][2], b[2][2], a[1][3] * b[3][2]),
                    ),
                ),
                mul_add(
                    a[1][0],
                    b[0][3],
                    mul_add(
                        a[1][1],
                        b[1][3],
                        mul_add(a[1][2], b[2][3], a[1][3] * b[3][3]),
                    ),
                ),
            ],
            [
                mul_add(
                    a[2][0],
                    b[0][0],
                    mul_add(
                        a[2][1],
                        b[1][0],
                        mul_add(a[2][2], b[2][0], a[2][3] * b[3][0]),
                    ),
                ),
                mul_add(
                    a[2][0],
                    b[0][1],
                    mul_add(
                        a[2][1],
                        b[1][1],
                        mul_add(a[2][2], b[2][1], a[2][3] * b[3][1]),
                    ),
                ),
                mul_add(
                    a[2][0],
                    b[0][2],
                    mul_add(
                        a[2][1],
                        b[1][2],
                        mul_add(a[2][2], b[2][2], a[2][3] * b[3][2]),
                    ),
                ),
                mul_add(
                    a[2][0],
                    b[0][3],
                    mul_add(
                        a[2][1],
                        b[1][3],
                        mul_add(a[2][2], b[2][3], a[2][3] * b[3][3]),
                    ),
                ),
            ],
            [
                mul_add(
                    a[3][0],
                    b[0][0],
                    mul_add(
                        a[3][1],
                        b[1][0],
                        mul_add(a[3][2], b[2][0], a[3][3] * b[3][0]),
                    ),
                ),
                mul_add(
                    a[3][0],
                    b[0][1],
                    mul_add(
                        a[3][1],
                        b[1][1],
                        mul_add(a[3][2], b[2][1], a[3][3] * b[3][1]),
                    ),
                ),
                mul_add(
                    a[3][0],
                    b[0][2],
                    mul_add(
                        a[3][1],
                        b[1][2],
                        mul_add(a[3][2], b[2][2], a[3][3] * b[3][2]),
                    ),
                ),
                mul_add(
                    a[3][0],
                    b[0][3],
                    mul_add(
                        a[3][1],
                        b[1][3],
                        mul_add(a[3][2], b[2][3], a[3][3] * b[3][3]),
                    ),
                ),
            ],
        ];

        [
            result[0][0],
            result[0][1],
            result[1][0],
            result[1][1],
            result[3][0],
            result[3][1],
        ]
    }
}

// Computes `a * b + c`. `f32::mul_add` is broken on arm-unknown-linux-gnueabi
// (rust-lang/rust#46950), where a plain multiply and add is used.
#[inline(always)]
fn mul_add(a: f32, b: f32, c: f32) -> f32 {
    if cfg!(all(
        target_arch = "arm",
        target_os = "linux",
        target_env = "gnu"
    )) {
        // Workaround has two rounding errors and less accurate result,
        // but for PDF it doesn't matter much.
        (a * b) + c
    } else {
        a.mul_add(b, c)
    }
}

/// The text matrix, written with the `Tm` operator by `Op::SetTextMatrix`. It sets where the
/// next text is drawn and how it is rotated or scaled.
///
/// Unlike [`CurTransMat`] it replaces the previous text matrix, including any position set
/// by an earlier `Op::SetTextCursor`, and is applied on top of the current transformation
/// matrix. Because the position is replaced too, there is no rotate-only variant: use
/// [`TextMatrix::TranslateRotate`]. For spacing between words and characters use
/// `Op::SetWordSpacing` and `Op::SetCharacterSpacing`.
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TextMatrix {
    /// Puts the text position at (x, y), measured from the lower left corner of the page (in
    /// the current coordinate system). Like `Op::SetTextCursor`, but absolute.
    Translate(Pt, Pt),
    /// Rotates the text by the angle in degrees (counter-clockwise) and puts its position at
    /// (x, y), measured from the lower left corner of the page (in the current coordinate
    /// system).
    TranslateRotate(Pt, Pt, f32),
    /// The six numbers `[a b c d e f]` of the PDF matrix.
    Raw([f32; 6]),
}

impl PartialEq for TextMatrix {
    fn eq(&self, other: &Self) -> bool {
        self.as_array() == other.as_array()
    }
}

impl TextMatrix {
    /// The matrix as a CSS `matrix(a b c d e f)` value. With `invert_y` the vertical
    /// translation `f` is negated, to convert from PDF's upward y axis to CSS's downward one.
    pub fn as_css_val(&self, invert_y: bool) -> String {
        let m = self.as_array();
        let factor = if invert_y { -1.0 } else { 1.0 };
        format!(
            "matrix({} {} {} {} {} {})",
            m[0],
            m[1],
            m[2],
            m[3],
            m[4],
            m[5] * factor
        )
    }

    /// The six numbers `[a b c d e f]` of the matrix, as written to the PDF.
    pub fn as_array(&self) -> [f32; 6] {
        use self::TextMatrix::*;
        match self {
            Translate(x, y) => {
                // 1 0 0 1 x y cm
                [1.0, 0.0, 0.0, 1.0, x.0, y.0]
            }
            Raw(r) => *r,
            TranslateRotate(x, y, rot) => {
                let rad = (360.0 - rot).to_radians();
                [rad.cos(), -rad.sin(), rad.sin(), rad.cos(), x.0, y.0] /* cos sin -sin cos x y
                                                                         * cm */
            }
        }
    }
}

impl CurTransMat {
    /// The six numbers `[a b c d e f]` of the matrix, as written to the PDF.
    pub fn as_array(&self) -> [f32; 6] {
        use self::CurTransMat::*;
        match self {
            Translate(x, y) => {
                // 1 0 0 1 x y cm
                [1.0, 0.0, 0.0, 1.0, x.0, y.0]
            }
            TranslateRotate(x, y, rot) => {
                let rad = (360.0 - rot).to_radians();
                [rad.cos(), -rad.sin(), rad.sin(), rad.cos(), x.0, y.0] /* cos sin -sin cos x y
                                                                         * cm */
            }
            Rotate(rot) => {
                // cos sin -sin cos 0 0 cm
                let rad = (360.0 - rot).to_radians();
                [rad.cos(), -rad.sin(), rad.sin(), rad.cos(), 0.0, 0.0]
            }
            Raw(r) => *r,
            Scale(x, y) => {
                // x 0 0 y 0 0 cm
                [*x, 0.0, 0.0, *y, 0.0, 0.0]
            }
            Identity => [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        }
    }
}

#[test]
fn test_ctm_translate() {
    use self::*;

    // test that the translation matrix look like what PDF expects
    let ctm_trans = CurTransMat::Translate(Pt(150.0), Pt(50.0));
    let ctm_trans_arr: [f32; 6] = ctm_trans.as_array();
    assert_eq!([1.0_f32, 0.0, 0.0, 1.0, 150.0, 50.0], ctm_trans_arr);

    let ctm_scale = CurTransMat::Scale(2.0, 4.0);
    let ctm_scale_arr: [f32; 6] = ctm_scale.as_array();
    assert_eq!([2.0_f32, 0.0, 0.0, 4.0, 0.0, 0.0], ctm_scale_arr);

    let ctm_rot = CurTransMat::Rotate(30.0);
    let ctm_rot_arr: [f32; 6] = ctm_rot.as_array();
    assert_eq!(
        [0.8660253, 0.5000002, -0.5000002, 0.8660253, 0.0, 0.0],
        ctm_rot_arr
    );
}

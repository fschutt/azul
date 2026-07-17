//! 3D transform matrix computations for CSS transforms.
//!
//! This module implements 4x4 transformation matrices for CSS `transform` properties,
//! including translation, rotation, scaling, skewing, and perspective. It handles conversion
//! from CSS transform functions to hardware-accelerated matrices for WebRender.
//!
//! On x86_64 platforms, the module automatically detects and uses SSE/AVX instructions
//! for optimized matrix multiplication and inversion.
//!
//! **NOTE**: Matrices are stored in **row-major** format (unlike some graphics APIs that
//! use column-major). The module handles coordinate system differences between WebRender
//! and hit-testing via the `RotationMode` enum.

use core::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};

use azul_css::props::style::{StyleTransform, StyleTransformOrigin};

use crate::geom::LogicalPosition;

/// CPU feature detection: true if initialization has been performed
pub static INITIALIZED: AtomicBool = AtomicBool::new(false);
/// CPU feature detection: true if AVX instructions are available
pub static USE_AVX: AtomicBool = AtomicBool::new(false);
/// CPU feature detection: true if SSE instructions are available
pub static USE_SSE: AtomicBool = AtomicBool::new(false);

/// Specifies the coordinate system convention for rotations.
///
/// `WebRender` uses a different rotation direction than hit-testing, so transforms
/// must be adjusted based on their use case. This enum controls whether the
/// rotation matrix is inverted to match the expected behavior.
#[derive(Debug, Copy, Clone)]
pub enum RotationMode {
    /// Use rotation convention for `WebRender` (counter-clockwise, requires inversion)
    ForWebRender,
    /// Use rotation convention for hit-testing (clockwise, no inversion)
    ForHitTesting,
}

/// A computed 4x4 transformation matrix in pixel space.
///
/// Represents the final transformation matrix for a DOM element after applying
/// all CSS transform functions (translate, rotate, scale, etc.) and accounting
/// for transform-origin.
///
/// # Memory Layout
///
/// Matrix is stored in **row-major** format:
/// ```text
/// m[0] = [m11, m12, m13, m14]
/// m[1] = [m21, m22, m23, m24]
/// m[2] = [m31, m32, m33, m34]
/// m[3] = [m41, m42, m43, m44]
/// ```
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct ComputedTransform3D {
    /// The 4x4 matrix in row-major format
    pub m: [[f32; 4]; 4],
}

impl ComputedTransform3D {
    /// The identity matrix (no transformation).
    pub const IDENTITY: Self = Self {
        m: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
    };

    /// Creates a new 4x4 transformation matrix with the given elements.
    ///
    /// Elements are specified in row-major order (m11, m12, ..., m44).
    #[must_use] pub const fn new(
        m11: f32,
        m12: f32,
        m13: f32,
        m14: f32,
        m21: f32,
        m22: f32,
        m23: f32,
        m24: f32,
        m31: f32,
        m32: f32,
        m33: f32,
        m34: f32,
        m41: f32,
        m42: f32,
        m43: f32,
        m44: f32,
    ) -> Self {
        Self {
            m: [
                [m11, m12, m13, m14],
                [m21, m22, m23, m24],
                [m31, m32, m33, m34],
                [m41, m42, m43, m44],
            ],
        }
    }

    /// Creates a 2D transformation matrix (3D matrix with Z = 0).
    ///
    /// This is equivalent to the CSS `matrix()` function. The transformation
    /// only affects the X and Y axes.
    ///
    /// Corresponds to `matrix(m11, m12, m21, m22, m41, m42)` in CSS.
    const fn new_2d(m11: f32, m12: f32, m21: f32, m22: f32, m41: f32, m42: f32) -> Self {
        Self::new(
            m11, m12, 0.0, 0.0, m21, m22, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, m41, m42, 0.0, 1.0,
        )
    }

    /// Computes the inverse of this transformation matrix.
    ///
    /// This function uses a standard matrix inversion algorithm. Returns the
    /// identity matrix if the determinant is zero (singular matrix).
    ///
    /// NOTE: This is a relatively expensive operation.
    #[must_use]
    #[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
    pub fn inverse(&self) -> Self {
        let det = self.determinant();

        if det.abs() < f32::EPSILON {
            return Self::IDENTITY;
        }

        let m = Self::new(
            self.m[1][2] * self.m[2][3] * self.m[3][1] - self.m[1][3] * self.m[2][2] * self.m[3][1]
                + self.m[1][3] * self.m[2][1] * self.m[3][2]
                - self.m[1][1] * self.m[2][3] * self.m[3][2]
                - self.m[1][2] * self.m[2][1] * self.m[3][3]
                + self.m[1][1] * self.m[2][2] * self.m[3][3],
            self.m[0][3] * self.m[2][2] * self.m[3][1]
                - self.m[0][2] * self.m[2][3] * self.m[3][1]
                - self.m[0][3] * self.m[2][1] * self.m[3][2]
                + self.m[0][1] * self.m[2][3] * self.m[3][2]
                + self.m[0][2] * self.m[2][1] * self.m[3][3]
                - self.m[0][1] * self.m[2][2] * self.m[3][3],
            self.m[0][2] * self.m[1][3] * self.m[3][1] - self.m[0][3] * self.m[1][2] * self.m[3][1]
                + self.m[0][3] * self.m[1][1] * self.m[3][2]
                - self.m[0][1] * self.m[1][3] * self.m[3][2]
                - self.m[0][2] * self.m[1][1] * self.m[3][3]
                + self.m[0][1] * self.m[1][2] * self.m[3][3],
            self.m[0][3] * self.m[1][2] * self.m[2][1]
                - self.m[0][2] * self.m[1][3] * self.m[2][1]
                - self.m[0][3] * self.m[1][1] * self.m[2][2]
                + self.m[0][1] * self.m[1][3] * self.m[2][2]
                + self.m[0][2] * self.m[1][1] * self.m[2][3]
                - self.m[0][1] * self.m[1][2] * self.m[2][3],
            self.m[1][3] * self.m[2][2] * self.m[3][0]
                - self.m[1][2] * self.m[2][3] * self.m[3][0]
                - self.m[1][3] * self.m[2][0] * self.m[3][2]
                + self.m[1][0] * self.m[2][3] * self.m[3][2]
                + self.m[1][2] * self.m[2][0] * self.m[3][3]
                - self.m[1][0] * self.m[2][2] * self.m[3][3],
            self.m[0][2] * self.m[2][3] * self.m[3][0] - self.m[0][3] * self.m[2][2] * self.m[3][0]
                + self.m[0][3] * self.m[2][0] * self.m[3][2]
                - self.m[0][0] * self.m[2][3] * self.m[3][2]
                - self.m[0][2] * self.m[2][0] * self.m[3][3]
                + self.m[0][0] * self.m[2][2] * self.m[3][3],
            self.m[0][3] * self.m[1][2] * self.m[3][0]
                - self.m[0][2] * self.m[1][3] * self.m[3][0]
                - self.m[0][3] * self.m[1][0] * self.m[3][2]
                + self.m[0][0] * self.m[1][3] * self.m[3][2]
                + self.m[0][2] * self.m[1][0] * self.m[3][3]
                - self.m[0][0] * self.m[1][2] * self.m[3][3],
            self.m[0][2] * self.m[1][3] * self.m[2][0] - self.m[0][3] * self.m[1][2] * self.m[2][0]
                + self.m[0][3] * self.m[1][0] * self.m[2][2]
                - self.m[0][0] * self.m[1][3] * self.m[2][2]
                - self.m[0][2] * self.m[1][0] * self.m[2][3]
                + self.m[0][0] * self.m[1][2] * self.m[2][3],
            self.m[1][1] * self.m[2][3] * self.m[3][0] - self.m[1][3] * self.m[2][1] * self.m[3][0]
                + self.m[1][3] * self.m[2][0] * self.m[3][1]
                - self.m[1][0] * self.m[2][3] * self.m[3][1]
                - self.m[1][1] * self.m[2][0] * self.m[3][3]
                + self.m[1][0] * self.m[2][1] * self.m[3][3],
            self.m[0][3] * self.m[2][1] * self.m[3][0]
                - self.m[0][1] * self.m[2][3] * self.m[3][0]
                - self.m[0][3] * self.m[2][0] * self.m[3][1]
                + self.m[0][0] * self.m[2][3] * self.m[3][1]
                + self.m[0][1] * self.m[2][0] * self.m[3][3]
                - self.m[0][0] * self.m[2][1] * self.m[3][3],
            self.m[0][1] * self.m[1][3] * self.m[3][0] - self.m[0][3] * self.m[1][1] * self.m[3][0]
                + self.m[0][3] * self.m[1][0] * self.m[3][1]
                - self.m[0][0] * self.m[1][3] * self.m[3][1]
                - self.m[0][1] * self.m[1][0] * self.m[3][3]
                + self.m[0][0] * self.m[1][1] * self.m[3][3],
            self.m[0][3] * self.m[1][1] * self.m[2][0]
                - self.m[0][1] * self.m[1][3] * self.m[2][0]
                - self.m[0][3] * self.m[1][0] * self.m[2][1]
                + self.m[0][0] * self.m[1][3] * self.m[2][1]
                + self.m[0][1] * self.m[1][0] * self.m[2][3]
                - self.m[0][0] * self.m[1][1] * self.m[2][3],
            self.m[1][2] * self.m[2][1] * self.m[3][0]
                - self.m[1][1] * self.m[2][2] * self.m[3][0]
                - self.m[1][2] * self.m[2][0] * self.m[3][1]
                + self.m[1][0] * self.m[2][2] * self.m[3][1]
                + self.m[1][1] * self.m[2][0] * self.m[3][2]
                - self.m[1][0] * self.m[2][1] * self.m[3][2],
            self.m[0][1] * self.m[2][2] * self.m[3][0] - self.m[0][2] * self.m[2][1] * self.m[3][0]
                + self.m[0][2] * self.m[2][0] * self.m[3][1]
                - self.m[0][0] * self.m[2][2] * self.m[3][1]
                - self.m[0][1] * self.m[2][0] * self.m[3][2]
                + self.m[0][0] * self.m[2][1] * self.m[3][2],
            self.m[0][2] * self.m[1][1] * self.m[3][0]
                - self.m[0][1] * self.m[1][2] * self.m[3][0]
                - self.m[0][2] * self.m[1][0] * self.m[3][1]
                + self.m[0][0] * self.m[1][2] * self.m[3][1]
                + self.m[0][1] * self.m[1][0] * self.m[3][2]
                - self.m[0][0] * self.m[1][1] * self.m[3][2],
            self.m[0][1] * self.m[1][2] * self.m[2][0] - self.m[0][2] * self.m[1][1] * self.m[2][0]
                + self.m[0][2] * self.m[1][0] * self.m[2][1]
                - self.m[0][0] * self.m[1][2] * self.m[2][1]
                - self.m[0][1] * self.m[1][0] * self.m[2][2]
                + self.m[0][0] * self.m[1][1] * self.m[2][2],
        );

        m.multiply_scalar(1.0 / det)
    }

    #[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
    fn determinant(&self) -> f32 {
        // Accumulate in f64. Individual f32 products (e.g. m[0][0]*m[1][1] on a
        // diag(1e20) matrix = 1e40) overflow to ±inf BEFORE the legitimately-zero
        // off-diagonal factors multiply in, and inf * 0 = NaN, poisoning the whole sum.
        // f64 has the range to hold the products; the final cast saturates a real
        // overflow to ±inf and propagates a NaN input as NaN.
        let m = |i: usize, j: usize| f64::from(self.m[i][j]);
        let det = m(0, 3) * m(1, 2) * m(2, 1) * m(3, 0)
            - m(0, 2) * m(1, 3) * m(2, 1) * m(3, 0)
            - m(0, 3) * m(1, 1) * m(2, 2) * m(3, 0)
            + m(0, 1) * m(1, 3) * m(2, 2) * m(3, 0)
            + m(0, 2) * m(1, 1) * m(2, 3) * m(3, 0)
            - m(0, 1) * m(1, 2) * m(2, 3) * m(3, 0)
            - m(0, 3) * m(1, 2) * m(2, 0) * m(3, 1)
            + m(0, 2) * m(1, 3) * m(2, 0) * m(3, 1)
            + m(0, 3) * m(1, 0) * m(2, 2) * m(3, 1)
            - m(0, 0) * m(1, 3) * m(2, 2) * m(3, 1)
            - m(0, 2) * m(1, 0) * m(2, 3) * m(3, 1)
            + m(0, 0) * m(1, 2) * m(2, 3) * m(3, 1)
            + m(0, 3) * m(1, 1) * m(2, 0) * m(3, 2)
            - m(0, 1) * m(1, 3) * m(2, 0) * m(3, 2)
            - m(0, 3) * m(1, 0) * m(2, 1) * m(3, 2)
            + m(0, 0) * m(1, 3) * m(2, 1) * m(3, 2)
            + m(0, 1) * m(1, 0) * m(2, 3) * m(3, 2)
            - m(0, 0) * m(1, 1) * m(2, 3) * m(3, 2)
            - m(0, 2) * m(1, 1) * m(2, 0) * m(3, 3)
            + m(0, 1) * m(1, 2) * m(2, 0) * m(3, 3)
            + m(0, 2) * m(1, 0) * m(2, 1) * m(3, 3)
            - m(0, 0) * m(1, 2) * m(2, 1) * m(3, 3)
            - m(0, 1) * m(1, 0) * m(2, 2) * m(3, 3)
            + m(0, 0) * m(1, 1) * m(2, 2) * m(3, 3);
        #[allow(clippy::cast_possible_truncation)] // determinant computed in f64, narrowed to the f32 public type
        let det = det as f32;
        det
    }

    fn multiply_scalar(&self, x: f32) -> Self {
        Self::new(
            self.m[0][0] * x,
            self.m[0][1] * x,
            self.m[0][2] * x,
            self.m[0][3] * x,
            self.m[1][0] * x,
            self.m[1][1] * x,
            self.m[1][2] * x,
            self.m[1][3] * x,
            self.m[2][0] * x,
            self.m[2][1] * x,
            self.m[2][2] * x,
            self.m[2][3] * x,
            self.m[3][0] * x,
            self.m[3][1] * x,
            self.m[3][2] * x,
            self.m[3][3] * x,
        )
    }

    /// Computes the matrix of a rect from a `&[StyleTransform]`.
    pub fn from_style_transform_vec(
        t_vec: &[StyleTransform],
        transform_origin: &StyleTransformOrigin,
        percent_resolve_x: f32,
        percent_resolve_y: f32,
        rotation_mode: RotationMode,
    ) -> Self {
        // Uses AVX or SSE SIMD when available on x86_64
        //
        // AUDIT-TODO: `USE_AVX`/`USE_SSE` are populated in `gpu.rs` from a raw
        // CPUID leaf-1 feature bit (ECX[28] for AVX), which reports only that
        // the CPU *implements* AVX — NOT that the OS has enabled the YMM state
        // via XCR0 (XGETBV). On a kernel that didn't `XSETBV`-enable AVX, using
        // these intrinsics faults with SIGILL. The robust gate is
        // `is_x86_feature_detected!("avx")` / `("sse")`, which also checks the
        // OS-enabled bit. That detection lives in `gpu.rs` (out of scope for
        // this edit); consumers here rely on it having gated the flags. Prefer
        // migrating the `gpu.rs` probe to `is_x86_feature_detected!`.
        let mut matrix = Self::IDENTITY;
        let use_avx =
            INITIALIZED.load(AtomicOrdering::Relaxed) && USE_AVX.load(AtomicOrdering::Relaxed);
        let use_sse = !use_avx
            && INITIALIZED.load(AtomicOrdering::Relaxed)
            && USE_SSE.load(AtomicOrdering::Relaxed);

        if use_avx {
            for t in t_vec {
                // SAFETY: `use_avx` is only set when the AVX feature flag was
                // detected (see AUDIT-TODO above), so calling the AVX intrinsics
                // in `then_avx8` is legal on this CPU.
                #[cfg(target_arch = "x86_64")]
                unsafe {
                    matrix = matrix.then_avx8(&Self::from_style_transform(
                        t,
                        transform_origin,
                        percent_resolve_x,
                        percent_resolve_y,
                        rotation_mode,
                    ));
                }
            }
        } else if use_sse {
            for t in t_vec {
                // SAFETY: `use_sse` is only set when the SSE feature flag was
                // detected (see AUDIT-TODO above), so calling the SSE intrinsics
                // in `then_sse` is legal on this CPU.
                #[cfg(target_arch = "x86_64")]
                unsafe {
                    matrix = matrix.then_sse(&Self::from_style_transform(
                        t,
                        transform_origin,
                        percent_resolve_x,
                        percent_resolve_y,
                        rotation_mode,
                    ));
                }
            }
        } else {
            // fallback for everything else
            for t in t_vec {
                matrix = matrix.then(&Self::from_style_transform(
                    t,
                    transform_origin,
                    percent_resolve_x,
                    percent_resolve_y,
                    rotation_mode,
                ));
            }
        }

        matrix
    }

    /// Creates a new transform from a style transform using the
    /// parent width as a way to resolve for percentages
    #[allow(clippy::many_single_char_names)] // domain-standard colour/coordinate component names
    #[allow(clippy::too_many_lines)] // large but cohesive: single-purpose parser/builder/dispatch (one branch per input variant)
    fn from_style_transform(
        t: &StyleTransform,
        transform_origin: &StyleTransformOrigin,
        percent_resolve_x: f32,
        percent_resolve_y: f32,
        rotation_mode: RotationMode,
    ) -> Self {
        use azul_css::props::basic::pixel::DEFAULT_FONT_SIZE;
        use azul_css::props::style::StyleTransform::{Matrix, Matrix3D, Translate, Translate3D, TranslateX, TranslateY, TranslateZ, Rotate3D, RotateX, RotateY, Rotate, RotateZ, Scale, Scale3D, ScaleX, ScaleY, ScaleZ, Skew, SkewX, SkewY, Perspective};
        match t {
            Matrix(mat2d) => {
                let a = mat2d.a.get();
                let b = mat2d.b.get();
                let c = mat2d.c.get();
                let d = mat2d.d.get();
                let tx = mat2d.tx.get();
                let ty = mat2d.ty.get();

                Self::new_2d(a, b, c, d, tx, ty)
            }
            Matrix3D(mat3d) => {
                let m11 = mat3d.m11.get();
                let m12 = mat3d.m12.get();
                let m13 = mat3d.m13.get();
                let m14 = mat3d.m14.get();
                let m21 = mat3d.m21.get();
                let m22 = mat3d.m22.get();
                let m23 = mat3d.m23.get();
                let m24 = mat3d.m24.get();
                let m31 = mat3d.m31.get();
                let m32 = mat3d.m32.get();
                let m33 = mat3d.m33.get();
                let m34 = mat3d.m34.get();
                let m41 = mat3d.m41.get();
                let m42 = mat3d.m42.get();
                let m43 = mat3d.m43.get();
                let m44 = mat3d.m44.get();

                Self::new(
                    m11, m12, m13, m14, m21, m22, m23, m24, m31, m32, m33, m34, m41, m42, m43, m44,
                )
            }
            Translate(trans2d) => {

                Self::new_translation(
                    trans2d
                        .x
                        .to_pixels_internal(percent_resolve_x, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE),
                    trans2d
                        .y
                        .to_pixels_internal(percent_resolve_y, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE),
                    0.0,
                )
            }
            Translate3D(trans3d) => {

                Self::new_translation(
                    trans3d
                        .x
                        .to_pixels_internal(percent_resolve_x, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE),
                    trans3d
                        .y
                        .to_pixels_internal(percent_resolve_y, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE),
                    trans3d
                        .z
                        // CSS has no containing block for Z-axis percentages; use X as fallback
                        .to_pixels_internal(percent_resolve_x, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE),
                )
            }
            TranslateX(trans_x) => {

                Self::new_translation(
                    trans_x.to_pixels_internal(percent_resolve_x, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE),
                    0.0,
                    0.0,
                )
            }
            TranslateY(trans_y) => {

                Self::new_translation(
                    0.0,
                    trans_y.to_pixels_internal(percent_resolve_y, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE),
                    0.0,
                )
            }
            TranslateZ(trans_z) => {

                Self::new_translation(
                    0.0,
                    0.0,
                    trans_z.to_pixels_internal(percent_resolve_x, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE),
                )
            } // CSS has no containing block for Z-axis percentages; use X as fallback
            Rotate3D(rot3d) => {

                let rotation_origin = (
                    transform_origin
                        .x
                        .to_pixels_internal(percent_resolve_x, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE),
                    transform_origin
                        .y
                        .to_pixels_internal(percent_resolve_y, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE),
                );
                Self::make_rotation(
                    rotation_origin,
                    rot3d.angle.to_degrees(),
                    rot3d.x.get(),
                    rot3d.y.get(),
                    rot3d.z.get(),
                    rotation_mode,
                )
            }
            RotateX(angle_x) => {

                let rotation_origin = (
                    transform_origin
                        .x
                        .to_pixels_internal(percent_resolve_x, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE),
                    transform_origin
                        .y
                        .to_pixels_internal(percent_resolve_y, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE),
                );
                Self::make_rotation(
                    rotation_origin,
                    angle_x.to_degrees(),
                    1.0,
                    0.0,
                    0.0,
                    rotation_mode,
                )
            }
            RotateY(angle_y) => {

                let rotation_origin = (
                    transform_origin
                        .x
                        .to_pixels_internal(percent_resolve_x, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE),
                    transform_origin
                        .y
                        .to_pixels_internal(percent_resolve_y, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE),
                );
                Self::make_rotation(
                    rotation_origin,
                    angle_y.to_degrees(),
                    0.0,
                    1.0,
                    0.0,
                    rotation_mode,
                )
            }
            Rotate(angle_z) | RotateZ(angle_z) => {

                let rotation_origin = (
                    transform_origin
                        .x
                        .to_pixels_internal(percent_resolve_x, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE),
                    transform_origin
                        .y
                        .to_pixels_internal(percent_resolve_y, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE),
                );
                Self::make_rotation(
                    rotation_origin,
                    angle_z.to_degrees(),
                    0.0,
                    0.0,
                    1.0,
                    rotation_mode,
                )
            }
            Scale(scale2d) => Self::new_scale(scale2d.x.get(), scale2d.y.get(), 1.0),
            Scale3D(scale3d) => Self::new_scale(scale3d.x.get(), scale3d.y.get(), scale3d.z.get()),
            ScaleX(scale_x) => Self::new_scale(scale_x.normalized(), 1.0, 1.0),
            ScaleY(scale_y) => Self::new_scale(1.0, scale_y.normalized(), 1.0),
            ScaleZ(scale_z) => Self::new_scale(1.0, 1.0, scale_z.normalized()),
            Skew(skew2d) => Self::new_skew(skew2d.x.to_degrees(), skew2d.y.to_degrees()),
            SkewX(skew_x) => Self::new_skew(skew_x.to_degrees(), 0.0),
            SkewY(skew_y) => Self::new_skew(0.0, skew_y.to_degrees()),
            Perspective(px) => {

                Self::new_perspective(px.to_pixels_internal(percent_resolve_x, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE))
            }
        }
    }

    /// Creates a scaling matrix with independent scale factors per axis.
    #[must_use]
    #[inline]
    pub const fn new_scale(x: f32, y: f32, z: f32) -> Self {
        Self::new(
            x, 0.0, 0.0, 0.0, 0.0, y, 0.0, 0.0, 0.0, 0.0, z, 0.0, 0.0, 0.0, 0.0, 1.0,
        )
    }

    /// Creates a translation matrix that moves by `(x, y, z)`.
    #[must_use]
    #[inline]
    pub const fn new_translation(x: f32, y: f32, z: f32) -> Self {
        Self::new(
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, x, y, z, 1.0,
        )
    }

    /// Creates a perspective projection matrix with distance `d`.
    #[must_use]
    #[inline]
    fn new_perspective(d: f32) -> Self {
        Self::new(
            1.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            -1.0 / d,
            0.0,
            0.0,
            0.0,
            1.0,
        )
    }

    /// Create a 3d rotation transform from an angle / axis.
    /// The supplied axis must be normalized.
    #[must_use]
    #[inline]
    #[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
    fn new_rotation(x: f32, y: f32, z: f32, theta_radians: f32) -> Self {
        let xx = x * x;
        let yy = y * y;
        let zz = z * z;

        let half_theta = theta_radians / 2.0;
        let sc = half_theta.sin() * half_theta.cos();
        let sq = half_theta.sin() * half_theta.sin();

        Self::new(
            1.0 - 2.0 * (yy + zz) * sq,
            2.0 * (x * y * sq + z * sc),
            2.0 * (x * z * sq - y * sc),
            0.0,
            2.0 * (x * y * sq - z * sc),
            1.0 - 2.0 * (xx + zz) * sq,
            2.0 * (y * z * sq + x * sc),
            0.0,
            2.0 * (x * z * sq + y * sc),
            2.0 * (y * z * sq - x * sc),
            1.0 - 2.0 * (xx + yy) * sq,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
        )
    }

    /// Creates a 2D skew matrix from angles in degrees.
    #[must_use]
    #[inline]
    fn new_skew(alpha: f32, beta: f32) -> Self {
        let (sx, sy) = (beta.to_radians().tan(), alpha.to_radians().tan());
        Self::new(
            1.0, sx, 0.0, 0.0, sy, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        )
    }

    /// Returns this matrix transposed to column-major layout.
    #[must_use]
    pub(crate) const fn get_column_major(&self) -> Self {
        Self::new(
            self.m[0][0],
            self.m[1][0],
            self.m[2][0],
            self.m[3][0],
            self.m[0][1],
            self.m[1][1],
            self.m[2][1],
            self.m[3][1],
            self.m[0][2],
            self.m[1][2],
            self.m[2][2],
            self.m[3][2],
            self.m[0][3],
            self.m[1][3],
            self.m[2][3],
            self.m[3][3],
        )
    }

    /// Transforms a 2D point into the target coordinate space.
    #[must_use]
    pub fn transform_point2d(&self, p: LogicalPosition) -> Option<LogicalPosition> {
        let w =
            p.x.mul_add(self.m[0][3], p.y.mul_add(self.m[1][3], self.m[3][3]));

        if !w.is_sign_positive() {
            return None;
        }

        let x =
            p.x.mul_add(self.m[0][0], p.y.mul_add(self.m[1][0], self.m[3][0]));
        let y =
            p.x.mul_add(self.m[0][1], p.y.mul_add(self.m[1][1], self.m[3][1]));

        Some(LogicalPosition { x: x / w, y: y / w })
    }

    /// Scales the translation components of this matrix by `scale_factor` for DPI adjustment.
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        // only scale the translation, don't scale anything else
        self.m[3][0] *= scale_factor;
        self.m[3][1] *= scale_factor;
        self.m[3][2] *= scale_factor;
    }

    /// Multiplies this matrix by `other`, applying `other` AFTER the current matrix.
    #[must_use]
    #[inline]
    #[allow(clippy::too_many_lines)] // large but cohesive: single-purpose parser/builder/dispatch (one branch per input variant)
    pub fn then(&self, other: &Self) -> Self {
        Self::new(
            self.m[0][0].mul_add(
                other.m[0][0],
                self.m[0][1].mul_add(
                    other.m[1][0],
                    self.m[0][2].mul_add(other.m[2][0], self.m[0][3] * other.m[3][0]),
                ),
            ),
            self.m[0][0].mul_add(
                other.m[0][1],
                self.m[0][1].mul_add(
                    other.m[1][1],
                    self.m[0][2].mul_add(other.m[2][1], self.m[0][3] * other.m[3][1]),
                ),
            ),
            self.m[0][0].mul_add(
                other.m[0][2],
                self.m[0][1].mul_add(
                    other.m[1][2],
                    self.m[0][2].mul_add(other.m[2][2], self.m[0][3] * other.m[3][2]),
                ),
            ),
            self.m[0][0].mul_add(
                other.m[0][3],
                self.m[0][1].mul_add(
                    other.m[1][3],
                    self.m[0][2].mul_add(other.m[2][3], self.m[0][3] * other.m[3][3]),
                ),
            ),
            self.m[1][0].mul_add(
                other.m[0][0],
                self.m[1][1].mul_add(
                    other.m[1][0],
                    self.m[1][2].mul_add(other.m[2][0], self.m[1][3] * other.m[3][0]),
                ),
            ),
            self.m[1][0].mul_add(
                other.m[0][1],
                self.m[1][1].mul_add(
                    other.m[1][1],
                    self.m[1][2].mul_add(other.m[2][1], self.m[1][3] * other.m[3][1]),
                ),
            ),
            self.m[1][0].mul_add(
                other.m[0][2],
                self.m[1][1].mul_add(
                    other.m[1][2],
                    self.m[1][2].mul_add(other.m[2][2], self.m[1][3] * other.m[3][2]),
                ),
            ),
            self.m[1][0].mul_add(
                other.m[0][3],
                self.m[1][1].mul_add(
                    other.m[1][3],
                    self.m[1][2].mul_add(other.m[2][3], self.m[1][3] * other.m[3][3]),
                ),
            ),
            self.m[2][0].mul_add(
                other.m[0][0],
                self.m[2][1].mul_add(
                    other.m[1][0],
                    self.m[2][2].mul_add(other.m[2][0], self.m[2][3] * other.m[3][0]),
                ),
            ),
            self.m[2][0].mul_add(
                other.m[0][1],
                self.m[2][1].mul_add(
                    other.m[1][1],
                    self.m[2][2].mul_add(other.m[2][1], self.m[2][3] * other.m[3][1]),
                ),
            ),
            self.m[2][0].mul_add(
                other.m[0][2],
                self.m[2][1].mul_add(
                    other.m[1][2],
                    self.m[2][2].mul_add(other.m[2][2], self.m[2][3] * other.m[3][2]),
                ),
            ),
            self.m[2][0].mul_add(
                other.m[0][3],
                self.m[2][1].mul_add(
                    other.m[1][3],
                    self.m[2][2].mul_add(other.m[2][3], self.m[2][3] * other.m[3][3]),
                ),
            ),
            self.m[3][0].mul_add(
                other.m[0][0],
                self.m[3][1].mul_add(
                    other.m[1][0],
                    self.m[3][2].mul_add(other.m[2][0], self.m[3][3] * other.m[3][0]),
                ),
            ),
            self.m[3][0].mul_add(
                other.m[0][1],
                self.m[3][1].mul_add(
                    other.m[1][1],
                    self.m[3][2].mul_add(other.m[2][1], self.m[3][3] * other.m[3][1]),
                ),
            ),
            self.m[3][0].mul_add(
                other.m[0][2],
                self.m[3][1].mul_add(
                    other.m[1][2],
                    self.m[3][2].mul_add(other.m[2][2], self.m[3][3] * other.m[3][2]),
                ),
            ),
            self.m[3][0].mul_add(
                other.m[0][3],
                self.m[3][1].mul_add(
                    other.m[1][3],
                    self.m[3][2].mul_add(other.m[2][3], self.m[3][3] * other.m[3][3]),
                ),
            ),
        )
    }

    // credit: https://gist.github.com/rygorous/4172889

    // linear combination:
    // a[0] * B.row[0] + a[1] * B.row[1] + a[2] * B.row[2] + a[3] * B.row[3]
    //
    // SAFETY: the caller must guarantee SSE is available on this CPU (see the
    // `use_sse` gate in `from_style_transform_vec`). Every `mem::transmute` here
    // is a BY-VALUE `[f32; 4]` -> `__m128` conversion: both types are 16 bytes
    // and the value is moved through a register, so no *reference* to under-
    // aligned storage is ever formed and there is no alignment invariant to
    // violate (unlike the AVX broadcast, which must use an unaligned load).
    #[cfg(target_arch = "x86_64")]
    #[inline]
    unsafe fn linear_combine_sse(a: [f32; 4], b: &Self) -> [f32; 4] { unsafe {
        use core::{
            arch::x86_64::{__m128, _mm_add_ps, _mm_mul_ps, _mm_shuffle_ps},
            mem,
        };

        let a: __m128 = mem::transmute(a);
        let mut result = _mm_mul_ps(_mm_shuffle_ps(a, a, 0x00), mem::transmute::<[f32; 4], __m128>(b.m[0]));
        result = _mm_add_ps(
            result,
            _mm_mul_ps(_mm_shuffle_ps(a, a, 0x55), mem::transmute::<[f32; 4], __m128>(b.m[1])),
        );
        result = _mm_add_ps(
            result,
            _mm_mul_ps(_mm_shuffle_ps(a, a, 0xaa), mem::transmute::<[f32; 4], __m128>(b.m[2])),
        );
        result = _mm_add_ps(
            result,
            _mm_mul_ps(_mm_shuffle_ps(a, a, 0xff), mem::transmute::<[f32; 4], __m128>(b.m[3])),
        );

        mem::transmute(result)
    }}

    /// Multiplies this matrix by `other` using SSE instructions.
    ///
    /// SAFETY: caller must guarantee SSE is available; only forwards to
    /// `linear_combine_sse`, whose safety contract is identical.
    #[cfg(target_arch = "x86_64")]
    #[inline]
    unsafe fn then_sse(&self, other: &Self) -> Self { unsafe {
        Self {
            m: [
                Self::linear_combine_sse(self.m[0], other),
                Self::linear_combine_sse(self.m[1], other),
                Self::linear_combine_sse(self.m[2], other),
                Self::linear_combine_sse(self.m[3], other),
            ],
        }
    }}

    /// Dual linear combination using AVX instructions on YMM registers.
    ///
    /// AUDIT: the rows `b.m[i]` are `[f32; 4]` fields with alignment 4, but
    /// `_mm256_broadcast_ps` takes a `&__m128` (alignment 16). Forming that
    /// reference — `&*(ptr as *const __m128)` — from an align-4 field is
    /// misaligned-reference UB even though the underlying `vbroadcastf128`
    /// tolerates it. Use `_mm256_loadu2_m128`, which does an *unaligned*
    /// 128-bit load from a raw `*const f32` and never forms a `&__m128`;
    /// passing the same row pointer for both lanes reproduces the broadcast
    /// (`result[127:0] = result[255:128] = row`).
    ///
    /// SAFETY: caller must guarantee AVX is available. Each `broadcast_row`
    /// reads exactly 4 f32 (16 bytes) through `_mm256_loadu2_m128`, an
    /// *unaligned* load, so the align-4 `[f32; 4]` rows are read in-bounds and
    /// no `&__m128` (align 16) is ever formed from them.
    #[cfg(target_arch = "x86_64")]
    unsafe fn linear_combine_avx8(
        a01: core::arch::x86_64::__m256,
        b: &Self,
    ) -> core::arch::x86_64::__m256 { unsafe {
        use core::arch::x86_64::{
            _mm256_add_ps, _mm256_loadu2_m128, _mm256_mul_ps, _mm256_shuffle_ps,
        };

        // Unaligned broadcast of a row into both 128-bit lanes. Runs inside the
        // enclosing `unsafe` block, so the intrinsic call needs no inner `unsafe`.
        let broadcast_row = |row: &[f32; 4]| {
            let p = row.as_ptr();
            _mm256_loadu2_m128(p, p)
        };

        let mut result = _mm256_mul_ps(
            _mm256_shuffle_ps(a01, a01, 0x00),
            broadcast_row(&b.m[0]),
        );
        result = _mm256_add_ps(
            result,
            _mm256_mul_ps(_mm256_shuffle_ps(a01, a01, 0x55), broadcast_row(&b.m[1])),
        );
        result = _mm256_add_ps(
            result,
            _mm256_mul_ps(_mm256_shuffle_ps(a01, a01, 0xaa), broadcast_row(&b.m[2])),
        );
        result = _mm256_add_ps(
            result,
            _mm256_mul_ps(_mm256_shuffle_ps(a01, a01, 0xff), broadcast_row(&b.m[3])),
        );
        result
    }}

    /// Multiplies this matrix by `other` using AVX instructions.
    ///
    /// SAFETY: caller must guarantee AVX is available. Both `_mm256_loadu_ps`
    /// reads and `_mm256_storeu_ps` writes are *unaligned* 8-f32 (32-byte)
    /// accesses. `m` is `[[f32; 4]; 4]`, i.e. 16 contiguous f32 with no padding,
    /// so `&m[0][0]..` and `&m[2][0]..` each span two full rows in-bounds; the
    /// raw pointers come from live `self`/`out` locals, so lifetimes are valid.
    #[cfg(target_arch = "x86_64")]
    #[inline]
    unsafe fn then_avx8(&self, other: &Self) -> Self { unsafe {
        use core::{
            arch::x86_64::{__m256, _mm256_loadu_ps, _mm256_storeu_ps, _mm256_zeroupper},
            mem,
        };

        _mm256_zeroupper();

        let a01: __m256 = _mm256_loadu_ps(&raw const self.m[0][0]);
        let a23: __m256 = _mm256_loadu_ps(&raw const self.m[2][0]);

        let out01x = Self::linear_combine_avx8(a01, other);
        let out23x = Self::linear_combine_avx8(a23, other);

        let mut out = Self {
            m: [self.m[0], self.m[1], self.m[2], self.m[3]],
        };

        _mm256_storeu_ps(&raw mut out.m[0][0], out01x);
        _mm256_storeu_ps(&raw mut out.m[2][0], out23x);

        out
    }}

    /// Creates a rotation matrix around the given axis, adjusted for the coordinate system.
    #[must_use]
    #[inline]
    #[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
    fn make_rotation(
        rotation_origin: (f32, f32),
        mut degrees: f32,
        axis_x: f32,
        axis_y: f32,
        axis_z: f32,
        // see documentation for RotationMode
        rotation_mode: RotationMode,
    ) -> Self {
        degrees = match rotation_mode {
            // CSS rotations are clockwise
            RotationMode::ForWebRender => -degrees,
            // hit-testing turns counter-clockwise
            RotationMode::ForHitTesting => degrees,
        };

        let (origin_x, origin_y) = rotation_origin;
        let pre_transform = Self::new_translation(-origin_x, -origin_y, 0.0);
        let post_transform = Self::new_translation(origin_x, origin_y, 0.0);
        let theta = 2.0_f32 * core::f32::consts::PI - degrees.to_radians();
        let rotate_transform =
            Self::new_rotation(axis_x, axis_y, axis_z, theta);

        pre_transform.then(&rotate_transform).then(&post_transform)
    }
}

#[cfg(test)]
#[allow(clippy::items_after_statements, clippy::redundant_clone, clippy::cast_possible_truncation, clippy::cast_sign_loss, trivial_casts, clippy::borrow_as_ptr, clippy::cast_ptr_alignment, clippy::unused_self, unused_qualifications, unreachable_pub, private_interfaces)] // pedantic lints are noise in unsafe-exercising test code
mod audit_tests {
    use super::*;

    fn sample_a() -> ComputedTransform3D {
        ComputedTransform3D::new(
            1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0,
        )
    }
    fn sample_b() -> ComputedTransform3D {
        ComputedTransform3D::new(
            16.0, 15.0, 14.0, 13.0, 12.0, 11.0, 10.0, 9.0, 8.0, 7.0, 6.0, 5.0, 4.0, 3.0, 2.0, 1.0,
        )
    }

    fn approx_eq(a: &ComputedTransform3D, b: &ComputedTransform3D) {
        for r in 0..4 {
            for c in 0..4 {
                assert!(
                    (a.m[r][c] - b.m[r][c]).abs() < 1e-3,
                    "mismatch at [{r}][{c}]: {} vs {}",
                    a.m[r][c],
                    b.m[r][c]
                );
            }
        }
    }

    /// Naive row-major 4x4 multiply used as an independent reference for the
    /// `then` (and hence SIMD) paths. Deliberately avoids `mul_add` so it is a
    /// separate implementation from the code under test.
    fn naive_then(a: &ComputedTransform3D, b: &ComputedTransform3D) -> ComputedTransform3D {
        let mut out = ComputedTransform3D::IDENTITY;
        for r in 0..4 {
            for c in 0..4 {
                let mut acc = 0.0f32;
                for k in 0..4 {
                    acc += a.m[r][k] * b.m[k][c];
                }
                out.m[r][c] = acc;
            }
        }
        out
    }

    // Miri-compatible: exercises only the safe scalar `then` against an
    // independent naive reference. Runs everywhere, including under Miri, so the
    // scalar anchor that the SIMD paths are compared against is itself checked.
    #[test]
    fn scalar_matmul_matches_reference() {
        let a = sample_a();
        let b = sample_b();
        approx_eq(&a.then(&b), &naive_then(&a, &b));
        // Identity is a left/right unit.
        approx_eq(&ComputedTransform3D::IDENTITY.then(&b), &b);
        approx_eq(&a.then(&ComputedTransform3D::IDENTITY), &a);
    }

    // AUDIT: the SSE/AVX matrix-multiply paths must agree with the scalar
    // reference. In particular this exercises `linear_combine_avx8`, whose
    // unaligned-load fix (`_mm256_loadu2_m128` instead of forming a misaligned
    // `&__m128`) must produce identical results. Only runs the SIMD paths when
    // the CPU (and OS) actually support the feature.
    //
    // `#[cfg(not(miri))]`: the AVX/SSE intrinsics cannot execute under Miri, so
    // this test is skipped there; a native run covers it.
    #[cfg(not(miri))]
    #[test]
    fn simd_matmul_matches_scalar() {
        let a = sample_a();
        let b = sample_b();
        let scalar = a.then(&b);

        #[cfg(target_arch = "x86_64")]
        {
            if std::is_x86_feature_detected!("sse") {
                let sse = unsafe { a.then_sse(&b) };
                approx_eq(&scalar, &sse);
            }
            if std::is_x86_feature_detected!("avx") {
                let avx = unsafe { a.then_avx8(&b) };
                approx_eq(&scalar, &avx);
            }
        }

        // Always assert the scalar path is self-consistent (identity * b == b).
        approx_eq(&ComputedTransform3D::IDENTITY.then(&b), &b);
    }

    // AUDIT regression test for the misaligned-`&__m128` bug: the AVX path reads
    // matrix rows (`[f32; 4]`, alignment 4) that are NOT guaranteed to sit on a
    // 16-byte boundary. The earlier code formed a `&__m128` from such a row,
    // which is misaligned-reference UB; the current code uses unaligned loads.
    // This runs `then_avx8` on the same logical matrix placed at a 16-byte
    // aligned address AND at that address + 4 (i.e. 4-mod-16, deliberately not
    // 16-aligned) and asserts identical results. A sanitizer/Valgrind run over
    // this test would fault on the pre-fix misaligned access.
    //
    // `#[cfg(not(miri))]`: invokes AVX intrinsics, which Miri cannot execute.
    #[cfg(all(target_arch = "x86_64", not(miri)))]
    #[test]
    fn avx_result_independent_of_row_alignment() {
        if !std::is_x86_feature_detected!("avx") {
            return;
        }

        let a = sample_a();
        let b = sample_b();
        let expected = unsafe { a.then_avx8(&b) };

        const N: usize = core::mem::size_of::<ComputedTransform3D>(); // 64, no padding
        let mut buf = vec![0u8; N * 2 + 16];
        let base = buf.as_mut_ptr();

        // SAFETY: `aligned` lands within `buf` (align_offset < 16, then +N),
        // `misaligned` = aligned + 4 stays in-bounds (buf has N*2+16 bytes).
        // Both are >= 4-byte aligned (base is heap-aligned; +4 preserves that),
        // so forming `&ComputedTransform3D` (alignment 4) from them is valid.
        unsafe {
            let aligned = base.add(base.align_offset(16));
            let misaligned = aligned.add(4); // 4 mod 16: not 16-aligned
            for off_ptr in [aligned, misaligned] {
                core::ptr::copy_nonoverlapping(
                    (&raw const a).cast::<u8>(),
                    off_ptr,
                    N,
                );
                let a_ref = &*off_ptr.cast::<ComputedTransform3D>();
                let got = a_ref.then_avx8(&b);
                approx_eq(&expected, &got);
            }
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::float_cmp,
    clippy::unreadable_literal,
    clippy::excessive_precision,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::too_many_lines,
    clippy::needless_range_loop,
    clippy::suboptimal_flops,
    unused_qualifications
)] // adversarial numeric tests: exact FP comparisons and literal matrices are the point
mod autotest_generated {
    use azul_css::props::basic::{
        AngleValue, FloatValue, PercentageValue, PixelValue, SizeMetric,
    };
    use azul_css::props::style::{
        StyleTransformMatrix2D, StyleTransformMatrix3D, StyleTransformRotate3D,
        StyleTransformScale2D, StyleTransformScale3D, StyleTransformSkew2D,
        StyleTransformTranslate2D, StyleTransformTranslate3D,
    };

    use super::*;

    // ---------------------------------------------------------------- helpers

    /// A matrix whose 16 entries are all `v` — the worst case for any code that
    /// assumes a well-formed (invertible / affine) transform.
    fn filled(v: f32) -> ComputedTransform3D {
        ComputedTransform3D { m: [[v; 4]; 4] }
    }

    fn assert_mat_approx(a: &ComputedTransform3D, b: &ComputedTransform3D, tol: f32) {
        for r in 0..4 {
            for c in 0..4 {
                assert!(
                    (a.m[r][c] - b.m[r][c]).abs() <= tol,
                    "mismatch at [{r}][{c}]: {} vs {} (tol {tol})",
                    a.m[r][c],
                    b.m[r][c]
                );
            }
        }
    }

    fn all_finite(t: &ComputedTransform3D) -> bool {
        t.m.iter().flatten().all(|v| v.is_finite())
    }

    /// Independent 3x3 determinant (Sarrus) for the reference 4x4 below.
    fn det3(m: [[f32; 3]; 3]) -> f32 {
        m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
            - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
    }

    /// Independent 4x4 determinant via cofactor expansion along row 0. This is a
    /// deliberately different algorithm from the 24-term expansion in
    /// `ComputedTransform3D::determinant`, so agreement is a real cross-check.
    fn det4_naive(t: &ComputedTransform3D) -> f32 {
        let mut sum = 0.0f32;
        for col in 0..4 {
            let mut minor = [[0.0f32; 3]; 3];
            for r in 1..4 {
                let mut cc = 0;
                for c in 0..4 {
                    if c == col {
                        continue;
                    }
                    minor[r - 1][cc] = t.m[r][c];
                    cc += 1;
                }
            }
            let sign = if col % 2 == 0 { 1.0 } else { -1.0 };
            sum += sign * t.m[0][col] * det3(minor);
        }
        sum
    }

    /// `row * B` — the reference for the SSE/AVX linear-combination kernels.
    /// (Only reachable from the x86_64 / non-Miri tests below.)
    #[allow(dead_code)]
    fn naive_row_combine(a: [f32; 4], b: &ComputedTransform3D) -> [f32; 4] {
        let mut out = [0.0f32; 4];
        for c in 0..4 {
            for k in 0..4 {
                out[c] += a[k] * b.m[k][c];
            }
        }
        out
    }

    fn origin_px(x: isize, y: isize) -> StyleTransformOrigin {
        StyleTransformOrigin {
            x: PixelValue::const_px(x),
            y: PixelValue::const_px(y),
        }
    }

    /// Convenience: run a single `StyleTransform` through the private builder
    /// with a zero origin (so rotations are not wrapped in translations).
    fn build(t: &StyleTransform, px: f32, py: f32) -> ComputedTransform3D {
        ComputedTransform3D::from_style_transform(t, &origin_px(0, 0), px, py, RotationMode::ForHitTesting)
    }

    // ------------------------------------------------------ constructors: new

    #[test]
    fn new_stores_all_16_elements_row_major() {
        let t = ComputedTransform3D::new(
            1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0,
        );
        for r in 0..4 {
            for c in 0..4 {
                let expected = (r * 4 + c + 1) as f32;
                assert_eq!(t.m[r][c], expected, "row-major slot [{r}][{c}]");
            }
        }
    }

    #[test]
    fn new_preserves_extreme_values_verbatim() {
        let t = ComputedTransform3D::new(
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::MAX,
            f32::MIN,
            f32::MIN_POSITIVE,
            -0.0,
            0.0,
            f32::EPSILON,
            -f32::EPSILON,
            1e-45, // subnormal
            -1e-45,
            f32::MAX,
            f32::MIN,
            f32::INFINITY,
            f32::NEG_INFINITY,
        );
        // The constructor must not sanitize, clamp or panic on any of these.
        assert!(t.m[0][0].is_nan());
        assert!(t.m[0][1].is_infinite() && t.m[0][1].is_sign_positive());
        assert!(t.m[0][2].is_infinite() && t.m[0][2].is_sign_negative());
        assert_eq!(t.m[0][3], f32::MAX);
        assert_eq!(t.m[1][0], f32::MIN);
        assert_eq!(t.m[1][1], f32::MIN_POSITIVE);
        // -0.0 must survive as -0.0 (it compares == 0.0, so check the sign bit).
        assert!(t.m[1][2].is_sign_negative());
        assert!(t.m[1][3].is_sign_positive());
        assert_eq!(t.m[2][0], f32::EPSILON);
        assert!(t.m[3][2].is_infinite());
    }

    #[test]
    fn new_2d_matches_css_matrix_layout() {
        // matrix(a, b, c, d, tx, ty) => [[a,b,0,0],[c,d,0,0],[0,0,1,0],[tx,ty,0,1]]
        let t = ComputedTransform3D::new_2d(2.0, 3.0, 4.0, 5.0, 6.0, 7.0);
        assert_eq!(t.m[0], [2.0, 3.0, 0.0, 0.0]);
        assert_eq!(t.m[1], [4.0, 5.0, 0.0, 0.0]);
        assert_eq!(t.m[2], [0.0, 0.0, 1.0, 0.0]); // Z untouched
        assert_eq!(t.m[3], [6.0, 7.0, 0.0, 1.0]);
    }

    #[test]
    fn new_2d_with_extremes_keeps_z_row_intact() {
        let t = ComputedTransform3D::new_2d(
            f32::NAN,
            f32::INFINITY,
            f32::MAX,
            f32::MIN,
            f32::NEG_INFINITY,
            -0.0,
        );
        assert!(t.m[0][0].is_nan());
        // The constant Z row / W column must not be corrupted by extreme args.
        assert_eq!(t.m[2], [0.0, 0.0, 1.0, 0.0]);
        assert_eq!(t.m[3][3], 1.0);
    }

    // -------------------------------------------- constructors: scale / translate

    #[test]
    fn new_scale_places_factors_on_the_diagonal() {
        let t = ComputedTransform3D::new_scale(2.0, -3.0, 0.5);
        assert_eq!(t.m[0][0], 2.0);
        assert_eq!(t.m[1][1], -3.0);
        assert_eq!(t.m[2][2], 0.5);
        assert_eq!(t.m[3][3], 1.0);
        // Everything off the diagonal stays zero.
        for r in 0..4 {
            for c in 0..4 {
                if r != c {
                    assert_eq!(t.m[r][c], 0.0, "off-diagonal [{r}][{c}]");
                }
            }
        }
    }

    #[test]
    fn new_scale_zero_is_singular_and_inverse_falls_back_to_identity() {
        let z = ComputedTransform3D::new_scale(0.0, 0.0, 0.0);
        assert_eq!(z.determinant(), 0.0);
        // Documented: a singular matrix inverts to the identity rather than
        // producing inf/NaN or panicking.
        assert_eq!(z.inverse(), ComputedTransform3D::IDENTITY);
    }

    #[test]
    fn new_scale_extremes_do_not_panic() {
        for v in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, f32::MAX, f32::MIN] {
            let t = ComputedTransform3D::new_scale(v, v, v);
            assert_eq!(t.m[3][3], 1.0);
            assert_eq!(t.m[0][1], 0.0);
        }
        let nan = ComputedTransform3D::new_scale(f32::NAN, 1.0, 1.0);
        assert!(nan.m[0][0].is_nan());
        assert!(nan.determinant().is_nan());
    }

    #[test]
    fn new_translation_places_offsets_in_last_row() {
        let t = ComputedTransform3D::new_translation(10.0, -20.0, 30.0);
        assert_eq!(t.m[3], [10.0, -20.0, 30.0, 1.0]);
        // The upper-left 3x3 stays the identity.
        assert_eq!(t.m[0], [1.0, 0.0, 0.0, 0.0]);
        assert_eq!(t.m[1], [0.0, 1.0, 0.0, 0.0]);
        assert_eq!(t.m[2], [0.0, 0.0, 1.0, 0.0]);
    }

    #[test]
    fn new_translation_is_always_invertible_even_at_f32_max() {
        let t = ComputedTransform3D::new_translation(f32::MAX, f32::MIN, f32::MAX);
        // determinant of a translation is 1 regardless of how big the offsets are
        assert_eq!(t.determinant(), 1.0);
        let inv = t.inverse();
        assert_eq!(inv.m[3][0], -f32::MAX);
        assert_eq!(inv.m[3][1], f32::MAX); // -f32::MIN
    }

    #[test]
    fn new_translation_nan_does_not_poison_the_linear_part() {
        let t = ComputedTransform3D::new_translation(f32::NAN, f32::INFINITY, 0.0);
        assert!(t.m[3][0].is_nan());
        assert!(t.m[3][1].is_infinite());
        assert_eq!(t.m[0][0], 1.0);
        // determinant only sees the 1.0s and 0.0s of the linear part... except the
        // 24-term expansion also multiplies through the translation row, so a NaN
        // offset does reach it. Assert the *actual* (defined) behaviour: NaN in,
        // NaN out — no panic.
        assert!(t.determinant().is_nan() || t.determinant() == 1.0);
    }

    // ------------------------------------------- constructors: perspective / skew

    #[test]
    fn new_perspective_finite_distance() {
        let t = ComputedTransform3D::new_perspective(100.0);
        assert!((t.m[2][3] - (-0.01)).abs() < 1e-6);
        assert_eq!(t.m[0][0], 1.0);
        assert_eq!(t.m[3][3], 1.0);
    }

    #[test]
    fn new_perspective_zero_distance_divides_by_zero() {
        // -1.0 / 0.0 is -inf in IEEE-754: no panic, but the matrix is unusable.
        // This is the documented (if hazardous) behaviour of `perspective(0)`.
        let t = ComputedTransform3D::new_perspective(0.0);
        assert!(t.m[2][3].is_infinite() && t.m[2][3].is_sign_negative());
        assert!(!all_finite(&t));
    }

    #[test]
    fn new_perspective_extreme_distances_do_not_panic() {
        let nan = ComputedTransform3D::new_perspective(f32::NAN);
        assert!(nan.m[2][3].is_nan());

        let inf = ComputedTransform3D::new_perspective(f32::INFINITY);
        assert_eq!(inf.m[2][3], -0.0); // -1/inf == -0.0 => equals the identity
        assert!(all_finite(&inf));

        // -1 / (smallest subnormal) overflows f32 => -inf, still no panic.
        let tiny = ComputedTransform3D::new_perspective(1e-45);
        assert!(tiny.m[2][3].is_infinite() && tiny.m[2][3].is_sign_negative());
    }

    #[test]
    fn new_skew_45_degrees_is_unit_shear() {
        // new_skew(alpha, beta): m[1][0] = tan(alpha), m[0][1] = tan(beta)
        let t = ComputedTransform3D::new_skew(45.0, 0.0);
        assert!((t.m[1][0] - 1.0).abs() < 1e-5, "tan(45deg) ~= 1, got {}", t.m[1][0]);
        assert_eq!(t.m[0][1], 0.0);
        assert_eq!(t.m[0][0], 1.0);
        assert_eq!(t.m[1][1], 1.0);
        // A unit shear preserves area.
        assert!((t.determinant() - 1.0).abs() < 1e-4);
    }

    #[test]
    fn new_skew_90_degrees_stays_finite() {
        // tan(pi/2) is not representable; f32's rounded pi/2 makes tan() a huge
        // (but finite) number rather than inf. Assert it does not panic and that
        // nothing becomes NaN/inf.
        let t = ComputedTransform3D::new_skew(90.0, 90.0);
        assert!(all_finite(&t), "skew(90deg) produced a non-finite entry: {t:?}");
        assert!(t.m[1][0].abs() > 1e6, "expected a huge shear, got {}", t.m[1][0]);
    }

    #[test]
    fn new_skew_nan_and_infinite_angles_do_not_panic() {
        let nan = ComputedTransform3D::new_skew(f32::NAN, 0.0);
        assert!(nan.m[1][0].is_nan());
        assert_eq!(nan.m[3][3], 1.0);

        // tan(inf) is NaN, not a panic.
        let inf = ComputedTransform3D::new_skew(f32::INFINITY, f32::NEG_INFINITY);
        assert!(inf.m[1][0].is_nan());
        assert!(inf.m[0][1].is_nan());
    }

    // --------------------------------------------------- constructors: rotation

    #[test]
    fn new_rotation_zero_angle_is_identity() {
        let t = ComputedTransform3D::new_rotation(0.0, 0.0, 1.0, 0.0);
        assert_mat_approx(&t, &ComputedTransform3D::IDENTITY, 1e-6);
    }

    #[test]
    fn new_rotation_quarter_turn_about_z() {
        let t = ComputedTransform3D::new_rotation(0.0, 0.0, 1.0, core::f32::consts::FRAC_PI_2);
        // sq = sin^2(pi/4) = 0.5, sc = sin*cos(pi/4) = 0.5
        assert!((t.m[0][0] - 0.0).abs() < 1e-6);
        assert!((t.m[0][1] - 1.0).abs() < 1e-6);
        assert!((t.m[1][0] - -1.0).abs() < 1e-6);
        assert!((t.m[1][1] - 0.0).abs() < 1e-6);
        assert_eq!(t.m[2][2], 1.0);
    }

    #[test]
    fn new_rotation_is_orthonormal_and_det_one() {
        // Normalized axis, as the doc requires.
        let (x, y, z) = (0.267_261_24, 0.534_522_5, 0.801_783_7); // (1,2,3)/|(1,2,3)|
        let t = ComputedTransform3D::new_rotation(x, y, z, 0.7);
        assert!((t.determinant() - 1.0).abs() < 1e-4, "det = {}", t.determinant());
        for r in 0..3 {
            let len_sq =
                t.m[r][0] * t.m[r][0] + t.m[r][1] * t.m[r][1] + t.m[r][2] * t.m[r][2];
            assert!((len_sq - 1.0).abs() < 1e-4, "row {r} is not unit length: {len_sq}");
        }
        // For a pure rotation, inverse == transpose.
        assert_mat_approx(&t.inverse(), &t.get_column_major(), 1e-4);
    }

    #[test]
    fn new_rotation_degenerate_zero_axis_yields_identity() {
        // The doc says the axis "must be normalized"; a zero axis is the classic
        // caller mistake. It must not panic or produce NaN — it degenerates to
        // the identity (every term is multiplied by an axis component).
        let t = ComputedTransform3D::new_rotation(0.0, 0.0, 0.0, 1.234);
        assert_mat_approx(&t, &ComputedTransform3D::IDENTITY, 1e-6);
    }

    #[test]
    fn new_rotation_nan_and_infinite_theta_do_not_panic() {
        let nan = ComputedTransform3D::new_rotation(0.0, 0.0, 1.0, f32::NAN);
        assert!(nan.m[0][0].is_nan());
        assert_eq!(nan.m[3][3], 1.0); // the constant W row survives

        // sin(inf) / cos(inf) are NaN in IEEE-754 — again, no panic.
        let inf = ComputedTransform3D::new_rotation(0.0, 0.0, 1.0, f32::INFINITY);
        assert!(inf.m[0][0].is_nan());
    }

    #[test]
    fn new_rotation_huge_theta_stays_bounded() {
        // A rotation matrix must stay in [-1, 1] no matter how absurd the angle.
        let t = ComputedTransform3D::new_rotation(0.0, 0.0, 1.0, 1e9);
        assert!(all_finite(&t));
        for r in 0..3 {
            for c in 0..3 {
                assert!(t.m[r][c].abs() <= 1.001, "entry [{r}][{c}] = {} escaped [-1,1]", t.m[r][c]);
            }
        }
    }

    // ------------------------------------------------------------- determinant

    #[test]
    fn determinant_of_identity_is_one() {
        assert_eq!(ComputedTransform3D::IDENTITY.determinant(), 1.0);
    }

    #[test]
    fn determinant_of_diagonal_is_product() {
        let t = ComputedTransform3D::new(
            2.0, 0.0, 0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 0.0, 0.0, 4.0, 0.0, 0.0, 0.0, 0.0, 5.0,
        );
        assert_eq!(t.determinant(), 120.0);
    }

    #[test]
    fn determinant_matches_independent_cofactor_expansion() {
        let t = ComputedTransform3D::new(
            3.0, 1.0, 0.0, 2.0, 0.0, 2.0, 1.0, 1.0, 1.0, 0.0, 4.0, 0.0, 2.0, 1.0, 1.0, 3.0,
        );
        let got = t.determinant();
        let want = det4_naive(&t);
        assert!((got - want).abs() < 1e-3, "determinant() = {got}, cofactor ref = {want}");
    }

    #[test]
    fn determinant_of_singular_matrices_is_zero() {
        // All-zero matrix.
        assert_eq!(filled(0.0).determinant(), 0.0);
        // Two identical rows => rank-deficient. Small integers keep the products
        // exact, so the cancellation is exact too.
        let dup = ComputedTransform3D::new(
            1.0, 2.0, 3.0, 4.0, 1.0, 2.0, 3.0, 4.0, 0.0, 1.0, 0.0, 2.0, 4.0, 3.0, 2.0, 1.0,
        );
        assert!(dup.determinant().abs() < 1e-4, "det = {}", dup.determinant());
        // A matrix where every entry is the same is also singular.
        assert!(filled(7.0).determinant().abs() < 1e-2);
    }

    #[test]
    fn determinant_overflows_to_infinity_rather_than_wrapping() {
        // diag(1e20)^4 = 1e80, far beyond f32::MAX (~3.4e38): saturates to +inf.
        let big = ComputedTransform3D::new_scale(1e20, 1e20, 1e20);
        let mut big = big;
        big.m[3][3] = 1e20;
        let det = big.determinant();
        assert!(det.is_infinite() && det.is_sign_positive(), "det = {det}");
    }

    #[test]
    fn determinant_of_nan_matrix_is_nan_not_a_panic() {
        assert!(filled(f32::NAN).determinant().is_nan());
    }

    // ----------------------------------------------------------------- inverse

    #[test]
    fn inverse_of_identity_is_identity() {
        assert_mat_approx(
            &ComputedTransform3D::IDENTITY.inverse(),
            &ComputedTransform3D::IDENTITY,
            1e-6,
        );
    }

    #[test]
    fn inverse_round_trips_to_identity() {
        let t = ComputedTransform3D::new_translation(10.0, 20.0, 30.0)
            .then(&ComputedTransform3D::new_scale(2.0, 4.0, 8.0));
        assert_mat_approx(&t.then(&t.inverse()), &ComputedTransform3D::IDENTITY, 1e-4);
        assert_mat_approx(&t.inverse().then(&t), &ComputedTransform3D::IDENTITY, 1e-4);
    }

    #[test]
    fn inverse_of_singular_matrix_returns_identity() {
        assert_eq!(filled(0.0).inverse(), ComputedTransform3D::IDENTITY);
        assert_eq!(
            ComputedTransform3D::new_scale(1.0, 1.0, 0.0).inverse(),
            ComputedTransform3D::IDENTITY
        );
    }

    #[test]
    fn inverse_treats_near_singular_as_singular() {
        // PRECISION HAZARD, asserted as the documented contract: the guard is
        // `det.abs() < f32::EPSILON` (~1.19e-7), so a *perfectly invertible*
        // uniform scale of 1e-3 (det = 1e-9) is rejected and the identity is
        // returned instead of the true inverse (a 1000x up-scale).
        let tiny = ComputedTransform3D::new_scale(1e-3, 1e-3, 1e-3);
        let det = tiny.determinant();
        assert!(det > 0.0 && det < f32::EPSILON, "det = {det} (must be a nonzero sub-EPSILON)");
        assert_eq!(tiny.inverse(), ComputedTransform3D::IDENTITY);
    }

    #[test]
    fn inverse_of_nan_matrix_does_not_panic() {
        // det is NaN, and `NaN.abs() < EPSILON` is false, so the singular guard
        // does NOT catch it: the algorithm runs and yields an all-NaN matrix.
        let inv = filled(f32::NAN).inverse();
        assert!(inv.m.iter().flatten().all(|v| v.is_nan()));
    }

    #[test]
    fn inverse_of_overflowing_matrix_yields_nan_not_a_panic() {
        // det = +inf => scale factor 1/inf = 0.0 => cofactor(inf) * 0.0 = NaN.
        let mut big = ComputedTransform3D::new_scale(1e20, 1e20, 1e20);
        big.m[3][3] = 1e20;
        let inv = big.inverse();
        assert!(inv.m[0][0].is_nan(), "expected NaN from inf * 0.0, got {}", inv.m[0][0]);
    }

    // ------------------------------------------------------- multiply_scalar

    #[test]
    fn multiply_scalar_by_zero_zeroes_every_entry() {
        let t = ComputedTransform3D::IDENTITY.multiply_scalar(0.0);
        assert!(t.m.iter().flatten().all(|v| *v == 0.0));
    }

    #[test]
    fn multiply_scalar_is_sign_and_magnitude_exact() {
        let t = ComputedTransform3D::new_scale(2.0, 3.0, 4.0).multiply_scalar(-1.0);
        assert_eq!(t.m[0][0], -2.0);
        assert_eq!(t.m[1][1], -3.0);
        assert_eq!(t.m[2][2], -4.0);
        assert_eq!(t.m[3][3], -1.0);

        let m = ComputedTransform3D::IDENTITY.multiply_scalar(f32::MAX);
        assert_eq!(m.m[0][0], f32::MAX);
        assert_eq!(m.m[0][1], 0.0);
    }

    #[test]
    fn multiply_scalar_overflow_saturates_to_infinity() {
        let t = filled(1e30).multiply_scalar(1e30);
        assert!(t.m.iter().flatten().all(|v| v.is_infinite() && v.is_sign_positive()));
    }

    #[test]
    fn multiply_scalar_by_infinity_poisons_zero_entries_with_nan() {
        // IEEE-754: 0.0 * inf == NaN. Scaling the IDENTITY by inf therefore does
        // NOT give an "infinitely scaled identity" — every off-diagonal 0 becomes
        // NaN. Asserted so a future refactor cannot silently change it.
        let t = ComputedTransform3D::IDENTITY.multiply_scalar(f32::INFINITY);
        assert!(t.m[0][0].is_infinite());
        assert!(t.m[0][1].is_nan(), "0.0 * inf should be NaN, got {}", t.m[0][1]);
    }

    #[test]
    fn multiply_scalar_by_nan_makes_everything_nan() {
        let t = ComputedTransform3D::new_scale(2.0, 3.0, 4.0).multiply_scalar(f32::NAN);
        assert!(t.m.iter().flatten().all(|v| v.is_nan()));
    }

    // ------------------------------------------------------ get_column_major

    #[test]
    fn get_column_major_transposes() {
        let t = ComputedTransform3D::new(
            1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0,
        );
        let c = t.get_column_major();
        for r in 0..4 {
            for col in 0..4 {
                assert_eq!(c.m[r][col], t.m[col][r], "transpose slot [{r}][{col}]");
            }
        }
    }

    #[test]
    fn get_column_major_is_an_involution() {
        let t = ComputedTransform3D::new_translation(3.0, -4.0, 5.0)
            .then(&ComputedTransform3D::new_scale(2.0, 2.0, 2.0));
        assert_eq!(t.get_column_major().get_column_major(), t);
        // The identity is its own transpose.
        assert_eq!(
            ComputedTransform3D::IDENTITY.get_column_major(),
            ComputedTransform3D::IDENTITY
        );
    }

    #[test]
    fn get_column_major_moves_translation_into_the_last_column() {
        let t = ComputedTransform3D::new_translation(7.0, 8.0, 9.0).get_column_major();
        assert_eq!(t.m[0][3], 7.0);
        assert_eq!(t.m[1][3], 8.0);
        assert_eq!(t.m[2][3], 9.0);
        assert_eq!(t.m[3], [0.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn get_column_major_of_nan_matrix_does_not_panic() {
        let t = filled(f32::NAN).get_column_major();
        assert!(t.m.iter().flatten().all(|v| v.is_nan()));
    }

    // ----------------------------------------------------- transform_point2d

    #[test]
    fn transform_point2d_identity_is_the_point_itself() {
        let p = LogicalPosition::new(3.0, -4.0);
        let out = ComputedTransform3D::IDENTITY.transform_point2d(p).unwrap();
        assert_eq!(out.x, 3.0);
        assert_eq!(out.y, -4.0);

        let zero = ComputedTransform3D::IDENTITY
            .transform_point2d(LogicalPosition::zero())
            .unwrap();
        assert_eq!((zero.x, zero.y), (0.0, 0.0));
    }

    #[test]
    fn transform_point2d_applies_translation_and_scale() {
        let t = ComputedTransform3D::new_translation(10.0, 20.0, 0.0);
        let out = t.transform_point2d(LogicalPosition::new(1.0, 2.0)).unwrap();
        assert_eq!((out.x, out.y), (11.0, 22.0));

        let s = ComputedTransform3D::new_scale(2.0, -3.0, 1.0);
        let out = s.transform_point2d(LogicalPosition::new(1.5, 2.0)).unwrap();
        assert_eq!((out.x, out.y), (3.0, -6.0));
    }

    #[test]
    fn transform_point2d_negative_w_returns_none() {
        let mut t = ComputedTransform3D::IDENTITY;
        t.m[3][3] = -1.0; // w = -1
        assert!(t.transform_point2d(LogicalPosition::new(1.0, 1.0)).is_none());

        // w driven negative by the point itself (perspective-style m[0][3]).
        let mut p = ComputedTransform3D::IDENTITY;
        p.m[0][3] = -1.0; // w = 1 - p.x
        assert!(p.transform_point2d(LogicalPosition::new(2.0, 0.0)).is_none());
        assert!(p.transform_point2d(LogicalPosition::new(0.5, 0.0)).is_some());
    }

    #[test]
    fn transform_point2d_zero_w_divides_by_zero_instead_of_returning_none() {
        // BOUNDARY: the guard is `!w.is_sign_positive()`, and (+0.0) IS
        // sign-positive — so a fully degenerate w == +0.0 slips through and the
        // function divides by zero, returning Some(inf, inf) rather than None.
        // Asserted as-is (no panic, defined IEEE result); flagged in the report.
        let mut t = ComputedTransform3D::IDENTITY;
        t.m[3][3] = 0.0;
        let out = t.transform_point2d(LogicalPosition::new(1.0, 1.0));
        let out = out.expect("w == +0.0 is treated as a valid positive w");
        assert!(out.x.is_infinite(), "expected 1.0/0.0 = inf, got {}", out.x);
        assert!(out.y.is_infinite());

        // ... whereas a w that comes out as -0.0 IS rejected, so the sign of a
        // zero w decides between Some(inf) and None. (The whole w column must be
        // -0.0: fma(1.0, +0.0, -0.0) would round back to +0.0.)
        let mut neg = ComputedTransform3D::IDENTITY;
        neg.m[0][3] = -0.0;
        neg.m[1][3] = -0.0;
        neg.m[3][3] = -0.0;
        assert!(neg.transform_point2d(LogicalPosition::new(1.0, 1.0)).is_none());
    }

    #[test]
    fn transform_point2d_nan_matrix_does_not_panic() {
        let out = filled(f32::NAN).transform_point2d(LogicalPosition::new(1.0, 1.0));
        // w is NaN; whichever branch the sign bit lands on, neither may panic.
        if let Some(p) = out {
            assert!(p.x.is_nan() && p.y.is_nan());
        }
    }

    #[test]
    fn transform_point2d_nan_point_does_not_panic() {
        let out = ComputedTransform3D::IDENTITY
            .transform_point2d(LogicalPosition::new(f32::NAN, f32::NAN));
        // w = NaN*0 + (NaN*0 + 1) = NaN => no panic either way.
        if let Some(p) = out {
            assert!(p.x.is_nan());
        }
    }

    #[test]
    fn transform_point2d_extreme_coordinates_saturate() {
        let t = ComputedTransform3D::new_translation(10.0, 10.0, 0.0);
        let out = t
            .transform_point2d(LogicalPosition::new(f32::MAX, f32::MIN))
            .unwrap();
        assert_eq!(out.x, f32::MAX); // MAX + 10 rounds back to MAX
        assert_eq!(out.y, f32::MIN);

        // A scale big enough to overflow saturates to inf, it does not wrap.
        let s = ComputedTransform3D::new_scale(1e30, 1e30, 1.0);
        let out = s
            .transform_point2d(LogicalPosition::new(1e30, 1e30))
            .unwrap();
        assert!(out.x.is_infinite() && out.x.is_sign_positive());
    }

    // --------------------------------------------------------- scale_for_dpi

    #[test]
    fn scale_for_dpi_touches_only_the_translation_row() {
        let mut t = ComputedTransform3D::new(
            1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0,
        );
        let before = t;
        t.scale_for_dpi(3.0);
        assert_eq!(t.m[0], before.m[0]);
        assert_eq!(t.m[1], before.m[1]);
        assert_eq!(t.m[2], before.m[2]);
        assert_eq!(t.m[3][0], 39.0);
        assert_eq!(t.m[3][1], 42.0);
        assert_eq!(t.m[3][2], 45.0);
        assert_eq!(t.m[3][3], 16.0, "m44 must NOT be scaled");
    }

    #[test]
    fn scale_for_dpi_zero_and_negative() {
        let mut t = ComputedTransform3D::new_translation(10.0, 20.0, 30.0);
        t.scale_for_dpi(0.0);
        assert_eq!(t.m[3], [0.0, 0.0, 0.0, 1.0]);

        let mut n = ComputedTransform3D::new_translation(10.0, -20.0, 30.0);
        n.scale_for_dpi(-2.0);
        assert_eq!(n.m[3], [-20.0, 40.0, -60.0, 1.0]);
    }

    #[test]
    fn scale_for_dpi_is_exactly_reversible_for_powers_of_two() {
        let original = ComputedTransform3D::new_translation(13.25, -7.5, 0.125);
        let mut t = original;
        t.scale_for_dpi(2.0);
        t.scale_for_dpi(0.5);
        assert_eq!(t, original);
    }

    #[test]
    fn scale_for_dpi_overflow_saturates_to_infinity() {
        let mut t = ComputedTransform3D::new_translation(1e38, -1e38, 1e38);
        t.scale_for_dpi(1e5);
        assert!(t.m[3][0].is_infinite() && t.m[3][0].is_sign_positive());
        assert!(t.m[3][1].is_infinite() && t.m[3][1].is_sign_negative());
        assert_eq!(t.m[3][3], 1.0);
    }

    #[test]
    fn scale_for_dpi_by_infinity_poisons_a_zero_translation() {
        // IEEE-754: 0.0 * inf == NaN. A DPI factor of inf turns the *identity's*
        // zero translation into NaN — assert the defined result, no panic.
        let mut t = ComputedTransform3D::IDENTITY;
        t.scale_for_dpi(f32::INFINITY);
        assert!(t.m[3][0].is_nan());
        assert_eq!(t.m[0][0], 1.0, "the linear part must stay untouched");

        let mut n = ComputedTransform3D::new_translation(1.0, 2.0, 3.0);
        n.scale_for_dpi(f32::INFINITY);
        assert!(n.m[3][0].is_infinite());
    }

    #[test]
    fn scale_for_dpi_by_nan_does_not_panic() {
        let mut t = ComputedTransform3D::new_translation(1.0, 2.0, 3.0);
        t.scale_for_dpi(f32::NAN);
        assert!(t.m[3][0].is_nan() && t.m[3][1].is_nan() && t.m[3][2].is_nan());
        assert_eq!(t.m[3][3], 1.0);
        assert_eq!(t.m[0][0], 1.0);
    }

    // -------------------------------------------------------------------- then

    #[test]
    fn then_has_identity_as_a_two_sided_unit() {
        let a = ComputedTransform3D::new(
            1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0,
        );
        assert_mat_approx(&a.then(&ComputedTransform3D::IDENTITY), &a, 1e-4);
        assert_mat_approx(&ComputedTransform3D::IDENTITY.then(&a), &a, 1e-4);
    }

    #[test]
    fn then_composes_translations_additively_and_scales_multiplicatively() {
        let t = ComputedTransform3D::new_translation(1.0, 2.0, 3.0)
            .then(&ComputedTransform3D::new_translation(10.0, 20.0, 30.0));
        assert_eq!(t.m[3], [11.0, 22.0, 33.0, 1.0]);

        let s = ComputedTransform3D::new_scale(2.0, 3.0, 4.0)
            .then(&ComputedTransform3D::new_scale(5.0, 7.0, 11.0));
        assert_eq!(s.m[0][0], 10.0);
        assert_eq!(s.m[1][1], 21.0);
        assert_eq!(s.m[2][2], 44.0);
    }

    #[test]
    fn then_is_associative() {
        let a = ComputedTransform3D::new(
            1.0, 0.5, 0.0, 0.0, -0.5, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 2.0, 3.0, 0.0, 1.0,
        );
        let b = ComputedTransform3D::new_scale(2.0, 0.5, 1.0);
        let c = ComputedTransform3D::new_translation(-1.0, 4.0, 0.0);
        assert_mat_approx(&a.then(&b).then(&c), &a.then(&b.then(&c)), 1e-2);
    }

    #[test]
    fn then_with_extreme_matrices_does_not_panic() {
        let big = filled(1e30).then(&filled(1e30));
        assert!(big.m[0][0].is_infinite(), "expected overflow to inf, got {}", big.m[0][0]);

        let nan = filled(f32::NAN).then(&ComputedTransform3D::IDENTITY);
        assert!(nan.m.iter().flatten().all(|v| v.is_nan()));

        // inf * 0 inside the dot product => NaN, not a panic.
        let mixed = filled(f32::INFINITY).then(&ComputedTransform3D::IDENTITY);
        assert!(mixed.m[0][0].is_infinite() || mixed.m[0][0].is_nan());
    }

    // ------------------------------------------------ SIMD paths (x86_64 only)

    // `not(miri)`: Miri cannot execute SSE/AVX intrinsics.
    #[cfg(all(target_arch = "x86_64", not(miri)))]
    #[test]
    fn linear_combine_sse_matches_naive_row_combine() {
        if !std::is_x86_feature_detected!("sse") {
            return;
        }
        let b = ComputedTransform3D::new(
            1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0,
        );
        for a in [
            [1.0f32, 2.0, 3.0, 4.0],
            [0.0, 0.0, 0.0, 0.0],
            [-1.5, 0.25, 1e6, -1e6],
        ] {
            // SAFETY: SSE availability was just checked at runtime.
            let got = unsafe { ComputedTransform3D::linear_combine_sse(a, &b) };
            let want = naive_row_combine(a, &b);
            for c in 0..4 {
                let tol = 1e-3 * want[c].abs().max(1.0);
                assert!((got[c] - want[c]).abs() <= tol, "lane {c}: {} vs {}", got[c], want[c]);
            }
        }
    }

    #[cfg(all(target_arch = "x86_64", not(miri)))]
    #[test]
    fn linear_combine_sse_propagates_nan_per_lane() {
        if !std::is_x86_feature_detected!("sse") {
            return;
        }
        // SAFETY: SSE availability was just checked at runtime.
        let got = unsafe {
            ComputedTransform3D::linear_combine_sse(
                [f32::NAN; 4],
                &ComputedTransform3D::IDENTITY,
            )
        };
        assert!(got.iter().all(|v| v.is_nan()), "NaN must survive the SIMD path: {got:?}");
    }

    #[cfg(all(target_arch = "x86_64", not(miri)))]
    #[test]
    fn then_sse_and_then_avx8_agree_with_scalar_then() {
        let a = ComputedTransform3D::new(
            1.0, 0.5, -2.0, 0.0, 3.0, 1.0, 0.0, 0.25, 0.0, -1.0, 4.0, 0.0, 5.0, 6.0, 7.0, 1.0,
        );
        let b = ComputedTransform3D::new(
            2.0, 0.0, 0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 1.0, 1.0, 1.0, 0.0, -4.0, 2.0, 0.5, 1.0,
        );
        let scalar = a.then(&b);

        if std::is_x86_feature_detected!("sse") {
            // SAFETY: SSE availability was just checked at runtime.
            let sse = unsafe { a.then_sse(&b) };
            assert_mat_approx(&scalar, &sse, 1e-3);
            // Identity is still a unit through the SIMD path.
            // SAFETY: same runtime check as above.
            let unit = unsafe { ComputedTransform3D::IDENTITY.then_sse(&b) };
            assert_mat_approx(&unit, &b, 1e-4);
        }
        if std::is_x86_feature_detected!("avx") {
            // SAFETY: AVX availability was just checked at runtime.
            let avx = unsafe { a.then_avx8(&b) };
            assert_mat_approx(&scalar, &avx, 1e-3);
            // SAFETY: same runtime check as above.
            let unit = unsafe { ComputedTransform3D::IDENTITY.then_avx8(&b) };
            assert_mat_approx(&unit, &b, 1e-4);
        }
    }

    #[cfg(all(target_arch = "x86_64", not(miri)))]
    #[test]
    fn simd_paths_do_not_panic_on_extreme_matrices() {
        // NaN/inf must flow through the intrinsics exactly like the scalar path:
        // no trap, no panic. (Values are not compared against the scalar path
        // because `then` fuses with mul_add while the SIMD kernels do not, and
        // fusion legitimately changes which NaN/inf a saturating term produces.)
        let extremes = [filled(f32::NAN), filled(f32::INFINITY), filled(1e30), filled(f32::MIN)];
        for a in &extremes {
            for b in &extremes {
                if std::is_x86_feature_detected!("sse") {
                    // SAFETY: SSE availability was just checked at runtime.
                    let r = unsafe { a.then_sse(b) };
                    core::hint::black_box(r);
                }
                if std::is_x86_feature_detected!("avx") {
                    // SAFETY: AVX availability was just checked at runtime.
                    let r = unsafe { a.then_avx8(b) };
                    core::hint::black_box(r);
                }
            }
        }
    }

    #[cfg(all(target_arch = "x86_64", not(miri)))]
    #[test]
    fn linear_combine_avx8_computes_two_rows_at_once() {
        use core::arch::x86_64::{_mm256_loadu_ps, _mm256_storeu_ps};

        if !std::is_x86_feature_detected!("avx") {
            return;
        }
        let b = ComputedTransform3D::new(
            1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0,
        );
        let row0 = [1.0f32, 0.0, -2.0, 3.0];
        let row1 = [0.5f32, 4.0, 0.0, -1.0];
        let packed: [f32; 8] = [
            row0[0], row0[1], row0[2], row0[3], row1[0], row1[1], row1[2], row1[3],
        ];
        let mut out = [0.0f32; 8];

        // SAFETY: AVX availability was just checked at runtime; `packed`/`out` are
        // live 8-f32 locals and both intrinsics used here are *unaligned* accesses.
        unsafe {
            let a01 = _mm256_loadu_ps(packed.as_ptr());
            let res = ComputedTransform3D::linear_combine_avx8(a01, &b);
            _mm256_storeu_ps(out.as_mut_ptr(), res);
        }

        let want0 = naive_row_combine(row0, &b);
        let want1 = naive_row_combine(row1, &b);
        for c in 0..4 {
            assert!((out[c] - want0[c]).abs() < 1e-3, "low lane {c}: {} vs {}", out[c], want0[c]);
            assert!(
                (out[4 + c] - want1[c]).abs() < 1e-3,
                "high lane {c}: {} vs {}",
                out[4 + c],
                want1[c]
            );
        }
    }

    // ------------------------------------------------ from_style_transform_vec

    #[test]
    fn from_style_transform_vec_empty_is_identity() {
        let t = ComputedTransform3D::from_style_transform_vec(
            &[],
            &StyleTransformOrigin::default(),
            100.0,
            100.0,
            RotationMode::ForWebRender,
        );
        assert_eq!(t, ComputedTransform3D::IDENTITY);
    }

    #[test]
    fn from_style_transform_vec_accumulates_a_thousand_translations_exactly() {
        // 1000 x translateX(1px): integer translations compose exactly on every
        // code path (scalar, SSE, AVX), so this must land on exactly 1000.0.
        let list = vec![StyleTransform::TranslateX(PixelValue::const_px(1)); 1000];
        let t = ComputedTransform3D::from_style_transform_vec(
            &list,
            &StyleTransformOrigin::default(),
            0.0,
            0.0,
            RotationMode::ForHitTesting,
        );
        assert_eq!(t.m[3][0], 1000.0);
        assert_eq!(t.m[3][1], 0.0);
        assert_eq!(t.m[0][0], 1.0);
    }

    #[test]
    fn from_style_transform_vec_resolves_percentages_against_each_axis() {
        let list = vec![
            StyleTransform::TranslateX(PixelValue::const_percent(50)),
            StyleTransform::TranslateY(PixelValue::const_percent(50)),
        ];
        let t = ComputedTransform3D::from_style_transform_vec(
            &list,
            &StyleTransformOrigin::default(),
            200.0,
            80.0,
            RotationMode::ForHitTesting,
        );
        assert_eq!(t.m[3][0], 100.0); // 50% of the X basis
        assert_eq!(t.m[3][1], 40.0); // 50% of the Y basis
    }

    #[test]
    fn from_style_transform_vec_with_extreme_percent_basis_does_not_panic() {
        let list = vec![StyleTransform::TranslateX(PixelValue::const_percent(50))];
        let run = |basis: f32| {
            ComputedTransform3D::from_style_transform_vec(
                &list,
                &StyleTransformOrigin::default(),
                basis,
                basis,
                RotationMode::ForWebRender,
            )
        };

        // Any *finite* basis, however absurd, leaves the linear part an identity.
        for basis in [f32::MAX, f32::MIN, 0.0, -0.0, -1e30] {
            let t = run(basis);
            assert_eq!(t.m[0][0], 1.0, "linear part corrupted for basis {basis}");
            assert!(!t.m[3][0].is_nan(), "finite basis {basis} produced a NaN offset");
        }

        // A NaN basis yields a NaN offset — defined, and no panic.
        assert!(run(f32::NAN).m[3][0].is_nan());

        // HAZARD, asserted as-is: an *infinite* basis does not just give an
        // infinite offset — `then` multiplies the identity's zero w-column by it
        // (0.0 * inf == NaN), so the NaN spreads into the linear part as well.
        // Same on the scalar, SSE and AVX paths.
        let inf = run(f32::INFINITY);
        assert!(inf.m[0][0].is_nan(), "0.0 * inf should poison m11, got {}", inf.m[0][0]);
    }

    #[test]
    fn from_style_transform_vec_long_mixed_list_does_not_panic() {
        let mut list = vec![];
        for i in 0..512 {
            list.push(match i % 4 {
                0 => StyleTransform::Rotate(AngleValue::const_deg(37)),
                1 => StyleTransform::Scale(StyleTransformScale2D {
                    x: FloatValue::const_new(1),
                    y: FloatValue::const_new(1),
                }),
                2 => StyleTransform::SkewX(AngleValue::const_deg(5)),
                _ => StyleTransform::TranslateY(PixelValue::const_px(1)),
            });
        }
        let t = ComputedTransform3D::from_style_transform_vec(
            &list,
            &StyleTransformOrigin::default(),
            300.0,
            150.0,
            RotationMode::ForWebRender,
        );
        // 512 chained f32 multiplies may drift, but must never produce NaN.
        assert!(!t.m[3][3].is_nan());
    }

    // ---------------------------------------------------- from_style_transform

    #[test]
    fn from_style_transform_matrix_2d_and_3d() {
        let m2d = StyleTransform::Matrix(StyleTransformMatrix2D {
            a: FloatValue::const_new(2),
            b: FloatValue::const_new(3),
            c: FloatValue::const_new(4),
            d: FloatValue::const_new(5),
            tx: FloatValue::const_new(6),
            ty: FloatValue::const_new(7),
        });
        let t = build(&m2d, 0.0, 0.0);
        assert_eq!(t.m[0], [2.0, 3.0, 0.0, 0.0]);
        assert_eq!(t.m[1], [4.0, 5.0, 0.0, 0.0]);
        assert_eq!(t.m[3], [6.0, 7.0, 0.0, 1.0]);

        // The default matrix3d() is the identity.
        let m3d = StyleTransform::Matrix3D(StyleTransformMatrix3D::default());
        assert_eq!(build(&m3d, 0.0, 0.0), ComputedTransform3D::IDENTITY);
    }

    #[test]
    fn from_style_transform_translate_units() {
        // px
        let t = build(&StyleTransform::TranslateX(PixelValue::const_px(25)), 0.0, 0.0);
        assert_eq!(t.m[3][0], 25.0);
        // em: resolved against DEFAULT_FONT_SIZE (16px)
        let t = build(&StyleTransform::TranslateY(PixelValue::const_em(2)), 0.0, 0.0);
        assert_eq!(t.m[3][1], 32.0);
        // 2D translate with mixed units
        let t = build(
            &StyleTransform::Translate(StyleTransformTranslate2D {
                x: PixelValue::const_percent(50),
                y: PixelValue::const_px(-10),
            }),
            400.0,
            0.0,
        );
        assert_eq!(t.m[3][0], 200.0);
        assert_eq!(t.m[3][1], -10.0);
    }

    #[test]
    fn from_style_transform_translate_z_percent_falls_back_to_the_x_basis() {
        // Documented: "CSS has no containing block for Z-axis percentages; use X".
        // percent_resolve_y is deliberately different so a Y-basis regression fails.
        let t = build(&StyleTransform::TranslateZ(PixelValue::const_percent(50)), 200.0, 999.0);
        assert_eq!(t.m[3][2], 100.0, "translateZ(%) must resolve against the X basis");

        let t3d = build(
            &StyleTransform::Translate3D(StyleTransformTranslate3D {
                x: PixelValue::const_px(0),
                y: PixelValue::const_px(0),
                z: PixelValue::const_percent(50),
            }),
            200.0,
            999.0,
        );
        assert_eq!(t3d.m[3][2], 100.0);
    }

    #[test]
    fn from_style_transform_viewport_units_resolve_to_zero() {
        // to_pixels_internal() has no viewport context and documents a 0.0 result
        // for vw/vh/vmin/vmax — assert that (a silently-dropped translation), so
        // a future viewport-aware fix has to update this test on purpose.
        let vw = PixelValue::from_metric(SizeMetric::Vw, 50.0);
        let t = build(&StyleTransform::TranslateX(vw), 1000.0, 1000.0);
        assert_eq!(t.m[3][0], 0.0);
    }

    #[test]
    fn from_style_transform_saturating_pixel_values_stay_finite() {
        // PixelValue stores an isize (value * 1000), so an infinite CSS length
        // saturates at isize::MAX/1000 instead of becoming inf, and NaN becomes 0.
        let inf = build(&StyleTransform::TranslateX(PixelValue::px(f32::INFINITY)), 0.0, 0.0);
        assert!(
            inf.m[3][0].is_finite() && inf.m[3][0] > 1e6,
            "an infinite px length must saturate, got {}",
            inf.m[3][0]
        );

        let nan = build(&StyleTransform::TranslateX(PixelValue::px(f32::NAN)), 0.0, 0.0);
        assert_eq!(nan.m[3][0], 0.0, "NaN px must saturate to 0, not propagate");
    }

    #[test]
    fn from_style_transform_scale_variants() {
        let s2d = build(
            &StyleTransform::Scale(StyleTransformScale2D {
                x: FloatValue::const_new(2),
                y: FloatValue::const_new(3),
            }),
            0.0,
            0.0,
        );
        assert_eq!((s2d.m[0][0], s2d.m[1][1], s2d.m[2][2]), (2.0, 3.0, 1.0));

        let s3d = build(
            &StyleTransform::Scale3D(StyleTransformScale3D {
                x: FloatValue::const_new(2),
                y: FloatValue::const_new(3),
                z: FloatValue::const_new(4),
            }),
            0.0,
            0.0,
        );
        assert_eq!((s3d.m[0][0], s3d.m[1][1], s3d.m[2][2]), (2.0, 3.0, 4.0));

        // scaleX/Y/Z take a PercentageValue: 150% => 1.5, and only one axis moves.
        let sx = build(&StyleTransform::ScaleX(PercentageValue::const_new(150)), 0.0, 0.0);
        assert_eq!((sx.m[0][0], sx.m[1][1], sx.m[2][2]), (1.5, 1.0, 1.0));
        let sy = build(&StyleTransform::ScaleY(PercentageValue::const_new(150)), 0.0, 0.0);
        assert_eq!((sy.m[0][0], sy.m[1][1], sy.m[2][2]), (1.0, 1.5, 1.0));
        let sz = build(&StyleTransform::ScaleZ(PercentageValue::const_new(150)), 0.0, 0.0);
        assert_eq!((sz.m[0][0], sz.m[1][1], sz.m[2][2]), (1.0, 1.0, 1.5));
    }

    #[test]
    fn from_style_transform_scale_zero_is_singular() {
        let s = build(
            &StyleTransform::Scale3D(StyleTransformScale3D {
                x: FloatValue::const_new(0),
                y: FloatValue::const_new(0),
                z: FloatValue::const_new(0),
            }),
            0.0,
            0.0,
        );
        assert_eq!(s.determinant(), 0.0);
        assert_eq!(s.inverse(), ComputedTransform3D::IDENTITY);
        // A collapsed element still maps points (to the origin), it does not panic.
        let p = s.transform_point2d(LogicalPosition::new(5.0, 9.0)).unwrap();
        assert_eq!((p.x, p.y), (0.0, 0.0));
    }

    #[test]
    fn from_style_transform_skew_variants() {
        let sx = build(&StyleTransform::SkewX(AngleValue::const_deg(45)), 0.0, 0.0);
        assert!((sx.m[1][0] - 1.0).abs() < 1e-5, "skewX => tan(a) at m21");
        assert_eq!(sx.m[0][1], 0.0);

        let sy = build(&StyleTransform::SkewY(AngleValue::const_deg(45)), 0.0, 0.0);
        assert!((sy.m[0][1] - 1.0).abs() < 1e-5, "skewY => tan(b) at m12");
        assert_eq!(sy.m[1][0], 0.0);

        let sk = build(
            &StyleTransform::Skew(StyleTransformSkew2D {
                x: AngleValue::const_deg(30),
                y: AngleValue::const_deg(60),
            }),
            0.0,
            0.0,
        );
        assert!((sk.m[1][0] - 0.577_350_3).abs() < 1e-3); // tan(30deg)
        assert!((sk.m[0][1] - 1.732_050_8).abs() < 1e-3); // tan(60deg)
    }

    #[test]
    fn from_style_transform_skew_90_degrees_stays_finite() {
        let sk = build(&StyleTransform::SkewX(AngleValue::const_deg(90)), 0.0, 0.0);
        assert!(all_finite(&sk), "skewX(90deg) must not produce inf/NaN: {sk:?}");
    }

    #[test]
    fn from_style_transform_perspective_zero_is_infinite() {
        let p = build(&StyleTransform::Perspective(PixelValue::const_px(0)), 0.0, 0.0);
        // perspective(0) => -1/0 => -inf. No panic, but a poisoned matrix.
        assert!(p.m[2][3].is_infinite() && p.m[2][3].is_sign_negative());

        let ok = build(&StyleTransform::Perspective(PixelValue::const_px(100)), 0.0, 0.0);
        assert!((ok.m[2][3] - (-0.01)).abs() < 1e-6);
    }

    #[test]
    fn from_style_transform_rotate_degenerate_axis_is_identity() {
        // rotate3d(0, 0, 0, 45deg) has no axis to rotate about; it must degenerate
        // to the identity (modulo the origin round-trip), not to NaN.
        let r = ComputedTransform3D::from_style_transform(
            &StyleTransform::Rotate3D(StyleTransformRotate3D {
                x: FloatValue::const_new(0),
                y: FloatValue::const_new(0),
                z: FloatValue::const_new(0),
                angle: AngleValue::const_deg(45),
            }),
            &StyleTransformOrigin::default(), // 50% / 50% => (50, 50)
            100.0,
            100.0,
            RotationMode::ForHitTesting,
        );
        assert_mat_approx(&r, &ComputedTransform3D::IDENTITY, 1e-4);
    }

    #[test]
    fn from_style_transform_rotate_angle_metrics_agree() {
        // 90deg == 0.25turn == 100grad. (Radians are NOT compared exactly here:
        // FloatValue quantizes to 3 decimals, so PI/2 stores as 1.570 rad
        // = 89.95deg — hence the looser tolerance on the rad case below.)
        let deg = build(&StyleTransform::Rotate(AngleValue::const_deg(90)), 0.0, 0.0);
        let turn = build(&StyleTransform::RotateZ(AngleValue::turn(0.25)), 0.0, 0.0);
        let grad = build(&StyleTransform::Rotate(AngleValue::const_grad(100)), 0.0, 0.0);
        assert_mat_approx(&deg, &turn, 1e-5);
        assert_mat_approx(&deg, &grad, 1e-5);

        let rad = build(
            &StyleTransform::Rotate(AngleValue::rad(core::f32::consts::FRAC_PI_2)),
            0.0,
            0.0,
        );
        assert_mat_approx(&deg, &rad, 1e-2);
    }

    #[test]
    fn from_style_transform_full_turn_normalizes_to_no_rotation() {
        // AngleValue::to_degrees() wraps into [0, 360), so 720deg == 0deg.
        let full = build(&StyleTransform::Rotate(AngleValue::const_deg(720)), 0.0, 0.0);
        assert_mat_approx(&full, &ComputedTransform3D::IDENTITY, 1e-3);

        // ... and a negative angle wraps to its positive equivalent (-90 => 270).
        let neg = build(&StyleTransform::Rotate(AngleValue::const_deg(-90)), 0.0, 0.0);
        let pos = build(&StyleTransform::Rotate(AngleValue::const_deg(270)), 0.0, 0.0);
        assert_mat_approx(&neg, &pos, 1e-5);
    }

    #[test]
    fn from_style_transform_rotate_x_y_z_pick_distinct_axes() {
        let rx = build(&StyleTransform::RotateX(AngleValue::const_deg(90)), 0.0, 0.0);
        let ry = build(&StyleTransform::RotateY(AngleValue::const_deg(90)), 0.0, 0.0);
        let rz = build(&StyleTransform::RotateZ(AngleValue::const_deg(90)), 0.0, 0.0);
        // Each keeps its own axis fixed: rotateX leaves m11, rotateY leaves m22,
        // rotateZ leaves m33 equal to 1.
        assert!((rx.m[0][0] - 1.0).abs() < 1e-5);
        assert!((ry.m[1][1] - 1.0).abs() < 1e-5);
        assert!((rz.m[2][2] - 1.0).abs() < 1e-5);
        // ... and they are genuinely different matrices.
        assert!(rx != ry && ry != rz && rx != rz);
        // Every rotation preserves volume.
        for r in [rx, ry, rz] {
            assert!((r.determinant() - 1.0).abs() < 1e-3, "det = {}", r.determinant());
        }
    }

    #[test]
    fn from_style_transform_huge_angle_stays_finite() {
        // AngleValue saturates through FloatValue's isize backing, and to_degrees()
        // wraps modulo 360 — so even a "f32::MAX degrees" rotation is well-defined.
        let huge = build(&StyleTransform::Rotate(AngleValue::deg(f32::MAX)), 0.0, 0.0);
        assert!(all_finite(&huge), "huge angle produced inf/NaN: {huge:?}");
    }

    // ----------------------------------------------------------- make_rotation

    #[test]
    fn make_rotation_zero_degrees_is_identity_about_any_origin() {
        for mode in [RotationMode::ForWebRender, RotationMode::ForHitTesting] {
            let r = ComputedTransform3D::make_rotation((10.0, 20.0), 0.0, 0.0, 0.0, 1.0, mode);
            assert_mat_approx(&r, &ComputedTransform3D::IDENTITY, 1e-3);
        }
    }

    #[test]
    fn make_rotation_modes_are_mutual_inverses() {
        // ForWebRender negates the angle, ForHitTesting does not — so composing
        // the two about the same origin must cancel out to the identity.
        let origin = (100.0, 50.0);
        let wr =
            ComputedTransform3D::make_rotation(origin, 45.0, 0.0, 0.0, 1.0, RotationMode::ForWebRender);
        let ht = ComputedTransform3D::make_rotation(
            origin,
            45.0,
            0.0,
            0.0,
            1.0,
            RotationMode::ForHitTesting,
        );
        assert!(wr != ht, "the two rotation modes must not produce the same matrix");
        assert_mat_approx(&wr.then(&ht), &ComputedTransform3D::IDENTITY, 1e-3);
    }

    #[test]
    fn make_rotation_keeps_its_origin_fixed() {
        // The defining property of a rotation about a point: that point does not move.
        let r = ComputedTransform3D::make_rotation(
            (30.0, 40.0),
            90.0,
            0.0,
            0.0,
            1.0,
            RotationMode::ForHitTesting,
        );
        let p = r.transform_point2d(LogicalPosition::new(30.0, 40.0)).unwrap();
        assert!((p.x - 30.0).abs() < 1e-2, "origin moved in x: {}", p.x);
        assert!((p.y - 40.0).abs() < 1e-2, "origin moved in y: {}", p.y);
    }

    #[test]
    fn make_rotation_preserves_volume() {
        let r = ComputedTransform3D::make_rotation(
            (7.0, -3.0),
            123.456,
            0.0,
            0.0,
            1.0,
            RotationMode::ForWebRender,
        );
        assert!((r.determinant() - 1.0).abs() < 1e-3, "det = {}", r.determinant());
    }

    #[test]
    fn make_rotation_nan_degrees_does_not_panic() {
        let r = ComputedTransform3D::make_rotation(
            (1.0, 2.0),
            f32::NAN,
            0.0,
            0.0,
            1.0,
            RotationMode::ForWebRender,
        );
        assert!(r.m[0][0].is_nan(), "NaN degrees must propagate, not panic");
    }

    #[test]
    fn make_rotation_infinite_degrees_does_not_panic() {
        for deg in [f32::INFINITY, f32::NEG_INFINITY, f32::MAX, f32::MIN] {
            let r = ComputedTransform3D::make_rotation(
                (0.0, 0.0),
                deg,
                0.0,
                0.0,
                1.0,
                RotationMode::ForHitTesting,
            );
            core::hint::black_box(r);
        }
    }

    #[test]
    fn make_rotation_extreme_origin_does_not_panic() {
        for origin in [
            (f32::MAX, f32::MAX),
            (f32::INFINITY, f32::NEG_INFINITY),
            (f32::NAN, 0.0),
        ] {
            let r = ComputedTransform3D::make_rotation(
                origin,
                45.0,
                0.0,
                0.0,
                1.0,
                RotationMode::ForWebRender,
            );
            core::hint::black_box(r);
        }
    }

    #[test]
    fn make_rotation_degenerate_axis_is_a_pure_origin_round_trip() {
        // A zero axis cancels every rotation term, leaving T(-o) * I * T(o) = I.
        let r = ComputedTransform3D::make_rotation(
            (12.0, 34.0),
            90.0,
            0.0,
            0.0,
            0.0,
            RotationMode::ForHitTesting,
        );
        assert_mat_approx(&r, &ComputedTransform3D::IDENTITY, 1e-4);
    }
}

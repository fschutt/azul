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
        self.m[0][3] * self.m[1][2] * self.m[2][1] * self.m[3][0]
            - self.m[0][2] * self.m[1][3] * self.m[2][1] * self.m[3][0]
            - self.m[0][3] * self.m[1][1] * self.m[2][2] * self.m[3][0]
            + self.m[0][1] * self.m[1][3] * self.m[2][2] * self.m[3][0]
            + self.m[0][2] * self.m[1][1] * self.m[2][3] * self.m[3][0]
            - self.m[0][1] * self.m[1][2] * self.m[2][3] * self.m[3][0]
            - self.m[0][3] * self.m[1][2] * self.m[2][0] * self.m[3][1]
            + self.m[0][2] * self.m[1][3] * self.m[2][0] * self.m[3][1]
            + self.m[0][3] * self.m[1][0] * self.m[2][2] * self.m[3][1]
            - self.m[0][0] * self.m[1][3] * self.m[2][2] * self.m[3][1]
            - self.m[0][2] * self.m[1][0] * self.m[2][3] * self.m[3][1]
            + self.m[0][0] * self.m[1][2] * self.m[2][3] * self.m[3][1]
            + self.m[0][3] * self.m[1][1] * self.m[2][0] * self.m[3][2]
            - self.m[0][1] * self.m[1][3] * self.m[2][0] * self.m[3][2]
            - self.m[0][3] * self.m[1][0] * self.m[2][1] * self.m[3][2]
            + self.m[0][0] * self.m[1][3] * self.m[2][1] * self.m[3][2]
            + self.m[0][1] * self.m[1][0] * self.m[2][3] * self.m[3][2]
            - self.m[0][0] * self.m[1][1] * self.m[2][3] * self.m[3][2]
            - self.m[0][2] * self.m[1][1] * self.m[2][0] * self.m[3][3]
            + self.m[0][1] * self.m[1][2] * self.m[2][0] * self.m[3][3]
            + self.m[0][2] * self.m[1][0] * self.m[2][1] * self.m[3][3]
            - self.m[0][0] * self.m[1][2] * self.m[2][1] * self.m[3][3]
            - self.m[0][1] * self.m[1][0] * self.m[2][2] * self.m[3][3]
            + self.m[0][0] * self.m[1][1] * self.m[2][2] * self.m[3][3]
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

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

#[cfg(not(feature = "std"))]
use crate::float_math::FloatMath as _;
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
    #[must_use]
    pub const fn new(
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

    /// The inverse of this matrix, or `None` when it has none.
    ///
    /// A matrix has no inverse when its determinant is zero (a `scale(0)`,
    /// a projection onto a line) or is not a number, or when the inverse
    /// does not fit an `f32`. That is the rule `WebRender` marks a spatial
    /// node "not invertible" by (`determinant != 0.0 && !determinant.is_nan()`),
    /// and what follows from it is the same everywhere: **a frame whose
    /// matrix has no inverse paints nothing and is hit by nothing** - there
    /// is no point of its content a pixel or a pointer could be mapped back
    /// to. A caller that maps a point or a rect BACK through a matrix asks
    /// this and treats `None` so; it must not go on with a stand-in matrix.
    ///
    /// The determinant and the adjugate are accumulated in `f64`: in `f32`
    /// the products of a `scale(1e20)` overflow before they cancel, and the
    /// determinant of a `scale(0.001)` (1e-9) is far below `f32::EPSILON`
    /// although the matrix inverts exactly (a zoomed-out view).
    #[must_use]
    #[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target
                                       // +fma; keep explicit a*b+c
    #[allow(clippy::cast_possible_truncation)] // the inverse computed in f64, narrowed to the f32
                                               // public type and checked to be finite
    pub fn try_inverse(&self) -> Option<Self> {
        let det = self.determinant_f64();
        if det == 0.0 || !det.is_finite() {
            return None;
        }

        let m = |i: usize, j: usize| f64::from(self.m[i][j]);
        let adjugate: [f64; 16] = [
            m(1, 2) * m(2, 3) * m(3, 1) - m(1, 3) * m(2, 2) * m(3, 1)
                + m(1, 3) * m(2, 1) * m(3, 2)
                - m(1, 1) * m(2, 3) * m(3, 2)
                - m(1, 2) * m(2, 1) * m(3, 3)
                + m(1, 1) * m(2, 2) * m(3, 3),
            m(0, 3) * m(2, 2) * m(3, 1)
                - m(0, 2) * m(2, 3) * m(3, 1)
                - m(0, 3) * m(2, 1) * m(3, 2)
                + m(0, 1) * m(2, 3) * m(3, 2)
                + m(0, 2) * m(2, 1) * m(3, 3)
                - m(0, 1) * m(2, 2) * m(3, 3),
            m(0, 2) * m(1, 3) * m(3, 1) - m(0, 3) * m(1, 2) * m(3, 1)
                + m(0, 3) * m(1, 1) * m(3, 2)
                - m(0, 1) * m(1, 3) * m(3, 2)
                - m(0, 2) * m(1, 1) * m(3, 3)
                + m(0, 1) * m(1, 2) * m(3, 3),
            m(0, 3) * m(1, 2) * m(2, 1)
                - m(0, 2) * m(1, 3) * m(2, 1)
                - m(0, 3) * m(1, 1) * m(2, 2)
                + m(0, 1) * m(1, 3) * m(2, 2)
                + m(0, 2) * m(1, 1) * m(2, 3)
                - m(0, 1) * m(1, 2) * m(2, 3),
            m(1, 3) * m(2, 2) * m(3, 0)
                - m(1, 2) * m(2, 3) * m(3, 0)
                - m(1, 3) * m(2, 0) * m(3, 2)
                + m(1, 0) * m(2, 3) * m(3, 2)
                + m(1, 2) * m(2, 0) * m(3, 3)
                - m(1, 0) * m(2, 2) * m(3, 3),
            m(0, 2) * m(2, 3) * m(3, 0) - m(0, 3) * m(2, 2) * m(3, 0)
                + m(0, 3) * m(2, 0) * m(3, 2)
                - m(0, 0) * m(2, 3) * m(3, 2)
                - m(0, 2) * m(2, 0) * m(3, 3)
                + m(0, 0) * m(2, 2) * m(3, 3),
            m(0, 3) * m(1, 2) * m(3, 0)
                - m(0, 2) * m(1, 3) * m(3, 0)
                - m(0, 3) * m(1, 0) * m(3, 2)
                + m(0, 0) * m(1, 3) * m(3, 2)
                + m(0, 2) * m(1, 0) * m(3, 3)
                - m(0, 0) * m(1, 2) * m(3, 3),
            m(0, 2) * m(1, 3) * m(2, 0) - m(0, 3) * m(1, 2) * m(2, 0)
                + m(0, 3) * m(1, 0) * m(2, 2)
                - m(0, 0) * m(1, 3) * m(2, 2)
                - m(0, 2) * m(1, 0) * m(2, 3)
                + m(0, 0) * m(1, 2) * m(2, 3),
            m(1, 1) * m(2, 3) * m(3, 0) - m(1, 3) * m(2, 1) * m(3, 0)
                + m(1, 3) * m(2, 0) * m(3, 1)
                - m(1, 0) * m(2, 3) * m(3, 1)
                - m(1, 1) * m(2, 0) * m(3, 3)
                + m(1, 0) * m(2, 1) * m(3, 3),
            m(0, 3) * m(2, 1) * m(3, 0)
                - m(0, 1) * m(2, 3) * m(3, 0)
                - m(0, 3) * m(2, 0) * m(3, 1)
                + m(0, 0) * m(2, 3) * m(3, 1)
                + m(0, 1) * m(2, 0) * m(3, 3)
                - m(0, 0) * m(2, 1) * m(3, 3),
            m(0, 1) * m(1, 3) * m(3, 0) - m(0, 3) * m(1, 1) * m(3, 0)
                + m(0, 3) * m(1, 0) * m(3, 1)
                - m(0, 0) * m(1, 3) * m(3, 1)
                - m(0, 1) * m(1, 0) * m(3, 3)
                + m(0, 0) * m(1, 1) * m(3, 3),
            m(0, 3) * m(1, 1) * m(2, 0)
                - m(0, 1) * m(1, 3) * m(2, 0)
                - m(0, 3) * m(1, 0) * m(2, 1)
                + m(0, 0) * m(1, 3) * m(2, 1)
                + m(0, 1) * m(1, 0) * m(2, 3)
                - m(0, 0) * m(1, 1) * m(2, 3),
            m(1, 2) * m(2, 1) * m(3, 0)
                - m(1, 1) * m(2, 2) * m(3, 0)
                - m(1, 2) * m(2, 0) * m(3, 1)
                + m(1, 0) * m(2, 2) * m(3, 1)
                + m(1, 1) * m(2, 0) * m(3, 2)
                - m(1, 0) * m(2, 1) * m(3, 2),
            m(0, 1) * m(2, 2) * m(3, 0) - m(0, 2) * m(2, 1) * m(3, 0)
                + m(0, 2) * m(2, 0) * m(3, 1)
                - m(0, 0) * m(2, 2) * m(3, 1)
                - m(0, 1) * m(2, 0) * m(3, 2)
                + m(0, 0) * m(2, 1) * m(3, 2),
            m(0, 2) * m(1, 1) * m(3, 0)
                - m(0, 1) * m(1, 2) * m(3, 0)
                - m(0, 2) * m(1, 0) * m(3, 1)
                + m(0, 0) * m(1, 2) * m(3, 1)
                + m(0, 1) * m(1, 0) * m(3, 2)
                - m(0, 0) * m(1, 1) * m(3, 2),
            m(0, 1) * m(1, 2) * m(2, 0) - m(0, 2) * m(1, 1) * m(2, 0)
                + m(0, 2) * m(1, 0) * m(2, 1)
                - m(0, 0) * m(1, 2) * m(2, 1)
                - m(0, 1) * m(1, 0) * m(2, 2)
                + m(0, 0) * m(1, 1) * m(2, 2),
        ];

        let mut inverse = Self::IDENTITY;
        for (index, cofactor) in adjugate.iter().enumerate() {
            let value = (cofactor / det) as f32;
            if !value.is_finite() {
                return None;
            }
            inverse.m[index / 4][index % 4] = value;
        }
        Some(inverse)
    }

    /// [`Self::try_inverse`], with the IDENTITY for a matrix that has no
    /// inverse. Only for a caller for which "leave it where it is" is a
    /// right answer; a renderer or a hit tester asks [`Self::try_inverse`]
    /// (a frame that cannot be inverted paints nothing and hits nothing).
    #[must_use]
    pub fn inverse(&self) -> Self {
        self.try_inverse().unwrap_or(Self::IDENTITY)
    }

    fn determinant(&self) -> f32 {
        #[allow(clippy::cast_possible_truncation)]
        // determinant computed in f64, narrowed to the f32 public type
        let det = self.determinant_f64() as f32;
        det
    }

    /// The determinant, accumulated in `f64`.
    #[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target
                                       // +fma; keep explicit a*b+c
    fn determinant_f64(&self) -> f64 {
        // Accumulate in f64. Individual f32 products (e.g. m[0][0]*m[1][1] on a
        // diag(1e20) matrix = 1e40) overflow to ±inf BEFORE the legitimately-zero
        // off-diagonal factors multiply in, and inf * 0 = NaN, poisoning the whole sum.
        // f64 has the range to hold the products; the cast to f32 in `determinant`
        // saturates a real overflow to ±inf and propagates a NaN input as NaN.
        let m = |i: usize, j: usize| f64::from(self.m[i][j]);
        m(0, 3) * m(1, 2) * m(2, 1) * m(3, 0)
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
            + m(0, 0) * m(1, 1) * m(2, 2) * m(3, 3)
    }

    /// Every entry times `x`. (The inverse used it; it divides in `f64` now,
    /// and the tests of this arithmetic keep it.)
    #[cfg(test)]
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
        // CSS Transforms Level 1 §9 ("The Transform Rendering Model"):
        //
        //   1. The functions are MULTIPLIED left to right, so the LAST listed function is the first
        //      one applied to a point: `translate(100px) scale(2)` scales first, then translates -
        //      a point at (1, 0) lands at (102, 0), not (202, 0). `a.then(b)` applies `a` first, so
        //      each function is composed BEFORE the accumulated rest. (This used to fold left to
        //      right - the reverse - so every multi-function transform rendered differently than in
        //      a browser, and `perspective() rotateX()` projected the un-rotated plane, i.e. did
        //      nothing.)
        //   2. The WHOLE product is applied about `transform-origin`: `translate(origin) * M *
        //      translate(-origin)`. Wrapping happens ONCE, here - not per component - so `scale()`
        //      and `skew()` pivot at the origin exactly like `rotate()` does.
        use azul_css::props::basic::{pixel::DEFAULT_FONT_SIZE, PixelValue};
        let no_origin = StyleTransformOrigin {
            x: PixelValue::const_px(0),
            y: PixelValue::const_px(0),
        };
        let mut matrix = Self::IDENTITY;
        let use_avx =
            INITIALIZED.load(AtomicOrdering::Relaxed) && USE_AVX.load(AtomicOrdering::Relaxed);
        let use_sse = !use_avx
            && INITIALIZED.load(AtomicOrdering::Relaxed)
            && USE_SSE.load(AtomicOrdering::Relaxed);

        if use_avx {
            for t in t_vec {
                let component = Self::from_style_transform(
                    t,
                    &no_origin,
                    percent_resolve_x,
                    percent_resolve_y,
                    rotation_mode,
                );
                // SAFETY: `use_avx` is only set when the AVX feature flag was
                // detected (see AUDIT-TODO above), so calling the AVX intrinsics
                // in `then_avx8` is legal on this CPU.
                #[cfg(target_arch = "x86_64")]
                unsafe {
                    matrix = component.then_avx8(&matrix);
                }
            }
        } else if use_sse {
            for t in t_vec {
                let component = Self::from_style_transform(
                    t,
                    &no_origin,
                    percent_resolve_x,
                    percent_resolve_y,
                    rotation_mode,
                );
                // SAFETY: `use_sse` is only set when the SSE feature flag was
                // detected (see AUDIT-TODO above), so calling the SSE intrinsics
                // in `then_sse` is legal on this CPU.
                #[cfg(target_arch = "x86_64")]
                unsafe {
                    matrix = component.then_sse(&matrix);
                }
            }
        } else {
            // fallback for everything else
            for t in t_vec {
                let component = Self::from_style_transform(
                    t,
                    &no_origin,
                    percent_resolve_x,
                    percent_resolve_y,
                    rotation_mode,
                );
                matrix = component.then(&matrix);
            }
        }

        // Percentages in `transform-origin` resolve against the element's
        // own border box (the caller passes its size as the percent basis).
        let origin_x = transform_origin.x.to_pixels_internal(
            percent_resolve_x,
            DEFAULT_FONT_SIZE,
            DEFAULT_FONT_SIZE,
        );
        let origin_y = transform_origin.y.to_pixels_internal(
            percent_resolve_y,
            DEFAULT_FONT_SIZE,
            DEFAULT_FONT_SIZE,
        );
        if origin_x == 0.0 && origin_y == 0.0 {
            return matrix;
        }
        Self::new_translation(-origin_x, -origin_y, 0.0)
            .then(&matrix)
            .then(&Self::new_translation(origin_x, origin_y, 0.0))
    }

    /// Creates a new transform from a style transform using the
    /// parent width as a way to resolve for percentages
    #[allow(clippy::many_single_char_names)] // domain-standard colour/coordinate component names
    #[allow(clippy::too_many_lines)] // large but cohesive: single-purpose parser/builder/dispatch
                                     // (one branch per input variant)
    fn from_style_transform(
        t: &StyleTransform,
        transform_origin: &StyleTransformOrigin,
        percent_resolve_x: f32,
        percent_resolve_y: f32,
        rotation_mode: RotationMode,
    ) -> Self {
        use azul_css::props::{
            basic::pixel::DEFAULT_FONT_SIZE,
            style::StyleTransform::{
                Matrix, Matrix3D, Perspective, Rotate, Rotate3D, RotateX, RotateY, RotateZ, Scale,
                Scale3D, ScaleX, ScaleY, ScaleZ, Skew, SkewX, SkewY, Translate, Translate3D,
                TranslateX, TranslateY, TranslateZ,
            },
        };
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
            Translate(trans2d) => Self::new_translation(
                trans2d.x.to_pixels_internal(
                    percent_resolve_x,
                    DEFAULT_FONT_SIZE,
                    DEFAULT_FONT_SIZE,
                ),
                trans2d.y.to_pixels_internal(
                    percent_resolve_y,
                    DEFAULT_FONT_SIZE,
                    DEFAULT_FONT_SIZE,
                ),
                0.0,
            ),
            Translate3D(trans3d) => {
                Self::new_translation(
                    trans3d.x.to_pixels_internal(
                        percent_resolve_x,
                        DEFAULT_FONT_SIZE,
                        DEFAULT_FONT_SIZE,
                    ),
                    trans3d.y.to_pixels_internal(
                        percent_resolve_y,
                        DEFAULT_FONT_SIZE,
                        DEFAULT_FONT_SIZE,
                    ),
                    trans3d
                        .z
                        // CSS has no containing block for Z-axis percentages; use X as fallback
                        .to_pixels_internal(percent_resolve_x, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE),
                )
            }
            TranslateX(trans_x) => Self::new_translation(
                trans_x.to_pixels_internal(percent_resolve_x, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE),
                0.0,
                0.0,
            ),
            TranslateY(trans_y) => Self::new_translation(
                0.0,
                trans_y.to_pixels_internal(percent_resolve_y, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE),
                0.0,
            ),
            TranslateZ(trans_z) => Self::new_translation(
                0.0,
                0.0,
                trans_z.to_pixels_internal(percent_resolve_x, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE),
            ), // CSS has no containing block for Z-axis percentages; use X as fallback
            Rotate3D(rot3d) => {
                let rotation_origin = (
                    transform_origin.x.to_pixels_internal(
                        percent_resolve_x,
                        DEFAULT_FONT_SIZE,
                        DEFAULT_FONT_SIZE,
                    ),
                    transform_origin.y.to_pixels_internal(
                        percent_resolve_y,
                        DEFAULT_FONT_SIZE,
                        DEFAULT_FONT_SIZE,
                    ),
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
                    transform_origin.x.to_pixels_internal(
                        percent_resolve_x,
                        DEFAULT_FONT_SIZE,
                        DEFAULT_FONT_SIZE,
                    ),
                    transform_origin.y.to_pixels_internal(
                        percent_resolve_y,
                        DEFAULT_FONT_SIZE,
                        DEFAULT_FONT_SIZE,
                    ),
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
                    transform_origin.x.to_pixels_internal(
                        percent_resolve_x,
                        DEFAULT_FONT_SIZE,
                        DEFAULT_FONT_SIZE,
                    ),
                    transform_origin.y.to_pixels_internal(
                        percent_resolve_y,
                        DEFAULT_FONT_SIZE,
                        DEFAULT_FONT_SIZE,
                    ),
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
                    transform_origin.x.to_pixels_internal(
                        percent_resolve_x,
                        DEFAULT_FONT_SIZE,
                        DEFAULT_FONT_SIZE,
                    ),
                    transform_origin.y.to_pixels_internal(
                        percent_resolve_y,
                        DEFAULT_FONT_SIZE,
                        DEFAULT_FONT_SIZE,
                    ),
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
                // CSS applies the WHOLE transform list about the
                // transform-origin, `perspective()` included: the vanishing
                // point sits at the origin. Building it about (0, 0) skewed a
                // `perspective() rotateX()` tilt towards the element's
                // top-left corner instead of keeping the centre line
                // vertical (the map's 3D tilt leaned sideways).
                let origin_x = transform_origin.x.to_pixels_internal(
                    percent_resolve_x,
                    DEFAULT_FONT_SIZE,
                    DEFAULT_FONT_SIZE,
                );
                let origin_y = transform_origin.y.to_pixels_internal(
                    percent_resolve_y,
                    DEFAULT_FONT_SIZE,
                    DEFAULT_FONT_SIZE,
                );
                let d =
                    px.to_pixels_internal(percent_resolve_x, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE);
                Self::new_translation(-origin_x, -origin_y, 0.0)
                    .then(&Self::new_perspective(d))
                    .then(&Self::new_translation(origin_x, origin_y, 0.0))
            }
        }
    }

    /// The plane z = 0 of this transform as a 3x3 homography.
    ///
    /// Over `(x, y, 1)` row vectors — `[x' y' w'] = [x y 1] * H`, row-major
    /// `[m00 m01 m03; m10 m11 m13; m30 m31 m33]` — i.e. exactly what a 2D
    /// compositor needs to place a flat layer: the affine part plus the
    /// perspective row. [`Self::is_plane_affine`] tells whether the
    /// perspective row is the trivial `[0 0 1]`.
    #[must_use]
    pub const fn plane_homography(&self) -> [f32; 9] {
        [
            self.m[0][0],
            self.m[0][1],
            self.m[0][3],
            self.m[1][0],
            self.m[1][1],
            self.m[1][3],
            self.m[3][0],
            self.m[3][1],
            self.m[3][3],
        ]
    }

    /// Does the z = 0 plane map affinely (no perspective foreshortening)?
    #[must_use]
    pub const fn is_plane_affine(&self) -> bool {
        let (a, b, w) = (self.m[0][3], self.m[1][3], self.m[3][3]);
        a > -1e-7 && a < 1e-7 && b > -1e-7 && b < 1e-7 && w > 1.0 - 1e-6 && w < 1.0 + 1e-6
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
    ///
    /// A depth below 1px is treated as 1px (CSS Transforms 2, `perspective()`:
    /// "if the depth value is less than 1px, it must be treated as 1px for
    /// the purpose of rendering"): `perspective(0)` is the strongest
    /// perspective there is, not a division by zero. A depth that is not a
    /// number is treated the same way.
    #[must_use]
    #[inline]
    fn new_perspective(d: f32) -> Self {
        let d = if d >= 1.0 { d } else { 1.0 };
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
    #[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target
                                       // +fma; keep explicit a*b+c
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

        // A point at or behind the eye (`w <= 0`) has no image, and neither
        // has one whose `w` is not a number: `w == +0.0` used to pass (it is
        // "sign positive") and the division gave an infinite point.
        if w.is_nan() || w <= 0.0 {
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

    /// This matrix, which maps LOGICAL (CSS) px to logical px, as the matrix
    /// that maps DEVICE px to device px at `scale_factor` device px per
    /// logical px: `S^-1 * M * S` with `S = scale(f, f, f)` (row vectors, so
    /// a device point is first brought to logical px, then moved by `M`,
    /// then brought back).
    ///
    /// Element by element that is: the linear 3x3 part unchanged, the
    /// TRANSLATION row (`m[3][0..3]`, lengths) times `f`, and the PERSPECTIVE
    /// column (`m[0..3][3]`, 1 / length: what a coordinate adds to `w`)
    /// divided by `f`. [`Self::scale_for_dpi`] does the translation row
    /// alone, which is the whole conversion only for an affine matrix: under
    /// `perspective()` a renderer that takes its coordinates in device px
    /// and leaves the perspective column as it is foreshortens `f` times too
    /// strongly (`w = 1 + f * x * m03` where `w = 1 + x * m03` is meant).
    ///
    /// The CPU compositor converts this way (`cpurender`'s `layer_h`); the
    /// `WebRender` translation hands its frames the result of this.
    /// A `scale_factor` that is not a positive finite number leaves the
    /// matrix as it is.
    #[must_use]
    pub fn to_device_px(&self, scale_factor: f32) -> Self {
        if !(scale_factor.is_finite() && scale_factor > 0.0) {
            return *self;
        }
        let mut m = self.m;
        m[3][0] *= scale_factor;
        m[3][1] *= scale_factor;
        m[3][2] *= scale_factor;
        m[0][3] /= scale_factor;
        m[1][3] /= scale_factor;
        m[2][3] /= scale_factor;
        Self { m }
    }

    /// Multiplies this matrix by `other`, applying `other` AFTER the current matrix.
    #[must_use]
    #[inline]
    #[allow(clippy::too_many_lines)] // large but cohesive: single-purpose parser/builder/dispatch
                                     // (one branch per input variant)
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
    unsafe fn linear_combine_sse(a: [f32; 4], b: &Self) -> [f32; 4] {
        unsafe {
            use core::{
                arch::x86_64::{__m128, _mm_add_ps, _mm_mul_ps, _mm_shuffle_ps},
                mem,
            };

            let a: __m128 = mem::transmute(a);
            let mut result = _mm_mul_ps(
                _mm_shuffle_ps(a, a, 0x00),
                mem::transmute::<[f32; 4], __m128>(b.m[0]),
            );
            result = _mm_add_ps(
                result,
                _mm_mul_ps(
                    _mm_shuffle_ps(a, a, 0x55),
                    mem::transmute::<[f32; 4], __m128>(b.m[1]),
                ),
            );
            result = _mm_add_ps(
                result,
                _mm_mul_ps(
                    _mm_shuffle_ps(a, a, 0xaa),
                    mem::transmute::<[f32; 4], __m128>(b.m[2]),
                ),
            );
            result = _mm_add_ps(
                result,
                _mm_mul_ps(
                    _mm_shuffle_ps(a, a, 0xff),
                    mem::transmute::<[f32; 4], __m128>(b.m[3]),
                ),
            );

            mem::transmute(result)
        }
    }

    /// Multiplies this matrix by `other` using SSE instructions.
    ///
    /// SAFETY: caller must guarantee SSE is available; only forwards to
    /// `linear_combine_sse`, whose safety contract is identical.
    #[cfg(target_arch = "x86_64")]
    #[inline]
    unsafe fn then_sse(&self, other: &Self) -> Self {
        unsafe {
            Self {
                m: [
                    Self::linear_combine_sse(self.m[0], other),
                    Self::linear_combine_sse(self.m[1], other),
                    Self::linear_combine_sse(self.m[2], other),
                    Self::linear_combine_sse(self.m[3], other),
                ],
            }
        }
    }

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
    ) -> core::arch::x86_64::__m256 {
        unsafe {
            use core::arch::x86_64::{
                _mm256_add_ps, _mm256_loadu2_m128, _mm256_mul_ps, _mm256_shuffle_ps,
            };

            // Unaligned broadcast of a row into both 128-bit lanes. Runs inside the
            // enclosing `unsafe` block, so the intrinsic call needs no inner `unsafe`.
            let broadcast_row = |row: &[f32; 4]| {
                let p = row.as_ptr();
                _mm256_loadu2_m128(p, p)
            };

            let mut result =
                _mm256_mul_ps(_mm256_shuffle_ps(a01, a01, 0x00), broadcast_row(&b.m[0]));
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
        }
    }

    /// Multiplies this matrix by `other` using AVX instructions.
    ///
    /// SAFETY: caller must guarantee AVX is available. Both `_mm256_loadu_ps`
    /// reads and `_mm256_storeu_ps` writes are *unaligned* 8-f32 (32-byte)
    /// accesses. `m` is `[[f32; 4]; 4]`, i.e. 16 contiguous f32 with no padding,
    /// so `&m[0][0]..` and `&m[2][0]..` each span two full rows in-bounds; the
    /// raw pointers come from live `self`/`out` locals, so lifetimes are valid.
    #[cfg(target_arch = "x86_64")]
    #[inline]
    unsafe fn then_avx8(&self, other: &Self) -> Self {
        unsafe {
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
        }
    }

    /// Creates a rotation matrix around the given axis, adjusted for the coordinate system.
    #[must_use]
    #[inline]
    #[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target
                                       // +fma; keep explicit a*b+c
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
        let rotate_transform = Self::new_rotation(axis_x, axis_y, axis_z, theta);

        pre_transform.then(&rotate_transform).then(&post_transform)
    }
}

#[cfg(test)]
#[allow(
    clippy::items_after_statements,
    clippy::redundant_clone,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    trivial_casts,
    clippy::borrow_as_ptr,
    clippy::cast_ptr_alignment,
    clippy::unused_self,
    unused_qualifications,
    unreachable_pub,
    private_interfaces
)] // pedantic lints are noise in unsafe-exercising test code
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
                core::ptr::copy_nonoverlapping((&raw const a).cast::<u8>(), off_ptr, N);
                let a_ref = &*off_ptr.cast::<ComputedTransform3D>();
                let got = a_ref.then_avx8(&b);
                approx_eq(&expected, &got);
            }
        }
    }
}

#[cfg(test)]
#[path = "transform_test.rs"]
mod transform_test;

/// A2-gpu (audit 2026-11-21): the conversion of a matrix from logical to
/// device pixels, `audit/A2-gpu.md` finding A2-gpu-01.
#[cfg(test)]
mod audit_a2_gpu_tests {
    use azul_css::props::basic::{AngleValue, PixelValue};

    use super::*;

    fn close(a: f32, b: f32, tolerance: f32) -> bool {
        (a - b).abs() <= tolerance
    }

    /// `perspective(400px) rotateY(60deg)` about the centre of a 100 x 40 box.
    fn tilt() -> ComputedTransform3D {
        ComputedTransform3D::from_style_transform_vec(
            &[
                StyleTransform::Perspective(PixelValue::px(400.0)),
                StyleTransform::RotateY(AngleValue::deg(60.0)),
            ],
            &StyleTransformOrigin::default(),
            100.0,
            40.0,
            RotationMode::ForWebRender,
        )
    }

    fn at(m: &ComputedTransform3D, x: f32, y: f32) -> LogicalPosition {
        m.transform_point2d(LogicalPosition::new(x, y))
            .expect("in front of the eye")
    }

    /// The hand-computed matrix of the example: with c = cos 60 = 0.5,
    /// s = sin 60 = 0.8660254, d = 400 and the origin o = (50, 20),
    /// `T(-o) * R * P * T(o)` has the rows
    /// `[c + o.x s/d, o.y s/d, -s, s/d]`, `[0, 1, 0, 0]` and the translation
    /// row `[-o.x c + (1 - o.x s/d) o.x, -o.y + (1 - o.x s/d) o.y, o.x s, 1 - o.x s/d]`.
    #[test]
    fn the_tilt_of_the_example_is_the_hand_computed_matrix() {
        let m = tilt().m;
        assert!(close(m[0][0], 0.608_253, 1e-4), "{m:?}");
        assert!(close(m[0][1], 0.043_301, 1e-4), "{m:?}");
        assert!(close(m[0][3], 0.002_165_06, 1e-6), "{m:?}");
        assert!(close(m[1][1], 1.0, 1e-5) && close(m[1][3], 0.0, 1e-7), "{m:?}");
        assert!(close(m[3][0], 19.587_35, 1e-3), "{m:?}");
        assert!(close(m[3][1], -2.165_06, 1e-3), "{m:?}");
        assert!(close(m[3][3], 0.891_747, 1e-5), "{m:?}");
    }

    /// The box's top-right corner (100, 0), in logical px: (72.558, 1.954).
    /// At 2 device px per logical px the same corner is the device point
    /// (200, 0) and lands at twice that, (145.116, 3.907). The matrix whose
    /// translation alone was scaled puts it at (121.400, 3.269): 23.7 device
    /// px to the left.
    ///
    /// Mutation: `m[0][3] /= scale_factor` -> `m[0][3]` unchanged in
    /// `to_device_px` fails this test.
    #[test]
    fn a_perspective_matrix_in_device_px_draws_a_point_where_the_logical_one_does() {
        let m = tilt();
        let logical = at(&m, 100.0, 0.0);
        assert!(
            close(logical.x, 72.558, 0.01) && close(logical.y, 1.954, 0.01),
            "{logical:?}"
        );

        let device = at(&m.to_device_px(2.0), 200.0, 0.0);
        assert!(
            close(device.x, 145.116, 0.02) && close(device.y, 3.907, 0.02),
            "{device:?}"
        );

        let mut translation_only = m;
        translation_only.scale_for_dpi(2.0);
        let wrong = at(&translation_only, 200.0, 0.0);
        assert!(
            close(wrong.x, 121.400, 0.02) && close(wrong.y, 3.269, 0.02),
            "what a translation-only scaling gives: {wrong:?}"
        );
    }

    /// The property behind the example: for every point and every scale, the
    /// device matrix maps `f * p` to `f * M(p)`.
    #[test]
    fn the_device_matrix_commutes_with_the_scale() {
        let m = tilt();
        for f in [0.5_f32, 1.0, 1.25, 1.5, 2.0, 3.0] {
            let device = m.to_device_px(f);
            for (x, y) in [
                (0.0_f32, 0.0_f32),
                (100.0, 0.0),
                (100.0, 40.0),
                (0.0, 40.0),
                (50.0, 20.0),
                (-30.0, 75.0),
            ] {
                let want = at(&m, x, y);
                let got = at(&device, x * f, y * f);
                assert!(
                    close(got.x, want.x * f, 0.01 * f.max(1.0))
                        && close(got.y, want.y * f, 0.01 * f.max(1.0)),
                    "scale {f}, point ({x}, {y}): {got:?}, want {f} x {want:?}"
                );
            }
        }
    }

    /// An affine matrix has no perspective column to divide: there the
    /// conversion is the scaling of the translation row.
    #[test]
    fn an_affine_matrix_in_device_px_only_has_its_translation_scaled() {
        // scale(2) about (50, 20): p -> 2 p - (50, 20)
        let m = ComputedTransform3D::new_translation(-50.0, -20.0, 0.0)
            .then(&ComputedTransform3D::new_scale(2.0, 2.0, 1.0))
            .then(&ComputedTransform3D::new_translation(50.0, 20.0, 0.0));
        assert!(close(m.m[3][0], -50.0, 1e-4) && close(m.m[3][1], -20.0, 1e-4));
        let mut translation_only = m;
        translation_only.scale_for_dpi(2.0);
        assert_eq!(m.to_device_px(2.0), translation_only);
        assert!(close(m.to_device_px(2.0).m[3][0], -100.0, 1e-4));
    }

    /// A scale that is no scale leaves the matrix alone (no division by 0).
    #[test]
    fn a_degenerate_scale_leaves_the_matrix_as_it_is() {
        let m = tilt();
        for f in [0.0_f32, -2.0, f32::NAN, f32::INFINITY] {
            assert_eq!(m.to_device_px(f), m, "scale {f}");
        }
        assert_eq!(m.to_device_px(1.0), m);
    }

    /// A2-gpu-11. `inverse()` answered the IDENTITY for every matrix whose
    /// determinant was below `f32::EPSILON` in size - a matrix that has no
    /// inverse and a zoomed-out view alike - and a caller could not tell.
    ///
    /// By hand: `M: p -> (2 x + 10, 4 y + 20)` (scale(2, 4), then a move by
    /// (10, 20)). Its inverse is `p -> ((x - 10) / 2, (y - 20) / 4)`: the
    /// matrix diag(0.5, 0.25) with the translation (-5, -5); the point
    /// (14, 28) goes back to (2, 2). `scale(0.001)`, determinant 1e-9: its
    /// inverse is `scale(1000)` (old: the identity, so (3, 3) went "back" to
    /// (3, 3) instead of (3000, 3000)). `scale(0, 1)`: no inverse (old: the
    /// identity, as if the collapsed box were where it was laid out).
    ///
    /// Mutations: `inverse.m[index / 4][index % 4]` -> `[index % 4][index / 4]`
    /// in `try_inverse` fails "(14, 28) goes back to (2, 2)"; `det == 0.0 ||
    /// !det.is_finite()` -> `det.abs() < f64::from(f32::EPSILON)` fails "a
    /// zoomed-out view has an inverse".
    #[test]
    fn a_matrix_says_when_it_has_no_inverse() {
        let m = ComputedTransform3D::new_scale(2.0, 4.0, 1.0)
            .then(&ComputedTransform3D::new_translation(10.0, 20.0, 0.0));
        let forward = at(&m, 2.0, 2.0);
        assert_eq!((forward.x, forward.y), (14.0, 28.0));
        let back = m.try_inverse().expect("a scale by 2 and 4 has an inverse");
        assert_eq!(back.m[0][0], 0.5);
        assert_eq!(back.m[1][1], 0.25);
        assert_eq!((back.m[3][0], back.m[3][1]), (-5.0, -5.0));
        let p = at(&back, 14.0, 28.0);
        assert_eq!((p.x, p.y), (2.0, 2.0), "(14, 28) goes back to (2, 2)");
        assert_eq!(m.inverse(), back);

        let zoomed_out = ComputedTransform3D::new_scale(0.001, 0.001, 1.0);
        let zoomed_in = zoomed_out
            .try_inverse()
            .expect("a zoomed-out view has an inverse");
        let p = at(&zoomed_in, 3.0, 3.0);
        assert!(
            close(p.x, 3000.0, 0.01) && close(p.y, 3000.0, 0.01),
            "scale(0.001) inverts to scale(1000): {p:?}"
        );

        let collapsed = ComputedTransform3D::new_scale(0.0, 1.0, 1.0);
        assert_eq!(collapsed.try_inverse(), None, "a box collapsed to a line has no inverse");
        assert_eq!(collapsed.inverse(), ComputedTransform3D::IDENTITY);

        // A perspective tilt is inverted too: there and back is where it was.
        let there_and_back = tilt().then(&tilt().try_inverse().expect("a tilt of 60 degrees"));
        for row in 0..4 {
            for column in 0..4 {
                let want = if row == column { 1.0 } else { 0.0 };
                assert!(
                    close(there_and_back.m[row][column], want, 1e-4),
                    "m[{row}][{column}] = {}",
                    there_and_back.m[row][column]
                );
            }
        }
    }

    /// A2-gpu-11. `perspective(0)` made `m[2][3] = -1 / 0 = -inf`: under
    /// `perspective(0) rotateY(60deg)` every point's `w` was infinite and the
    /// whole box was drawn in one point (or nowhere). CSS Transforms 2: a
    /// depth below 1px is treated as 1px.
    ///
    /// By hand, about the origin (0, 0): the point (10, 0) turns to
    /// x = 10 cos 60 = 5, z = -10 sin 60 = -8.660; w = 1 - z / 1px = 9.660;
    /// it is drawn at x = 5 / 9.660 = 0.5176. Old: x = 5 / inf = 0.
    ///
    /// Mutation: `let d = if d >= 1.0 { d } else { 1.0 };` removed from
    /// `new_perspective` fails "a depth of 0 is a depth of 1px".
    #[test]
    fn a_perspective_of_less_than_a_pixel_is_one_pixel() {
        let corner = StyleTransformOrigin {
            x: PixelValue::px(0.0),
            y: PixelValue::px(0.0),
        };
        let tilt_at = |depth: f32| {
            ComputedTransform3D::from_style_transform_vec(
                &[
                    StyleTransform::Perspective(PixelValue::px(depth)),
                    StyleTransform::RotateY(AngleValue::deg(60.0)),
                ],
                &corner,
                100.0,
                40.0,
                RotationMode::ForWebRender,
            )
        };
        let zero = tilt_at(0.0);
        assert!(
            zero.m.iter().flatten().all(|v| v.is_finite()),
            "perspective(0) is a finite matrix: {zero:?}"
        );
        assert_eq!(zero, tilt_at(1.0), "a depth of 0 is a depth of 1px");
        assert_eq!(tilt_at(0.5), tilt_at(1.0));
        let p = at(&zero, 10.0, 0.0);
        assert!(close(p.x, 0.5176, 1e-3) && close(p.y, 0.0, 1e-6), "{p:?}");
        assert_ne!(tilt_at(2.0), tilt_at(1.0), "two pixels are two pixels");
    }
}

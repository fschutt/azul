//! CSS length and pixel value types, parsing, and unit resolution.
//!
//! Defines `PixelValue` (a numeric value + CSS unit like px, em, rem, %),
//! `ResolutionContext` (contextual information for resolving relative units),
//! and `PropertyContext` (which property is being resolved, affecting % and em semantics).
//!
//! **Resolution paths:**
//! - `resolve_with_context()` — the correct method for new code; properly distinguishes
//!   em vs rem, and resolves % based on property type per the CSS spec.
//! - `to_pixels_internal()` — legacy fallback used by `prop_cache.rs`; does not
//!   distinguish rem from em. Marked `#[doc(hidden)]`.

use core::fmt;
use std::num::ParseFloatError;
use crate::corety::AzString;

use crate::props::{
    basic::{error::ParseFloatErrorWithInput, FloatValue, SizeMetric},
    formatter::FormatAsCssValue,
};

/// Default font size in pixels (16px), matching the CSS "medium" keyword
/// and all major browser defaults (CSS 2.1 §15.7).
pub const DEFAULT_FONT_SIZE: f32 = 16.0;

/// Conversion factor from points to pixels (1pt = 1/72 inch, 1in = 96px, therefore 1pt = 96/72 px)
pub const PT_TO_PX: f32 = 96.0 / 72.0;

/// A normalized percentage value (0.0 = 0%, 1.0 = 100%)
///
/// This type prevents double-division bugs by making it explicit that the value
/// is already normalized to the 0.0-1.0 range. When you have a `NormalizedPercentage`,
/// you should multiply it directly with the containing block size, NOT divide by 100 again.
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct NormalizedPercentage(f32);

impl NormalizedPercentage {
    /// Create a new percentage value from a normalized float (0.0-1.0)
    ///
    /// # Arguments
    /// * `value` - A normalized percentage where 0.0 = 0% and 1.0 = 100%
    #[inline]
    #[must_use] pub const fn new(value: f32) -> Self {
        Self(value)
    }

    /// Create a percentage from an unnormalized value (0-100 scale)
    ///
    /// This divides by 100 internally, so you should use this when converting
    /// from CSS percentage syntax like "50%" which is stored as 50.0.
    #[inline]
    #[must_use] pub fn from_unnormalized(value: f32) -> Self {
        Self(value / 100.0)
    }

    /// Get the raw normalized value (0.0-1.0)
    #[inline]
    #[must_use] pub const fn get(self) -> f32 {
        self.0
    }

    /// Resolve this percentage against a containing block size
    ///
    /// This multiplies the normalized percentage by the containing block size.
    /// For example, 50% (0.5) of 640px = 320px.
    #[inline]
    #[must_use] pub fn resolve(self, containing_block_size: f32) -> f32 {
        self.0 * containing_block_size
    }
}

impl fmt::Display for NormalizedPercentage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}%", self.0 * 100.0)
    }
}

/// Logical size in CSS logical coordinate system
#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(C)]
pub struct CssLogicalSize {
    /// Inline-axis size (width in horizontal writing mode)
    pub inline_size: f32,
    /// Block-axis size (height in horizontal writing mode)
    pub block_size: f32,
}

impl CssLogicalSize {
    #[inline]
    #[must_use] pub const fn new(inline_size: f32, block_size: f32) -> Self {
        Self {
            inline_size,
            block_size,
        }
    }

    /// Convert to physical size (width, height) in horizontal writing mode
    #[inline]
    #[must_use] pub const fn to_physical(self) -> PhysicalSize {
        PhysicalSize {
            width: self.inline_size,
            height: self.block_size,
        }
    }
}

/// Physical size (always width x height, regardless of writing mode)
#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(C)]
pub struct PhysicalSize {
    pub width: f32,
    pub height: f32,
}

impl PhysicalSize {
    #[inline]
    #[must_use] pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }

    /// Convert to logical size in horizontal writing mode
    #[inline]
    #[must_use] pub const fn to_logical(self) -> CssLogicalSize {
        CssLogicalSize {
            inline_size: self.width,
            block_size: self.height,
        }
    }
}

/// Context information needed to properly resolve CSS units (em, rem, %) to pixels.
///
/// This struct contains all the contextual information that `PixelValue::resolve()`
/// needs to correctly convert relative units according to the CSS specification:
///
/// - **em** units: For most properties, em refers to the element's own computed font-size. For the
///   font-size property itself, em refers to the parent's computed font-size.
///
/// - **rem** units: Always refer to the root element's computed font-size.
///
/// - **%** units: Percentage resolution depends on the property:
///   - Width/height: relative to containing block dimensions
///   - Margin/padding: relative to containing block width (even top/bottom!)
///   - Border-radius: relative to element's own border box dimensions
///   - Font-size: relative to parent's font-size
#[derive(Debug, Copy, Clone)]
pub struct ResolutionContext {
    /// The computed font-size of the current element (for em in non-font properties)
    pub element_font_size: f32,

    /// The computed font-size of the parent element (for em in font-size property)
    pub parent_font_size: f32,

    /// The computed font-size of the root element (for rem units)
    pub root_font_size: f32,

    /// The containing block dimensions (for % in width/height/margins/padding)
    pub containing_block_size: PhysicalSize,

    /// The element's own border box size (for % in border-radius, transforms)
    /// May be None during first layout pass before size is determined
    pub element_size: Option<PhysicalSize>,

    /// The viewport size in CSS pixels (for vw, vh, vmin, vmax units)
    /// This is the layout viewport size, not physical screen size
    pub viewport_size: PhysicalSize,
}

impl Default for ResolutionContext {
    fn default() -> Self {
        Self {
            element_font_size: 16.0,
            parent_font_size: 16.0,
            root_font_size: 16.0,
            containing_block_size: PhysicalSize::new(0.0, 0.0),
            element_size: None,
            viewport_size: PhysicalSize::new(0.0, 0.0),
        }
    }
}

impl ResolutionContext {
    /// Create a minimal context for testing or default resolution
    #[inline]
    #[must_use] pub const fn default_const() -> Self {
        Self {
            element_font_size: 16.0,
            parent_font_size: 16.0,
            root_font_size: 16.0,
            containing_block_size: PhysicalSize {
                width: 0.0,
                height: 0.0,
            },
            element_size: None,
            viewport_size: PhysicalSize {
                width: 0.0,
                height: 0.0,
            },
        }
    }

}

/// Specifies which property context we're resolving for, to determine correct reference values
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum PropertyContext {
    /// Resolving for the font-size property itself (em refers to parent)
    FontSize,
    /// Resolving for margin properties (% refers to containing block width)
    Margin,
    /// Resolving for padding properties (% refers to containing block width)
    Padding,
    /// Resolving for width or horizontal properties (% refers to containing block width)
    Width,
    /// Resolving for height or vertical properties (% refers to containing block height)
    Height,
    /// Resolving for border-width properties (only absolute lengths + em/rem, no % support)
    BorderWidth,
    /// Resolving for border-radius (% refers to element's own dimensions)
    BorderRadius,
    /// Resolving for transforms (% refers to element's own dimensions)
    Transform,
    /// Resolving for other properties (em refers to element font-size)
    Other,
}

/// A CSS length value consisting of a numeric value and a unit (px, em, rem, %, etc.).
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct PixelValue {
    pub metric: SizeMetric,
    pub number: FloatValue,
}

impl PixelValue {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        self.number = FloatValue::new(self.number.get() * scale_factor);
    }
}

impl FormatAsCssValue for PixelValue {
    fn format_as_css_value(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.number, self.metric)
    }
}

impl crate::css::PrintAsCssValue for PixelValue {
    fn print_as_css_value(&self) -> String {
        format!("{}{}", self.number, self.metric)
    }
}

impl crate::codegen::format::FormatAsRustCode for PixelValue {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "PixelValue {{ metric: {:?}, number: FloatValue::new({}) }}",
            self.metric,
            self.number.get()
        )
    }
}

// Manual Debug implementation, because the auto-generated one is nearly unreadable
impl fmt::Debug for PixelValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.number, self.metric)
    }
}

impl fmt::Display for PixelValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.number, self.metric)
    }
}

impl PixelValue {
    #[inline]
    #[must_use] pub const fn zero() -> Self {
        const ZERO_PX: PixelValue = PixelValue::const_px(0);
        ZERO_PX
    }

    /// Same as `PixelValue::px()`, but only accepts whole numbers,
    /// since using `f32` in const fn is not yet stabilized.
    #[inline]
    #[must_use] pub const fn const_px(value: isize) -> Self {
        Self::const_from_metric(SizeMetric::Px, value)
    }

    /// Same as `PixelValue::em()`, but only accepts whole numbers,
    /// since using `f32` in const fn is not yet stabilized.
    #[inline]
    #[must_use] pub const fn const_em(value: isize) -> Self {
        Self::const_from_metric(SizeMetric::Em, value)
    }

    /// Creates an em value from a fractional number in const context.
    ///
    /// # Arguments
    /// * `pre_comma` - The integer part (e.g., 1 for 1.5em)
    /// * `post_comma` - The fractional part as digits (e.g., 5 for 0.5em, 83 for 0.83em)
    ///
    /// # Examples
    /// ```
    /// // 1.5em = const_em_fractional(1, 5)
    /// // 0.83em = const_em_fractional(0, 83)
    /// // 1.17em = const_em_fractional(1, 17)
    /// ```
    #[inline]
    #[must_use] pub const fn const_em_fractional(pre_comma: isize, post_comma: isize) -> Self {
        Self::const_from_metric_fractional(SizeMetric::Em, pre_comma, post_comma)
    }

    /// Same as `PixelValue::pt()`, but only accepts whole numbers,
    /// since using `f32` in const fn is not yet stabilized.
    #[inline]
    #[must_use] pub const fn const_pt(value: isize) -> Self {
        Self::const_from_metric(SizeMetric::Pt, value)
    }

    /// Creates a pt value from a fractional number in const context.
    #[inline]
    #[must_use] pub const fn const_pt_fractional(pre_comma: isize, post_comma: isize) -> Self {
        Self::const_from_metric_fractional(SizeMetric::Pt, pre_comma, post_comma)
    }

    /// Same as `PixelValue::percent()`, but only accepts whole numbers,
    /// since using `f32` in const fn is not yet stabilized.
    #[inline]
    #[must_use] pub const fn const_percent(value: isize) -> Self {
        Self::const_from_metric(SizeMetric::Percent, value)
    }

    /// Same as `PixelValue::in()`, but only accepts whole numbers,
    /// since using `f32` in const fn is not yet stabilized.
    #[inline]
    #[must_use] pub const fn const_in(value: isize) -> Self {
        Self::const_from_metric(SizeMetric::In, value)
    }

    /// Same as `PixelValue::cm()`, but only accepts whole numbers,
    /// since using `f32` in const fn is not yet stabilized.
    #[inline]
    #[must_use] pub const fn const_cm(value: isize) -> Self {
        Self::const_from_metric(SizeMetric::Cm, value)
    }

    /// Same as `PixelValue::mm()`, but only accepts whole numbers,
    /// since using `f32` in const fn is not yet stabilized.
    #[inline]
    #[must_use] pub const fn const_mm(value: isize) -> Self {
        Self::const_from_metric(SizeMetric::Mm, value)
    }

    #[inline]
    #[must_use] pub const fn const_from_metric(metric: SizeMetric, value: isize) -> Self {
        Self {
            metric,
            number: FloatValue::const_new(value),
        }
    }

    /// Creates a `PixelValue` from a fractional number in const context.
    ///
    /// # Arguments
    /// * `metric` - The size metric (Px, Em, Pt, etc.)
    /// * `pre_comma` - The integer part
    /// * `post_comma` - The fractional part as digits
    #[inline]
    #[must_use] pub const fn const_from_metric_fractional(
        metric: SizeMetric,
        pre_comma: isize,
        post_comma: isize,
    ) -> Self {
        Self {
            metric,
            number: FloatValue::const_new_fractional(pre_comma, post_comma),
        }
    }

    #[inline]
    #[must_use] pub fn px(value: f32) -> Self {
        Self::from_metric(SizeMetric::Px, value)
    }

    #[inline]
    #[must_use] pub fn em(value: f32) -> Self {
        Self::from_metric(SizeMetric::Em, value)
    }

    #[inline]
    #[must_use] pub fn inch(value: f32) -> Self {
        Self::from_metric(SizeMetric::In, value)
    }

    #[inline]
    #[must_use] pub fn cm(value: f32) -> Self {
        Self::from_metric(SizeMetric::Cm, value)
    }

    #[inline]
    #[must_use] pub fn mm(value: f32) -> Self {
        Self::from_metric(SizeMetric::Mm, value)
    }

    #[inline]
    #[must_use] pub fn pt(value: f32) -> Self {
        Self::from_metric(SizeMetric::Pt, value)
    }

    #[inline]
    #[must_use] pub fn percent(value: f32) -> Self {
        Self::from_metric(SizeMetric::Percent, value)
    }

    #[inline]
    #[must_use] pub fn rem(value: f32) -> Self {
        Self::from_metric(SizeMetric::Rem, value)
    }

    #[inline]
    #[must_use] pub fn from_metric(metric: SizeMetric, value: f32) -> Self {
        Self {
            metric,
            number: FloatValue::new(value),
        }
    }

    #[inline]
    #[allow(clippy::suboptimal_flops)] // explicit FP; mul_add slower without +fma
    #[must_use] pub fn interpolate(&self, other: &Self, t: f32) -> Self {
        if self.metric == other.metric {
            Self {
                metric: self.metric,
                number: self.number.interpolate(&other.number, t),
            }
        } else {
            // Interpolate between different metrics by converting to px
            // Note: Uses DEFAULT_FONT_SIZE for em/rem - acceptable for animation fallback
            let self_px_interp = self.to_pixels_internal(0.0, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE);
            let other_px_interp = other.to_pixels_internal(0.0, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE);
            Self::from_metric(
                SizeMetric::Px,
                self_px_interp + (other_px_interp - self_px_interp) * t,
            )
        }
    }

    /// Returns the value of the `SizeMetric` as a normalized percentage (0.0 = 0%, 1.0 = 100%)
    ///
    /// Returns `Some(NormalizedPercentage)` if this is a percentage value, `None` otherwise.
    /// The returned `NormalizedPercentage` is already normalized to 0.0-1.0 range,
    /// so you should multiply it directly with the containing block size.
    #[inline]
    #[must_use] pub fn to_percent(&self) -> Option<NormalizedPercentage> {
        match self.metric {
            SizeMetric::Percent => Some(NormalizedPercentage::from_unnormalized(self.number.get())),
            _ => None,
        }
    }

    /// Internal fallback method for converting to pixels with manual % resolution.
    ///
    /// Used internally by prop_cache.rs resolve_property_dependency().
    ///
    /// **DO NOT USE directly!** Use `resolve_with_context()` instead for new code.
    #[doc(hidden)]
    #[inline]
    #[must_use] pub fn to_pixels_internal(&self, percent_resolve: f32, em_resolve: f32, rem_resolve: f32) -> f32 {
        match self.metric {
            SizeMetric::Px => self.number.get(),
            SizeMetric::Pt => self.number.get() * PT_TO_PX,
            SizeMetric::In => self.number.get() * 96.0,
            SizeMetric::Cm => self.number.get() * 96.0 / 2.54,
            SizeMetric::Mm => self.number.get() * 96.0 / 25.4,
            SizeMetric::Em => self.number.get() * em_resolve,
            SizeMetric::Rem => self.number.get() * rem_resolve,
            SizeMetric::Percent => {
                NormalizedPercentage::from_unnormalized(self.number.get()).resolve(percent_resolve)
            }
            // Viewport units: Cannot resolve without viewport context, return 0
            // These should use resolve_with_context() instead
            SizeMetric::Vw | SizeMetric::Vh | SizeMetric::Vmin | SizeMetric::Vmax => 0.0,
        }
    }

    /// Resolve this value to pixels using proper CSS context.
    ///
    /// This is the **CORRECT** way to resolve CSS units. It properly handles:
    /// - em units: Uses element's own font-size (or parent's for font-size property)
    /// - rem units: Uses root element's font-size
    /// - % units: Uses property-appropriate reference (containing block width/height, element size,
    ///   etc.)
    /// - Absolute units: px, pt, in, cm, mm (already correct)
    ///
    /// # Arguments
    /// * `context` - Resolution context with font sizes and dimensions
    /// * `property_context` - Which property we're resolving for (affects % and em resolution)
    #[inline]
    #[must_use] pub fn resolve_with_context(
        &self,
        context: &ResolutionContext,
        property_context: PropertyContext,
    ) -> f32 {
        match self.metric {
            // Absolute units - already correct
            SizeMetric::Px => self.number.get(),
            SizeMetric::Pt => self.number.get() * PT_TO_PX,
            SizeMetric::In => self.number.get() * 96.0,
            SizeMetric::Cm => self.number.get() * 96.0 / 2.54,
            SizeMetric::Mm => self.number.get() * 96.0 / 25.4,

            // Em units - CRITICAL: different resolution for font-size vs other properties
            SizeMetric::Em => {
                let reference_font_size = if property_context == PropertyContext::FontSize {
                    // Em on font-size refers to parent's font-size (CSS 2.1 §15.7)
                    context.parent_font_size
                } else {
                    // Em on other properties refers to element's own font-size (CSS 2.1 §10.5)
                    context.element_font_size
                };
                self.number.get() * reference_font_size
            }

            // Rem units - ALWAYS refer to root font-size (CSS Values 3)
            SizeMetric::Rem => self.number.get() * context.root_font_size,

            // Viewport units - refer to viewport dimensions (CSS Values 3 §6.2)
            // 1vw = 1% of viewport width, 1vh = 1% of viewport height
            SizeMetric::Vw => self.number.get() * context.viewport_size.width / 100.0,
            SizeMetric::Vh => self.number.get() * context.viewport_size.height / 100.0,
            // vmin = smaller of vw or vh
            SizeMetric::Vmin => {
                let min_dimension = context
                    .viewport_size
                    .width
                    .min(context.viewport_size.height);
                self.number.get() * min_dimension / 100.0
            }
            // vmax = larger of vw or vh
            SizeMetric::Vmax => {
                let max_dimension = context
                    .viewport_size
                    .width
                    .max(context.viewport_size.height);
                self.number.get() * max_dimension / 100.0
            }

            // Percent units - reference depends on property type
            SizeMetric::Percent => {
                // Width and Other deliberately both resolve to containing-block width but are
                // kept as separate arms for documentation / likely future divergence.
                #[allow(clippy::match_same_arms)]
                let reference = match property_context {
                    // Font-size %: refers to parent's font-size (CSS 2.1 §15.7)
                    PropertyContext::FontSize => context.parent_font_size,

                    // Width and horizontal properties: containing block width (CSS 2.1 §10.3)
                    PropertyContext::Width => context.containing_block_size.width,

                    // Height and vertical properties: containing block height (CSS 2.1 §10.5)
                    PropertyContext::Height => context.containing_block_size.height,

                    // +spec:box-model:66e123 - margin/padding % resolved against inline size (= width in horizontal-tb)
                    // +spec:width-calculation:bef810 - margin percentages refer to containing block width (even top/bottom)
                    // Margins: ALWAYS containing block WIDTH, even for top/bottom! (CSS 2.1 §8.3)
                    // +spec:width-calculation:d78514 - margin percentages refer to width of containing block
                    // Padding: ALWAYS containing block WIDTH, even for top/bottom! (CSS 2.1 §8.4)
                    PropertyContext::Margin | PropertyContext::Padding => {
                        context.containing_block_size.width
                    }

                    // Border-width: % is NOT valid per CSS spec (CSS Backgrounds 3 §4.1)
                    // Return 0.0 if someone tries to use % on border-width
                    PropertyContext::BorderWidth => 0.0,

                    // Border-radius: element's own dimensions (CSS Backgrounds 3 §5.1)
                    // Note: More complex - horizontal % uses width, vertical % uses height
                    // For now, use width as default
                    PropertyContext::BorderRadius => {
                        context.element_size.map_or(0.0, |s| s.width)
                    }

                    // Transforms: element's own dimensions (CSS Transforms §20.1)
                    PropertyContext::Transform => {
                        context.element_size.map_or(0.0, |s| s.width)
                    }

                    // Other properties: default to containing block width
                    PropertyContext::Other => context.containing_block_size.width,
                };

                NormalizedPercentage::from_unnormalized(self.number.get()).resolve(reference)
            }
        }
    }
}

// border-width: thin / medium / thick keyword values
// These are the canonical CSS definitions and should be used consistently
// across parsing and resolution.

/// border-width: thin = 1px (per CSS spec)
pub const THIN_BORDER_THICKNESS: PixelValue = PixelValue {
    metric: SizeMetric::Px,
    number: FloatValue { number: 1000 },
};

/// border-width: medium = 3px (per CSS spec, default)
pub const MEDIUM_BORDER_THICKNESS: PixelValue = PixelValue {
    metric: SizeMetric::Px,
    number: FloatValue { number: 3000 },
};

/// border-width: thick = 5px (per CSS spec)
pub const THICK_BORDER_THICKNESS: PixelValue = PixelValue {
    metric: SizeMetric::Px,
    number: FloatValue { number: 5000 },
};

/// Same as `PixelValue`, but doesn't allow a "%" sign
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct PixelValueNoPercent {
    pub inner: PixelValue,
}

impl PixelValueNoPercent {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        self.inner.scale_for_dpi(scale_factor);
    }
}

impl_option!(
    PixelValueNoPercent,
    OptionPixelValueNoPercent,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl_option!(
    PixelValue,
    OptionPixelValue,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl fmt::Display for PixelValueNoPercent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl ::core::fmt::Debug for PixelValueNoPercent {
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        write!(f, "{self}")
    }
}

impl PixelValueNoPercent {
    /// Internal conversion to pixels (no percent support).
    ///
    /// Used internally by prop_cache.rs.
    ///
    /// **DO NOT USE directly!** Use `resolve_with_context()` on inner value instead.
    #[doc(hidden)]
    #[inline]
    #[must_use] pub fn to_pixels_internal(&self, em_resolve: f32, rem_resolve: f32) -> f32 {
        self.inner.to_pixels_internal(0.0, em_resolve, rem_resolve)
    }

    #[inline]
    #[must_use] pub const fn zero() -> Self {
        const ZERO_PXNP: PixelValueNoPercent = PixelValueNoPercent {
            inner: PixelValue::zero(),
        };
        ZERO_PXNP
    }
}
impl From<PixelValue> for PixelValueNoPercent {
    fn from(e: PixelValue) -> Self {
        Self { inner: e }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub enum CssPixelValueParseError<'a> {
    EmptyString,
    NoValueGiven(&'a str, SizeMetric),
    ValueParseErr(ParseFloatError, &'a str),
    InvalidPixelValue(&'a str),
}

impl_debug_as_display!(CssPixelValueParseError<'a>);

impl_display! { CssPixelValueParseError<'a>, {
    EmptyString => format!("Missing [px / pt / em / %] value"),
    NoValueGiven(input, metric) => format!("Expected floating-point pixel value, got: \"{}{}\"", input, metric),
    ValueParseErr(err, number_str) => format!("Could not parse \"{}\" as floating-point value: \"{}\"", number_str, err),
    InvalidPixelValue(s) => format!("Invalid pixel value: \"{}\"", s),
}}

/// Wrapper for `NoValueGiven` error in pixel value parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct PixelNoValueGivenError {
    pub value: AzString,
    pub metric: SizeMetric,
}

/// Owned version of `CssPixelValueParseError`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum CssPixelValueParseErrorOwned {
    EmptyString,
    NoValueGiven(PixelNoValueGivenError),
    ValueParseErr(ParseFloatErrorWithInput),
    InvalidPixelValue(AzString),
}

impl CssPixelValueParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> CssPixelValueParseErrorOwned {
        match self {
            CssPixelValueParseError::EmptyString => CssPixelValueParseErrorOwned::EmptyString,
            CssPixelValueParseError::NoValueGiven(s, metric) => {
                CssPixelValueParseErrorOwned::NoValueGiven(PixelNoValueGivenError { value: (*s).to_string().into(), metric: *metric })
            }
            CssPixelValueParseError::ValueParseErr(err, s) => {
                CssPixelValueParseErrorOwned::ValueParseErr(ParseFloatErrorWithInput { error: err.clone().into(), input: (*s).to_string().into() })
            }
            CssPixelValueParseError::InvalidPixelValue(s) => {
                CssPixelValueParseErrorOwned::InvalidPixelValue((*s).to_string().into())
            }
        }
    }
}

impl CssPixelValueParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> CssPixelValueParseError<'_> {
        match self {
            Self::EmptyString => CssPixelValueParseError::EmptyString,
            Self::NoValueGiven(e) => {
                CssPixelValueParseError::NoValueGiven(e.value.as_str(), e.metric)
            }
            Self::ValueParseErr(e) => {
                CssPixelValueParseError::ValueParseErr(e.error.to_std(), e.input.as_str())
            }
            Self::InvalidPixelValue(s) => {
                CssPixelValueParseError::InvalidPixelValue(s.as_str())
            }
        }
    }
}

/// parses an angle value like `30deg`, `1.64rad`, `100%`, etc.
fn parse_pixel_value_inner<'a>(
    input: &'a str,
    match_values: &[(&'static str, SizeMetric)],
) -> Result<PixelValue, CssPixelValueParseError<'a>> {
    let input = input.trim();

    if input.is_empty() {
        return Err(CssPixelValueParseError::EmptyString);
    }

    for (match_val, metric) in match_values {
        if let Some(value) = input.strip_suffix(match_val) {
            let value = value.trim();
            if value.is_empty() {
                return Err(CssPixelValueParseError::NoValueGiven(input, *metric));
            }
            match value.parse::<f32>() {
                Ok(o) => {
                    return Ok(PixelValue::from_metric(*metric, o));
                }
                Err(e) => {
                    return Err(CssPixelValueParseError::ValueParseErr(e, value));
                }
            }
        }
    }

    input.trim().parse::<f32>().map_or_else(
        |_| Err(CssPixelValueParseError::InvalidPixelValue(input)),
        |o| Ok(PixelValue::px(o)),
    )
}

/// # Errors
///
/// Returns an error if `input` is not a valid CSS `pixel-value` value.
pub fn parse_pixel_value(input: &str) -> Result<PixelValue, CssPixelValueParseError<'_>> {
    parse_pixel_value_inner(
        input,
        &[
            ("px", SizeMetric::Px),
            ("rem", SizeMetric::Rem), // Must be before "em" to match correctly
            ("em", SizeMetric::Em),
            ("pt", SizeMetric::Pt),
            ("in", SizeMetric::In),
            ("mm", SizeMetric::Mm),
            ("cm", SizeMetric::Cm),
            ("vmax", SizeMetric::Vmax), // Must be before "vw" to match correctly
            ("vmin", SizeMetric::Vmin), // Must be before "vw" to match correctly
            ("vw", SizeMetric::Vw),
            ("vh", SizeMetric::Vh),
            ("%", SizeMetric::Percent),
        ],
    )
}

/// # Errors
///
/// Returns an error if `input` is not a valid CSS `pixel-value-no-percent` value.
pub fn parse_pixel_value_no_percent(
    input: &str,
) -> Result<PixelValueNoPercent, CssPixelValueParseError<'_>> {
    Ok(PixelValueNoPercent {
        inner: parse_pixel_value_inner(
            input,
            &[
                ("px", SizeMetric::Px),
                ("rem", SizeMetric::Rem), // Must be before "em" to match correctly
                ("em", SizeMetric::Em),
                ("pt", SizeMetric::Pt),
                ("in", SizeMetric::In),
                ("mm", SizeMetric::Mm),
                ("cm", SizeMetric::Cm),
                ("vmax", SizeMetric::Vmax), // Must be before "vw" to match correctly
                ("vmin", SizeMetric::Vmin), // Must be before "vw" to match correctly
                ("vw", SizeMetric::Vw),
                ("vh", SizeMetric::Vh),
            ],
        )?,
    })
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PixelValueWithAuto {
    None,
    Initial,
    Inherit,
    Auto,
    Exact(PixelValue),
}

/// Parses a pixel value, but also tries values like "auto", "initial", "inherit" and "none"
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `pixel-value-with-auto` value.
pub fn parse_pixel_value_with_auto(
    input: &str,
) -> Result<PixelValueWithAuto, CssPixelValueParseError<'_>> {
    let input = input.trim();
    match input {
        "none" => Ok(PixelValueWithAuto::None),
        "initial" => Ok(PixelValueWithAuto::Initial),
        "inherit" => Ok(PixelValueWithAuto::Inherit),
        "auto" => Ok(PixelValueWithAuto::Auto),
        e => Ok(PixelValueWithAuto::Exact(parse_pixel_value(e)?)),
    }
}

// ============================================================================
// System Metric References (system:button-padding, system:button-radius, etc.)
// ============================================================================

/// Reference to a specific system metric value.
/// These are resolved at runtime based on the user's system preferences.
/// 
/// CSS syntax: `system:button-padding`, `system:button-radius`, `system:titlebar-height`, etc.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum SystemMetricRef {
    /// Button corner radius (system:button-radius)
    #[default]
    ButtonRadius,
    /// Button horizontal padding (system:button-padding-horizontal)
    ButtonPaddingHorizontal,
    /// Button vertical padding (system:button-padding-vertical)
    ButtonPaddingVertical,
    /// Button border width (system:button-border-width)
    ButtonBorderWidth,
    /// Titlebar height (system:titlebar-height)
    TitlebarHeight,
    /// Titlebar button area width (system:titlebar-button-width)
    TitlebarButtonWidth,
    /// Titlebar horizontal padding (system:titlebar-padding)
    TitlebarPadding,
    /// Safe area top inset for notched devices (system:safe-area-top)
    SafeAreaTop,
    /// Safe area bottom inset (system:safe-area-bottom)
    SafeAreaBottom,
    /// Safe area left inset (system:safe-area-left)
    SafeAreaLeft,
    /// Safe area right inset (system:safe-area-right)
    SafeAreaRight,
}


impl SystemMetricRef {
    /// Resolve this system metric reference against actual system metrics.
    #[must_use] pub const fn resolve(&self, metrics: &crate::system::SystemMetrics) -> Option<PixelValue> {
        match self {
            Self::ButtonRadius => metrics.corner_radius.as_option().copied(),
            Self::ButtonPaddingHorizontal => metrics.button_padding_horizontal.as_option().copied(),
            Self::ButtonPaddingVertical => metrics.button_padding_vertical.as_option().copied(),
            Self::ButtonBorderWidth => metrics.border_width.as_option().copied(),
            Self::TitlebarHeight => metrics.titlebar.height.as_option().copied(),
            Self::TitlebarButtonWidth => metrics.titlebar.button_area_width.as_option().copied(),
            Self::TitlebarPadding => metrics.titlebar.padding_horizontal.as_option().copied(),
            Self::SafeAreaTop => metrics.titlebar.safe_area.top.as_option().copied(),
            Self::SafeAreaBottom => metrics.titlebar.safe_area.bottom.as_option().copied(),
            Self::SafeAreaLeft => metrics.titlebar.safe_area.left.as_option().copied(),
            Self::SafeAreaRight => metrics.titlebar.safe_area.right.as_option().copied(),
        }
    }

    /// Returns the CSS string representation of this system metric reference.
    #[must_use] pub const fn as_css_str(&self) -> &'static str {
        match self {
            Self::ButtonRadius => "system:button-radius",
            Self::ButtonPaddingHorizontal => "system:button-padding-horizontal",
            Self::ButtonPaddingVertical => "system:button-padding-vertical",
            Self::ButtonBorderWidth => "system:button-border-width",
            Self::TitlebarHeight => "system:titlebar-height",
            Self::TitlebarButtonWidth => "system:titlebar-button-width",
            Self::TitlebarPadding => "system:titlebar-padding",
            Self::SafeAreaTop => "system:safe-area-top",
            Self::SafeAreaBottom => "system:safe-area-bottom",
            Self::SafeAreaLeft => "system:safe-area-left",
            Self::SafeAreaRight => "system:safe-area-right",
        }
    }

    /// Parse a system metric reference from a CSS string (without the "system:" prefix).
    #[must_use] pub fn from_css_str(s: &str) -> Option<Self> {
        match s {
            "button-radius" => Some(Self::ButtonRadius),
            "button-padding-horizontal" => Some(Self::ButtonPaddingHorizontal),
            "button-padding-vertical" => Some(Self::ButtonPaddingVertical),
            "button-border-width" => Some(Self::ButtonBorderWidth),
            "titlebar-height" => Some(Self::TitlebarHeight),
            "titlebar-button-width" => Some(Self::TitlebarButtonWidth),
            "titlebar-padding" => Some(Self::TitlebarPadding),
            "safe-area-top" => Some(Self::SafeAreaTop),
            "safe-area-bottom" => Some(Self::SafeAreaBottom),
            "safe-area-left" => Some(Self::SafeAreaLeft),
            "safe-area-right" => Some(Self::SafeAreaRight),
            _ => None,
        }
    }
}

impl fmt::Display for SystemMetricRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_css_str())
    }
}

impl FormatAsCssValue for SystemMetricRef {
    fn format_as_css_value(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_css_str())
    }
}

/// A pixel value reference that can be either a concrete value or a system metric.
/// System metrics are lazily evaluated at runtime based on the user's system theme.
/// 
/// CSS syntax: `10px`, `1.5em`, `system:button-padding`, etc.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd)]
#[repr(C, u8)]
pub enum PixelValueOrSystem {
    /// A concrete pixel value.
    Value(PixelValue),
    /// A reference to a system metric, resolved at runtime.
    System(SystemMetricRef),
}

impl Default for PixelValueOrSystem {
    fn default() -> Self {
        Self::Value(PixelValue::zero())
    }
}

impl From<PixelValue> for PixelValueOrSystem {
    fn from(value: PixelValue) -> Self {
        Self::Value(value)
    }
}

impl PixelValueOrSystem {
    /// Create a new `PixelValueOrSystem` from a concrete value.
    #[must_use] pub const fn value(v: PixelValue) -> Self {
        Self::Value(v)
    }
    
    /// Create a new `PixelValueOrSystem` from a system metric reference.
    #[must_use] pub const fn system(s: SystemMetricRef) -> Self {
        Self::System(s)
    }
    
    /// Resolve the pixel value against a `SystemMetrics` struct.
    /// Returns the system metric if available, or falls back to the provided default.
    #[must_use] pub fn resolve(&self, system_metrics: &crate::system::SystemMetrics, fallback: PixelValue) -> PixelValue {
        match self {
            Self::Value(v) => *v,
            Self::System(ref_type) => ref_type.resolve(system_metrics).unwrap_or(fallback),
        }
    }
    
}

impl fmt::Display for PixelValueOrSystem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Value(v) => write!(f, "{v}"),
            Self::System(s) => write!(f, "{s}"),
        }
    }
}

impl FormatAsCssValue for PixelValueOrSystem {
    fn format_as_css_value(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Value(v) => v.format_as_css_value(f),
            Self::System(s) => s.format_as_css_value(f),
        }
    }
}

/// Parse a pixel value that may include system metric references.
/// 
/// Accepts: `10px`, `1.5em`, `system:button-padding`, etc.
#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `pixel-value-or-system` value.
pub fn parse_pixel_value_or_system(
    input: &str,
) -> Result<PixelValueOrSystem, CssPixelValueParseError<'_>> {
    let input = input.trim();
    
    // Check for system metric reference
    if let Some(metric_name) = input.strip_prefix("system:") {
        if let Some(metric_ref) = SystemMetricRef::from_css_str(metric_name) {
            return Ok(PixelValueOrSystem::System(metric_ref));
        }
        return Err(CssPixelValueParseError::InvalidPixelValue(input));
    }
    
    // Parse as regular pixel value
    Ok(PixelValueOrSystem::Value(parse_pixel_value(input)?))
}

#[cfg(all(test, feature = "parser"))]
mod tests {
    // Tests assert that parsed values equal the exact source literals.
    #![allow(clippy::float_cmp)]
    use super::*;

    #[test]
    fn test_parse_pixel_value() {
        assert_eq!(parse_pixel_value("10px").unwrap(), PixelValue::px(10.0));
        assert_eq!(parse_pixel_value("1.5em").unwrap(), PixelValue::em(1.5));
        assert_eq!(parse_pixel_value("2rem").unwrap(), PixelValue::rem(2.0));
        assert_eq!(parse_pixel_value("-20pt").unwrap(), PixelValue::pt(-20.0));
        assert_eq!(parse_pixel_value("50%").unwrap(), PixelValue::percent(50.0));
        assert_eq!(parse_pixel_value("1in").unwrap(), PixelValue::inch(1.0));
        assert_eq!(parse_pixel_value("2.54cm").unwrap(), PixelValue::cm(2.54));
        assert_eq!(parse_pixel_value("10mm").unwrap(), PixelValue::mm(10.0));
        assert_eq!(parse_pixel_value("  0  ").unwrap(), PixelValue::px(0.0));
    }

    #[test]
    fn test_resolve_with_context_em() {
        // Element has font-size: 32px, margin: 0.67em
        let context = ResolutionContext {
            element_font_size: 32.0,
            parent_font_size: 16.0,
            ..Default::default()
        };

        // Margin em uses element's own font-size
        let margin = PixelValue::em(0.67);
        assert!(
            (margin.resolve_with_context(&context, PropertyContext::Margin) - 21.44).abs() < 0.01
        );

        // Font-size em uses parent's font-size
        let font_size = PixelValue::em(2.0);
        assert_eq!(
            font_size.resolve_with_context(&context, PropertyContext::FontSize),
            32.0
        );
    }

    #[test]
    fn test_resolve_with_context_rem() {
        // Root has font-size: 18px
        let context = ResolutionContext {
            element_font_size: 32.0,
            parent_font_size: 16.0,
            root_font_size: 18.0,
            ..Default::default()
        };

        // Rem always uses root font-size, regardless of property
        let margin = PixelValue::rem(2.0);
        assert_eq!(
            margin.resolve_with_context(&context, PropertyContext::Margin),
            36.0
        );

        let font_size = PixelValue::rem(1.5);
        assert_eq!(
            font_size.resolve_with_context(&context, PropertyContext::FontSize),
            27.0
        );
    }

    #[test]
    fn test_resolve_with_context_percent_margin() {
        // Margin % uses containing block WIDTH (even for top/bottom!)
        let context = ResolutionContext {
            element_font_size: 16.0,
            parent_font_size: 16.0,
            root_font_size: 16.0,
            containing_block_size: PhysicalSize::new(800.0, 600.0),
            element_size: None,
            viewport_size: PhysicalSize::new(1920.0, 1080.0),
        };

        let margin = PixelValue::percent(10.0); // 10%
        assert_eq!(
            margin.resolve_with_context(&context, PropertyContext::Margin),
            80.0
        ); // 10% of 800
    }

    #[test]
    fn test_parse_pixel_value_no_percent() {
        assert_eq!(
            parse_pixel_value_no_percent("10px").unwrap().inner,
            PixelValue::px(10.0)
        );
        assert!(parse_pixel_value_no_percent("50%").is_err());
    }

    #[test]
    fn test_parse_pixel_value_with_auto() {
        assert_eq!(
            parse_pixel_value_with_auto("10px").unwrap(),
            PixelValueWithAuto::Exact(PixelValue::px(10.0))
        );
        assert_eq!(
            parse_pixel_value_with_auto("auto").unwrap(),
            PixelValueWithAuto::Auto
        );
        assert_eq!(
            parse_pixel_value_with_auto("initial").unwrap(),
            PixelValueWithAuto::Initial
        );
        assert_eq!(
            parse_pixel_value_with_auto("inherit").unwrap(),
            PixelValueWithAuto::Inherit
        );
        assert_eq!(
            parse_pixel_value_with_auto("none").unwrap(),
            PixelValueWithAuto::None
        );
    }

    #[test]
    fn test_parse_pixel_value_errors() {
        assert!(parse_pixel_value("").is_err());
        // Modern CSS parsers can be liberal - unitless numbers treated as px
        assert!(parse_pixel_value("10").is_ok()); // Parsed as 10px
                                                  // This parser is liberal and trims whitespace, so "10 px" is accepted
        assert!(parse_pixel_value("10 px").is_ok()); // Liberal parsing accepts this
        assert!(parse_pixel_value("px").is_err());
        assert!(parse_pixel_value("ten-px").is_err());
    }
}

#[cfg(test)]
#[allow(
    clippy::float_cmp,
    clippy::unreadable_literal,
    clippy::cast_precision_loss,
    clippy::too_many_lines,
    clippy::excessive_precision
)]
mod autotest_generated {
    use std::{
        collections::hash_map::DefaultHasher,
        hash::{Hash, Hasher},
    };

    use super::*;
    use crate::{
        codegen::format::FormatAsRustCode,
        css::PrintAsCssValue,
        props::{
            basic::length::{FloatValue, SizeMetric},
            formatter::FormatAsCssValue,
        },
        system::{SafeAreaInsets, SystemMetrics, TitlebarMetrics},
    };

    /// `FloatValue` stores `f32 * 1000` truncated into an `isize`, so every value
    /// is quantized to 1/1000 and every `get()` is finite by construction.
    const MULT: f32 = 1000.0;

    /// `const_new` multiplies by 1000 in `isize` space, so anything beyond this
    /// overflows the multiply (debug-panics / release-wraps). The `const_*`
    /// constructors are only usable up to here.
    const MAX_SAFE_CONST: isize = isize::MAX / 1000;
    const MIN_SAFE_CONST: isize = isize::MIN / 1000;

    const ALL_METRICS: [SizeMetric; 12] = [
        SizeMetric::Px,
        SizeMetric::Pt,
        SizeMetric::Em,
        SizeMetric::Rem,
        SizeMetric::In,
        SizeMetric::Cm,
        SizeMetric::Mm,
        SizeMetric::Percent,
        SizeMetric::Vw,
        SizeMetric::Vh,
        SizeMetric::Vmin,
        SizeMetric::Vmax,
    ];

    const ALL_PROPERTY_CONTEXTS: [PropertyContext; 9] = [
        PropertyContext::FontSize,
        PropertyContext::Margin,
        PropertyContext::Padding,
        PropertyContext::Width,
        PropertyContext::Height,
        PropertyContext::BorderWidth,
        PropertyContext::BorderRadius,
        PropertyContext::Transform,
        PropertyContext::Other,
    ];

    const ALL_SYSTEM_REFS: [SystemMetricRef; 11] = [
        SystemMetricRef::ButtonRadius,
        SystemMetricRef::ButtonPaddingHorizontal,
        SystemMetricRef::ButtonPaddingVertical,
        SystemMetricRef::ButtonBorderWidth,
        SystemMetricRef::TitlebarHeight,
        SystemMetricRef::TitlebarButtonWidth,
        SystemMetricRef::TitlebarPadding,
        SystemMetricRef::SafeAreaTop,
        SystemMetricRef::SafeAreaBottom,
        SystemMetricRef::SafeAreaLeft,
        SystemMetricRef::SafeAreaRight,
    ];

    /// The values that historically break fixed-point encoders.
    const EXTREME_F32: [f32; 13] = [
        0.0,
        -0.0,
        1.0,
        -1.0,
        f32::MIN_POSITIVE,
        -f32::MIN_POSITIVE,
        1e30,
        -1e30,
        f32::MAX,
        f32::MIN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NAN,
    ];

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.001
    }

    fn hash_of<T: Hash>(v: &T) -> u64 {
        let mut h = DefaultHasher::new();
        v.hash(&mut h);
        h.finish()
    }

    /// Renders anything through the `FormatAsCssValue` impl, which is otherwise
    /// only reachable with a live `Formatter`.
    struct CssVal<T>(T);

    impl<T: FormatAsCssValue> core::fmt::Display for CssVal<T> {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            self.0.format_as_css_value(f)
        }
    }

    fn as_css_value<T: FormatAsCssValue>(v: T) -> String {
        CssVal(v).to_string()
    }

    /// A context whose every reference value is distinct, so a resolver that
    /// reads the wrong field cannot accidentally produce the right number.
    fn distinct_context() -> ResolutionContext {
        ResolutionContext {
            element_font_size: 32.0,
            parent_font_size: 8.0,
            root_font_size: 4.0,
            containing_block_size: PhysicalSize::new(800.0, 600.0),
            element_size: Some(PhysicalSize::new(200.0, 100.0)),
            viewport_size: PhysicalSize::new(1000.0, 500.0),
        }
    }

    fn populated_metrics() -> SystemMetrics {
        SystemMetrics {
            corner_radius: OptionPixelValue::Some(PixelValue::px(1.0)),
            border_width: OptionPixelValue::Some(PixelValue::px(2.0)),
            button_padding_horizontal: OptionPixelValue::Some(PixelValue::px(3.0)),
            button_padding_vertical: OptionPixelValue::Some(PixelValue::px(4.0)),
            titlebar: TitlebarMetrics {
                height: OptionPixelValue::Some(PixelValue::px(5.0)),
                button_area_width: OptionPixelValue::Some(PixelValue::px(6.0)),
                padding_horizontal: OptionPixelValue::Some(PixelValue::px(7.0)),
                safe_area: SafeAreaInsets {
                    top: OptionPixelValue::Some(PixelValue::px(8.0)),
                    bottom: OptionPixelValue::Some(PixelValue::px(9.0)),
                    left: OptionPixelValue::Some(PixelValue::px(10.0)),
                    right: OptionPixelValue::Some(PixelValue::px(11.0)),
                },
                ..TitlebarMetrics::default()
            },
        }
    }

    // ============================================================== parsers ===

    #[test]
    fn parse_pixel_value_rejects_empty_and_whitespace_only() {
        assert_eq!(
            parse_pixel_value("").unwrap_err(),
            CssPixelValueParseError::EmptyString
        );
        for ws in ["   ", "\t\n", "\r\n\t ", "\n"] {
            assert_eq!(
                parse_pixel_value(ws).unwrap_err(),
                CssPixelValueParseError::EmptyString,
                "whitespace-only input {ws:?} must trim down to EmptyString"
            );
        }
    }

    #[test]
    fn parse_pixel_value_rejects_a_bare_unit_with_no_number() {
        // Every suffix that is reachable as a bare token must report NoValueGiven
        // (i.e. "the unit is fine, the number is missing") rather than panicking.
        // "vmin" is deliberately absent — see the vmin-shadowing test below.
        for unit in [
            "px", "rem", "em", "pt", "in", "mm", "cm", "vmax", "vw", "vh", "%",
        ] {
            let err = parse_pixel_value(unit).unwrap_err();
            assert!(
                matches!(err, CssPixelValueParseError::NoValueGiven(input, _) if input == unit),
                "bare unit {unit:?} should be NoValueGiven, got {err:?}"
            );
        }
        // Whitespace between the (missing) number and the unit is trimmed too.
        assert!(matches!(
            parse_pixel_value("   px").unwrap_err(),
            CssPixelValueParseError::NoValueGiven("px", SizeMetric::Px)
        ));
    }

    #[test]
    fn parse_pixel_value_vmin_is_shadowed_by_the_in_suffix() {
        // BUG (real defect, characterized here so the suite stays green): the
        // suffix table in `parse_pixel_value` tests "in" BEFORE "vmin", so
        // "5vmin" strips the "in" and then fails to parse the "5vm" remainder.
        // Every vmin length in a stylesheet is therefore rejected. "vmax" is
        // unaffected: no earlier entry is a suffix of it.
        let err = parse_pixel_value("5vmin").unwrap_err();
        assert!(
            matches!(
                err,
                CssPixelValueParseError::ValueParseErr(_, remainder) if remainder == "5vm"
            ),
            "expected the (buggy) 'in'-suffix strip, got {err:?}"
        );
        // Same shadowing for the bare unit: "vmin" -> "vm" -> parse error,
        // instead of the NoValueGiven that every other bare unit reports.
        assert!(matches!(
            parse_pixel_value("vmin").unwrap_err(),
            CssPixelValueParseError::ValueParseErr(_, "vm")
        ));

        // The sibling viewport units are all fine.
        assert_eq!(
            parse_pixel_value("5vmax").unwrap(),
            PixelValue::from_metric(SizeMetric::Vmax, 5.0)
        );
        assert_eq!(
            parse_pixel_value("5vw").unwrap(),
            PixelValue::from_metric(SizeMetric::Vw, 5.0)
        );
        assert_eq!(
            parse_pixel_value("5vh").unwrap(),
            PixelValue::from_metric(SizeMetric::Vh, 5.0)
        );
    }

    #[test]
    fn parse_pixel_value_inner_proves_the_vmin_bug_is_pure_suffix_ordering() {
        // Same input, same two units, only the table order differs. This pins the
        // fix: move "vmin"/"vmax" ahead of "in" (and "em"/"rem" style ordering).
        let in_first: [(&'static str, SizeMetric); 2] =
            [("in", SizeMetric::In), ("vmin", SizeMetric::Vmin)];
        let vmin_first: [(&'static str, SizeMetric); 2] =
            [("vmin", SizeMetric::Vmin), ("in", SizeMetric::In)];

        assert!(parse_pixel_value_inner("5vmin", &in_first).is_err());
        assert_eq!(
            parse_pixel_value_inner("5vmin", &vmin_first).unwrap(),
            PixelValue::from_metric(SizeMetric::Vmin, 5.0)
        );
        // ...and the reordering does not break plain inches.
        assert_eq!(
            parse_pixel_value_inner("5in", &vmin_first).unwrap(),
            PixelValue::inch(5.0)
        );
    }

    #[test]
    fn parse_pixel_value_inner_with_an_empty_table_falls_back_to_unitless_px() {
        let empty: [(&'static str, SizeMetric); 0] = [];

        // No suffix table -> only a bare float is acceptable, and it means px.
        assert_eq!(
            parse_pixel_value_inner("10", &empty).unwrap(),
            PixelValue::px(10.0)
        );
        assert!(matches!(
            parse_pixel_value_inner("10px", &empty).unwrap_err(),
            CssPixelValueParseError::InvalidPixelValue("10px")
        ));
        assert_eq!(
            parse_pixel_value_inner("", &empty).unwrap_err(),
            CssPixelValueParseError::EmptyString
        );
    }

    #[test]
    fn parse_pixel_value_accepts_every_unit_it_advertises() {
        // Positive controls, including the liberal shapes this parser allows.
        let cases: [(&str, PixelValue); 12] = [
            ("10px", PixelValue::px(10.0)),
            ("1.5em", PixelValue::em(1.5)),
            ("2rem", PixelValue::rem(2.0)),
            ("-20pt", PixelValue::pt(-20.0)),
            ("50%", PixelValue::percent(50.0)),
            ("1in", PixelValue::inch(1.0)),
            ("2.54cm", PixelValue::cm(2.54)),
            ("10mm", PixelValue::mm(10.0)),
            ("+7px", PixelValue::px(7.0)),
            (".5px", PixelValue::px(0.5)),
            ("5.px", PixelValue::px(5.0)),
            ("1e2px", PixelValue::px(100.0)),
        ];
        for (input, expected) in cases {
            assert_eq!(
                parse_pixel_value(input).unwrap(),
                expected,
                "parsing {input:?}"
            );
        }

        // Unitless numbers mean px, and interior/exterior whitespace is trimmed.
        assert_eq!(parse_pixel_value("  0  ").unwrap(), PixelValue::px(0.0));
        assert_eq!(parse_pixel_value("10 px").unwrap(), PixelValue::px(10.0));
        assert_eq!(parse_pixel_value("\t10px\n").unwrap(), PixelValue::px(10.0));
    }

    #[test]
    fn parse_pixel_value_boundary_numbers_saturate_instead_of_overflowing() {
        // Signed zero collapses onto a single encoding.
        assert_eq!(parse_pixel_value("-0").unwrap(), PixelValue::px(0.0));
        assert_eq!(parse_pixel_value("-0").unwrap(), PixelValue::zero());

        // Anything under 1/1000 of a unit quantizes away to exactly zero.
        assert_eq!(parse_pixel_value("0.0004px").unwrap(), PixelValue::px(0.0));
        assert_eq!(parse_pixel_value("-0.0009px").unwrap(), PixelValue::px(0.0));
        assert_eq!(parse_pixel_value("1e-40px").unwrap(), PixelValue::px(0.0));

        // Values far past f32/isize range saturate; `get()` stays finite.
        for huge in ["9223372036854775807", "1e40px", "3.5e38"] {
            let v = parse_pixel_value(huge).unwrap();
            assert!(
                v.number.get().is_finite(),
                "{huge:?} leaked a non-finite value: {}",
                v.number.get()
            );
            assert!(v.number.get() > 0.0, "{huge:?} lost its sign");
        }
        let neg = parse_pixel_value("-1e40px").unwrap();
        assert!(neg.number.get().is_finite() && neg.number.get() < 0.0);
    }

    #[test]
    fn parse_pixel_value_inherits_rusts_float_keywords() {
        // BUG-adjacent (spec conformance, characterized): `str::parse::<f32>`
        // accepts "NaN"/"infinity", so CSS that no browser would accept is taken
        // here. NaN sanitizes to 0px and infinity saturates, so nothing downstream
        // sees a non-finite length -- but neither input should have parsed at all.
        assert_eq!(parse_pixel_value("NaN").unwrap(), PixelValue::zero());

        let inf = parse_pixel_value("infinity").unwrap();
        assert_eq!(inf, PixelValue::px(f32::INFINITY));
        assert!(inf.number.get().is_finite() && inf.number.get() > 0.0);

        let neg_inf = parse_pixel_value("-infinity").unwrap();
        assert_eq!(neg_inf, PixelValue::px(f32::NEG_INFINITY));
        assert!(neg_inf.number.get().is_finite() && neg_inf.number.get() < 0.0);

        // "inf" is accepted too. The "in" (inches) suffix does NOT eat it: a suffix
        // match needs the string to END in "in", and "inf" ends in "nf" -- so it
        // falls through to the same `str::parse::<f32>()` path as "infinity" above.
        let inf_short = parse_pixel_value("inf").unwrap();
        assert_eq!(inf_short, PixelValue::px(f32::INFINITY));
        assert!(inf_short.number.get().is_finite() && inf_short.number.get() > 0.0);
    }

    #[test]
    fn parse_pixel_value_is_case_sensitive_about_units() {
        // Conformance gap (characterized): CSS units are case-insensitive
        // ("10PX" is valid CSS), but the suffix table only matches lowercase, so
        // these fall through to the float parser and are rejected outright.
        for input in ["10PX", "10Px", "10EM", "10REM", "10VMAX"] {
            let err = parse_pixel_value(input).unwrap_err();
            assert!(
                matches!(err, CssPixelValueParseError::InvalidPixelValue(s) if s == input),
                "uppercase unit {input:?} should be InvalidPixelValue, got {err:?}"
            );
        }
    }

    #[test]
    fn parse_pixel_value_rejects_garbage_and_trailing_junk() {
        for input in [
            "ten-px",
            "px10",
            "10px;garbage",
            "10;",
            "--",
            "1%%",
            "10 20px",
            "#",
            "10px 10px",
            "e",
            "0x10px",
        ] {
            assert!(
                parse_pixel_value(input).is_err(),
                "{input:?} must not parse, got {:?}",
                parse_pixel_value(input)
            );
        }
    }

    #[test]
    fn parse_pixel_value_survives_unicode() {
        // Multibyte input must never slice mid-codepoint or panic.
        for input in [
            "\u{1F600}",             // emoji alone
            "10px\u{1F600}",         // emoji suffix
            "10px\u{0301}",          // combining acute after the unit
            "\u{200B}10px",          // zero-width space (NOT trimmable whitespace)
            "\u{0661}\u{0660}px",    // arabic-indic digits
            "10\u{0440}\u{0445}",    // cyrillic look-alike of "px"
            "\u{202E}10px",          // RTL override
        ] {
            let got = parse_pixel_value(input);
            assert!(got.is_err(), "{input:?} must be rejected, got {got:?}");
        }

        // The zero-width space specifically survives the trim and lands in the
        // reported remainder, which proves no byte-level slicing happened.
        assert!(matches!(
            parse_pixel_value("\u{200B}10px").unwrap_err(),
            CssPixelValueParseError::ValueParseErr(_, "\u{200B}10")
        ));
    }

    #[test]
    fn parse_pixel_value_handles_extremely_long_and_deeply_nested_input() {
        // 100k digits: must terminate quickly and saturate, not hang or overflow.
        let long_number = format!("{}px", "9".repeat(100_000));
        let parsed = parse_pixel_value(&long_number).unwrap();
        assert!(parsed.number.get().is_finite());
        assert_eq!(parsed.metric, SizeMetric::Px);

        // 100k junk bytes: rejected, no quadratic blow-up.
        let long_junk = "x".repeat(100_000);
        assert!(parse_pixel_value(&long_junk).is_err());

        // 10k nested brackets: this parser is not recursive, so this must be a
        // plain rejection rather than a stack overflow.
        let nested = "(".repeat(10_000);
        assert!(matches!(
            parse_pixel_value(&nested).unwrap_err(),
            CssPixelValueParseError::InvalidPixelValue(_)
        ));
    }

    #[test]
    fn parse_pixel_value_no_percent_rejects_percentages_but_keeps_the_rest() {
        assert_eq!(
            parse_pixel_value_no_percent("10px").unwrap().inner,
            PixelValue::px(10.0)
        );
        assert_eq!(
            parse_pixel_value_no_percent("5vmax").unwrap().inner,
            PixelValue::from_metric(SizeMetric::Vmax, 5.0)
        );

        // "%" is not in the table, so it falls through to the float parser.
        assert!(matches!(
            parse_pixel_value_no_percent("50%").unwrap_err(),
            CssPixelValueParseError::InvalidPixelValue("50%")
        ));
        assert!(matches!(
            parse_pixel_value_no_percent("%").unwrap_err(),
            CssPixelValueParseError::InvalidPixelValue("%")
        ));

        assert_eq!(
            parse_pixel_value_no_percent("").unwrap_err(),
            CssPixelValueParseError::EmptyString
        );
        assert_eq!(
            parse_pixel_value_no_percent("   ").unwrap_err(),
            CssPixelValueParseError::EmptyString
        );
        assert!(parse_pixel_value_no_percent("\u{1F600}").is_err());
        // Inherits the vmin shadowing bug from the shared inner parser.
        assert!(parse_pixel_value_no_percent("5vmin").is_err());
    }

    #[test]
    fn parse_pixel_value_with_auto_keywords_and_fallthrough() {
        assert_eq!(
            parse_pixel_value_with_auto("auto").unwrap(),
            PixelValueWithAuto::Auto
        );
        assert_eq!(
            parse_pixel_value_with_auto("  initial  ").unwrap(),
            PixelValueWithAuto::Initial
        );
        assert_eq!(
            parse_pixel_value_with_auto("\tinherit\n").unwrap(),
            PixelValueWithAuto::Inherit
        );
        assert_eq!(
            parse_pixel_value_with_auto("none").unwrap(),
            PixelValueWithAuto::None
        );
        assert_eq!(
            parse_pixel_value_with_auto("10px").unwrap(),
            PixelValueWithAuto::Exact(PixelValue::px(10.0))
        );

        // Keywords are matched case-sensitively (CSS says they should not be).
        for input in ["AUTO", "Auto", "INHERIT", "None"] {
            assert!(
                parse_pixel_value_with_auto(input).is_err(),
                "{input:?} unexpectedly matched a keyword"
            );
        }

        // Empty / junk / unicode all funnel into the pixel-value errors.
        assert_eq!(
            parse_pixel_value_with_auto("").unwrap_err(),
            CssPixelValueParseError::EmptyString
        );
        assert_eq!(
            parse_pixel_value_with_auto(" \t ").unwrap_err(),
            CssPixelValueParseError::EmptyString
        );
        assert!(parse_pixel_value_with_auto("auto;garbage").is_err());
        assert!(parse_pixel_value_with_auto("\u{1F600}").is_err());
        assert!(parse_pixel_value_with_auto(&"(".repeat(10_000)).is_err());
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_pixel_value_or_system_accepts_every_system_ref() {
        for r in ALL_SYSTEM_REFS {
            let css = r.as_css_str(); // already carries the "system:" prefix
            assert_eq!(
                parse_pixel_value_or_system(css).unwrap(),
                PixelValueOrSystem::System(r),
                "round-tripping {css:?}"
            );
            // Surrounding whitespace is trimmed before the prefix check.
            assert_eq!(
                parse_pixel_value_or_system(&format!("  {css}  ")).unwrap(),
                PixelValueOrSystem::System(r)
            );
        }

        // Plain lengths still work.
        assert_eq!(
            parse_pixel_value_or_system("10px").unwrap(),
            PixelValueOrSystem::Value(PixelValue::px(10.0))
        );
        assert_eq!(
            parse_pixel_value_or_system("1.5em").unwrap(),
            PixelValueOrSystem::Value(PixelValue::em(1.5))
        );
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_pixel_value_or_system_rejects_malformed_system_refs() {
        // DOC BUG (characterized): the doc comment on `parse_pixel_value_or_system`
        // and on `PixelValueOrSystem` advertises `system:button-padding`, but
        // `from_css_str` only knows the -horizontal / -vertical spellings, so the
        // documented example is rejected.
        assert!(matches!(
            parse_pixel_value_or_system("system:button-padding").unwrap_err(),
            CssPixelValueParseError::InvalidPixelValue("system:button-padding")
        ));

        for input in [
            "system:",                  // empty metric name
            "system:unknown",           // unknown metric
            "system: button-radius",    // no inner trim after the colon
            "system:BUTTON-RADIUS",     // case-sensitive
            "system:button-radius;x",   // trailing junk
            "system:\u{1F600}",         // unicode metric name
        ] {
            let err = parse_pixel_value_or_system(input).unwrap_err();
            assert!(
                matches!(err, CssPixelValueParseError::InvalidPixelValue(s) if s == input),
                "{input:?} should be InvalidPixelValue, got {err:?}"
            );
        }

        // Without the exact lowercase prefix it is treated as a length, and fails
        // as one.
        assert!(matches!(
            parse_pixel_value_or_system("SYSTEM:button-radius").unwrap_err(),
            CssPixelValueParseError::InvalidPixelValue("SYSTEM:button-radius")
        ));
        assert_eq!(
            parse_pixel_value_or_system("").unwrap_err(),
            CssPixelValueParseError::EmptyString
        );
        assert_eq!(
            parse_pixel_value_or_system("   ").unwrap_err(),
            CssPixelValueParseError::EmptyString
        );

        // A pathologically long metric name must be rejected, not hang.
        let long = format!("system:{}", "a".repeat(100_000));
        assert!(parse_pixel_value_or_system(&long).is_err());
    }

    // ============================================== parse-error round-trips ===

    #[test]
    fn parse_errors_survive_the_owned_round_trip() {
        let float_err = "x".parse::<f32>().unwrap_err();
        let errors = [
            CssPixelValueParseError::EmptyString,
            CssPixelValueParseError::NoValueGiven("px", SizeMetric::Px),
            CssPixelValueParseError::ValueParseErr(float_err, "abc"),
            CssPixelValueParseError::InvalidPixelValue("ten-px"),
        ];

        for err in errors {
            let owned = err.to_contained();
            let shared = owned.to_shared();
            assert_eq!(shared, err, "to_contained -> to_shared must be lossless");
            // Display must survive too, and never be empty.
            assert!(!err.to_string().is_empty());
            assert_eq!(shared.to_string(), err.to_string());
        }
    }

    #[test]
    fn parse_errors_round_trip_from_real_parse_failures() {
        // The same path, but with errors that actually came out of the parser
        // (including a unicode-bearing remainder).
        for input in ["", "px", "\u{200B}10px", "ten-px", "%"] {
            let err = parse_pixel_value(input).unwrap_err();
            let owned = err.to_contained();
            assert_eq!(owned.to_shared(), err, "round-trip failed for {input:?}");
        }
    }

    // ============================================================= numeric ===

    #[test]
    fn float_constructors_never_leak_a_non_finite_value() {
        // NaN must sanitize to 0 and infinities must saturate: `FloatValue`
        // decodes through an isize, so `get()` is finite for *every* input.
        type Ctor = (fn(f32) -> PixelValue, SizeMetric);
        let ctors: [Ctor; 8] = [
            (PixelValue::px, SizeMetric::Px),
            (PixelValue::em, SizeMetric::Em),
            (PixelValue::pt, SizeMetric::Pt),
            (PixelValue::inch, SizeMetric::In),
            (PixelValue::cm, SizeMetric::Cm),
            (PixelValue::mm, SizeMetric::Mm),
            (PixelValue::percent, SizeMetric::Percent),
            (PixelValue::rem, SizeMetric::Rem),
        ];

        for (ctor, metric) in ctors {
            for v in EXTREME_F32 {
                let px = ctor(v);
                assert_eq!(px.metric, metric, "constructor lost its metric for {v}");
                assert!(
                    px.number.get().is_finite(),
                    "{metric:?} constructor leaked a non-finite value for input {v}"
                );
            }
            assert_eq!(ctor(f32::NAN).number.get(), 0.0, "NaN must sanitize to 0");
            assert!(ctor(f32::INFINITY).number.get() > 0.0);
            assert!(ctor(f32::NEG_INFINITY).number.get() < 0.0);
        }

        // from_metric agrees with the named constructors for every metric.
        for metric in ALL_METRICS {
            for v in EXTREME_F32 {
                let px = PixelValue::from_metric(metric, v);
                assert_eq!(px.metric, metric);
                assert!(px.number.get().is_finite());
            }
            assert_eq!(
                PixelValue::from_metric(metric, 12.0),
                PixelValue {
                    metric,
                    number: FloatValue::new(12.0)
                }
            );
        }
    }

    #[test]
    fn float_constructors_quantize_to_one_thousandth() {
        // Sub-milli magnitudes vanish entirely...
        assert_eq!(PixelValue::px(0.0004).number.get(), 0.0);
        assert_eq!(PixelValue::px(-0.0009).number.get(), 0.0);
        // ...and the excess precision above 1/1000 is dropped, not rounded up.
        assert_eq!(PixelValue::px(1.0005).number.get(), 1.0);

        // Signed zero normalizes, which keeps Eq/Hash total (PixelValue is Eq +
        // Hash despite wrapping a float).
        assert_eq!(PixelValue::px(-0.0), PixelValue::px(0.0));
        assert_eq!(
            hash_of(&PixelValue::px(-0.0)),
            hash_of(&PixelValue::px(0.0))
        );
        // Even NaN is reflexive here, because it sanitizes to the zero encoding.
        assert_eq!(PixelValue::px(f32::NAN), PixelValue::px(f32::NAN));
        assert_eq!(PixelValue::px(f32::NAN), PixelValue::zero());
    }

    #[test]
    fn const_constructors_agree_with_their_float_twins() {
        assert_eq!(PixelValue::const_px(5), PixelValue::px(5.0));
        assert_eq!(PixelValue::const_em(5), PixelValue::em(5.0));
        assert_eq!(PixelValue::const_pt(5), PixelValue::pt(5.0));
        assert_eq!(PixelValue::const_percent(5), PixelValue::percent(5.0));
        assert_eq!(PixelValue::const_in(5), PixelValue::inch(5.0));
        assert_eq!(PixelValue::const_cm(5), PixelValue::cm(5.0));
        assert_eq!(PixelValue::const_mm(5), PixelValue::mm(5.0));

        assert_eq!(PixelValue::const_px(0), PixelValue::zero());
        assert_eq!(PixelValue::const_px(-7), PixelValue::px(-7.0));

        for metric in ALL_METRICS {
            assert_eq!(
                PixelValue::const_from_metric(metric, 7),
                PixelValue::from_metric(metric, 7.0),
                "const_from_metric disagrees with from_metric for {metric:?}"
            );
            assert_eq!(
                PixelValue::const_from_metric(metric, -7),
                PixelValue::from_metric(metric, -7.0)
            );
        }
    }

    #[test]
    fn const_constructors_are_usable_up_to_the_documented_isize_bound() {
        // `const_new` scales by 1000 in isize space, so MAX_SAFE_CONST is the
        // largest input that does not overflow the multiply. (Anything beyond it
        // -- e.g. `const_px(isize::MAX)` -- overflows: a debug panic and a
        // release wrap. Not exercised here because the two builds disagree.)
        for v in [0, 1, -1, MAX_SAFE_CONST, MIN_SAFE_CONST] {
            let px = PixelValue::const_px(v);
            assert!(
                px.number.get().is_finite(),
                "const_px({v}) leaked a non-finite value"
            );
        }
        assert!(PixelValue::const_px(MAX_SAFE_CONST).number.get() > 0.0);
        assert!(PixelValue::const_px(MIN_SAFE_CONST).number.get() < 0.0);
        assert_eq!(
            PixelValue::const_px(MAX_SAFE_CONST).number.number(),
            MAX_SAFE_CONST * 1000
        );
    }

    #[test]
    fn const_fractional_constructors_match_their_documented_examples() {
        // The doc-comment examples on `const_em_fractional`.
        assert!(approx(PixelValue::const_em_fractional(1, 5).number.get(), 1.5));
        assert!(approx(
            PixelValue::const_em_fractional(0, 83).number.get(),
            0.83
        ));
        assert!(approx(
            PixelValue::const_em_fractional(1, 17).number.get(),
            1.17
        ));
        assert_eq!(PixelValue::const_em_fractional(1, 5).metric, SizeMetric::Em);
        assert_eq!(PixelValue::const_pt_fractional(1, 5).metric, SizeMetric::Pt);
        assert!(approx(PixelValue::const_pt_fractional(2, 25).number.get(), 2.25));

        // Zero fraction, and the negative case (the sign must reach the fraction:
        // -1.5, not -1 + 0.5 = -0.5).
        assert_eq!(
            PixelValue::const_from_metric_fractional(SizeMetric::Px, 0, 0),
            PixelValue::zero()
        );
        assert!(approx(
            PixelValue::const_from_metric_fractional(SizeMetric::Px, -1, 5)
                .number
                .get(),
            -1.5
        ));

        // More than 3 decimals truncates to 3 (documented), rather than
        // overflowing the fixed-point encoding.
        assert!(approx(
            PixelValue::const_from_metric_fractional(SizeMetric::Px, 1, 5234)
                .number
                .get(),
            1.523
        ));

        // A pathological fraction must still land somewhere finite. (isize::MIN is
        // NOT exercised: negating it overflows inside the digit-counting code.)
        let extreme =
            PixelValue::const_from_metric_fractional(SizeMetric::Px, 0, isize::MAX);
        assert!(extreme.number.get().is_finite());
    }

    #[test]
    fn scale_for_dpi_is_defined_for_every_scale_factor() {
        let mut doubled = PixelValue::px(10.0);
        doubled.scale_for_dpi(2.0);
        assert_eq!(doubled, PixelValue::px(20.0));

        // Scaling compounds (it is not idempotent) -- worth pinning, since a
        // double-applied DPI scale is a classic layout bug.
        doubled.scale_for_dpi(2.0);
        assert_eq!(doubled, PixelValue::px(40.0));

        let mut zeroed = PixelValue::em(3.0);
        zeroed.scale_for_dpi(0.0);
        assert_eq!(zeroed, PixelValue::em(0.0));
        assert_eq!(zeroed.metric, SizeMetric::Em, "metric must be preserved");

        let mut flipped = PixelValue::px(10.0);
        flipped.scale_for_dpi(-1.5);
        assert_eq!(flipped, PixelValue::px(-15.0));

        // NaN collapses to zero, infinities saturate -- never a non-finite length.
        let mut nan_scaled = PixelValue::px(10.0);
        nan_scaled.scale_for_dpi(f32::NAN);
        assert_eq!(nan_scaled.number.get(), 0.0);

        let mut inf_scaled = PixelValue::px(10.0);
        inf_scaled.scale_for_dpi(f32::INFINITY);
        assert!(inf_scaled.number.get().is_finite() && inf_scaled.number.get() > 0.0);

        let mut max_scaled = PixelValue::px(f32::MAX);
        max_scaled.scale_for_dpi(f32::MAX);
        assert!(max_scaled.number.get().is_finite());

        // The no-percent wrapper just delegates.
        let mut wrapped = PixelValueNoPercent::from(PixelValue::px(10.0));
        wrapped.scale_for_dpi(2.5);
        assert_eq!(wrapped.inner, PixelValue::px(25.0));

        let mut wrapped_nan = PixelValueNoPercent::from(PixelValue::px(10.0));
        wrapped_nan.scale_for_dpi(f32::NAN);
        assert_eq!(wrapped_nan.inner.number.get(), 0.0);
    }

    #[test]
    fn interpolate_within_one_metric_keeps_that_metric() {
        let a = PixelValue::em(1.0);
        let b = PixelValue::em(3.0);

        assert_eq!(a.interpolate(&b, 0.0), a);
        assert_eq!(a.interpolate(&b, 1.0), b);
        assert_eq!(a.interpolate(&b, 0.5), PixelValue::em(2.0));

        // Out-of-range t extrapolates rather than clamping.
        assert_eq!(a.interpolate(&b, 2.0), PixelValue::em(5.0));
        assert_eq!(a.interpolate(&b, -1.0), PixelValue::em(-1.0));

        // Percent stays percent (it is NOT converted to px on the same-metric path).
        let p = PixelValue::percent(0.0).interpolate(&PixelValue::percent(100.0), 0.5);
        assert_eq!(p, PixelValue::percent(50.0));
        assert_eq!(p.metric, SizeMetric::Percent);

        // Non-finite t sanitizes to the zero encoding instead of poisoning layout.
        let nan_t = a.interpolate(&b, f32::NAN);
        assert_eq!(nan_t.number.get(), 0.0);
        assert_eq!(nan_t.metric, SizeMetric::Em);
        assert!(a.interpolate(&b, f32::INFINITY).number.get().is_finite());
    }

    #[test]
    fn interpolate_across_metrics_falls_back_to_px() {
        // Mixed metrics resolve through `to_pixels_internal` with DEFAULT_FONT_SIZE.
        let from_px = PixelValue::px(0.0);
        let to_em = PixelValue::em(1.0); // 16px at the default font size
        let mid = from_px.interpolate(&to_em, 0.5);
        assert_eq!(mid.metric, SizeMetric::Px);
        assert!(approx(mid.number.get(), DEFAULT_FONT_SIZE / 2.0));

        assert!(approx(
            PixelValue::px(0.0)
                .interpolate(&PixelValue::pt(72.0), 1.0)
                .number
                .get(),
            96.0
        ));

        // Percent and every viewport unit resolve to 0px on this path, because the
        // fallback has no containing block and no viewport. Documented as an
        // "acceptable animation fallback" -- pinned so it stays deliberate.
        for metric in [
            SizeMetric::Percent,
            SizeMetric::Vw,
            SizeMetric::Vh,
            SizeMetric::Vmin,
            SizeMetric::Vmax,
        ] {
            let other = PixelValue::from_metric(metric, 50.0);
            let done = PixelValue::px(100.0).interpolate(&other, 1.0);
            assert_eq!(
                done,
                PixelValue::px(0.0),
                "{metric:?} should collapse to 0px on the cross-metric path"
            );
        }

        let nan_t = PixelValue::px(0.0).interpolate(&PixelValue::em(1.0), f32::NAN);
        assert_eq!(nan_t.number.get(), 0.0);
    }

    #[test]
    fn to_pixels_internal_converts_every_absolute_and_relative_unit() {
        assert_eq!(PixelValue::px(10.0).to_pixels_internal(0.0, 16.0, 16.0), 10.0);
        assert!(approx(
            PixelValue::pt(72.0).to_pixels_internal(0.0, 16.0, 16.0),
            96.0
        ));
        assert!(approx(
            PixelValue::inch(1.0).to_pixels_internal(0.0, 16.0, 16.0),
            96.0
        ));
        assert!(approx(
            PixelValue::cm(2.54).to_pixels_internal(0.0, 16.0, 16.0),
            96.0
        ));
        assert!(approx(
            PixelValue::mm(25.4).to_pixels_internal(0.0, 16.0, 16.0),
            96.0
        ));
        assert_eq!(PT_TO_PX, 96.0 / 72.0);

        // em and rem read different resolves (this legacy path is the one that
        // historically conflated them, so pin that they are separate arguments).
        assert_eq!(PixelValue::em(2.0).to_pixels_internal(0.0, 10.0, 100.0), 20.0);
        assert_eq!(
            PixelValue::rem(2.0).to_pixels_internal(0.0, 10.0, 100.0),
            200.0
        );

        // % divides by 100 exactly once (the double-division bug this module's
        // NormalizedPercentage exists to prevent).
        assert_eq!(
            PixelValue::percent(50.0).to_pixels_internal(800.0, 16.0, 16.0),
            400.0
        );
        assert_eq!(
            PixelValue::percent(0.0).to_pixels_internal(800.0, 16.0, 16.0),
            0.0
        );
        assert_eq!(
            PixelValue::percent(-50.0).to_pixels_internal(800.0, 16.0, 16.0),
            -400.0
        );

        // Viewport units have no viewport here, so they are defined as 0 -- even
        // when the caller passes perfectly good resolves.
        for metric in [
            SizeMetric::Vw,
            SizeMetric::Vh,
            SizeMetric::Vmin,
            SizeMetric::Vmax,
        ] {
            assert_eq!(
                PixelValue::from_metric(metric, 50.0).to_pixels_internal(800.0, 16.0, 16.0),
                0.0,
                "{metric:?} must resolve to 0 on the legacy path"
            );
        }
    }

    #[test]
    fn to_pixels_internal_non_finite_resolves_are_defined_not_panics() {
        // The *result* of this method is a raw f32 (it is not re-encoded), so it
        // can go non-finite. Assert it does so predictably rather than panicking.
        assert!(PixelValue::em(1.0)
            .to_pixels_internal(0.0, f32::NAN, 0.0)
            .is_nan());
        assert!(PixelValue::rem(1.0)
            .to_pixels_internal(0.0, 0.0, f32::INFINITY)
            .is_infinite());
        assert!(PixelValue::percent(50.0)
            .to_pixels_internal(f32::INFINITY, 16.0, 16.0)
            .is_infinite());
        assert!(PixelValue::percent(50.0)
            .to_pixels_internal(f32::NAN, 16.0, 16.0)
            .is_nan());
        // 0% of an infinite containing block is NaN, not 0 -- worth knowing.
        assert!(PixelValue::percent(0.0)
            .to_pixels_internal(f32::INFINITY, 16.0, 16.0)
            .is_nan());

        // A saturated length times a huge resolve overflows to +inf (no panic).
        assert!(PixelValue::em(f32::MAX)
            .to_pixels_internal(0.0, f32::MAX, 0.0)
            .is_infinite());

        // Absolute units are always finite, whatever the resolves are.
        for v in EXTREME_F32 {
            assert!(PixelValue::px(v)
                .to_pixels_internal(f32::NAN, f32::NAN, f32::NAN)
                .is_finite());
        }
    }

    #[test]
    fn pixel_value_no_percent_to_pixels_internal_zeroes_out_percentages() {
        assert_eq!(
            PixelValueNoPercent::from(PixelValue::px(10.0)).to_pixels_internal(16.0, 16.0),
            10.0
        );
        assert_eq!(
            PixelValueNoPercent::from(PixelValue::em(2.0)).to_pixels_internal(10.0, 100.0),
            20.0
        );
        assert_eq!(
            PixelValueNoPercent::from(PixelValue::rem(2.0)).to_pixels_internal(10.0, 100.0),
            200.0
        );

        // The type forbids "%" at the *parser* level, but `From<PixelValue>` can
        // still smuggle one in. It resolves against a 0 containing block -> 0px.
        assert_eq!(
            PixelValueNoPercent::from(PixelValue::percent(50.0)).to_pixels_internal(16.0, 16.0),
            0.0
        );
        assert_eq!(PixelValueNoPercent::zero().to_pixels_internal(16.0, 16.0), 0.0);
        assert_eq!(PixelValueNoPercent::zero().inner, PixelValue::zero());
        assert_eq!(PixelValueNoPercent::default().inner, PixelValue::zero());
    }

    // =================================================== getters/predicates ===

    #[test]
    fn to_percent_is_some_only_for_the_percent_metric() {
        for metric in ALL_METRICS {
            let v = PixelValue::from_metric(metric, 50.0);
            if metric == SizeMetric::Percent {
                assert_eq!(v.to_percent().unwrap().get(), 0.5, "50% must normalize to 0.5");
            } else {
                assert!(
                    v.to_percent().is_none(),
                    "{metric:?} must not masquerade as a percentage"
                );
            }
        }

        // The returned percentage is already normalized: resolve() multiplies, it
        // must not divide by 100 a second time.
        assert_eq!(
            PixelValue::percent(50.0)
                .to_percent()
                .unwrap()
                .resolve(640.0),
            320.0
        );
        assert_eq!(
            PixelValue::percent(-50.0).to_percent().unwrap().get(),
            -0.5
        );
        assert_eq!(PixelValue::percent(0.0).to_percent().unwrap().get(), 0.0);
        // Extreme instances stay finite.
        assert!(PixelValue::percent(f32::MAX)
            .to_percent()
            .unwrap()
            .get()
            .is_finite());
        assert_eq!(
            PixelValue::percent(f32::NAN).to_percent().unwrap().get(),
            0.0
        );
    }

    #[test]
    fn normalized_percentage_new_and_from_unnormalized_disagree_by_100x() {
        // The whole point of the type: `new` takes 0.0-1.0, `from_unnormalized`
        // takes the CSS 0-100 scale.
        assert_eq!(NormalizedPercentage::new(0.5).get(), 0.5);
        assert_eq!(NormalizedPercentage::from_unnormalized(50.0).get(), 0.5);
        assert_eq!(NormalizedPercentage::from_unnormalized(0.0).get(), 0.0);
        assert_eq!(NormalizedPercentage::from_unnormalized(100.0).get(), 1.0);
        assert_eq!(NormalizedPercentage::from_unnormalized(-25.0).get(), -0.25);

        assert_eq!(NormalizedPercentage::new(0.5).resolve(640.0), 320.0);
        assert_eq!(NormalizedPercentage::new(0.0).resolve(640.0), 0.0);
        assert_eq!(NormalizedPercentage::new(1.0).resolve(f32::MAX), f32::MAX);
        assert_eq!(NormalizedPercentage::new(-1.0).resolve(100.0), -100.0);

        // Unlike PixelValue, this type is a raw f32 wrapper: it does NOT sanitize.
        // Non-finite in, non-finite out -- but never a panic.
        assert!(NormalizedPercentage::new(f32::NAN).get().is_nan());
        assert!(NormalizedPercentage::new(f32::NAN).resolve(100.0).is_nan());
        assert!(NormalizedPercentage::from_unnormalized(f32::INFINITY)
            .get()
            .is_infinite());
        assert!(NormalizedPercentage::new(1.0)
            .resolve(f32::INFINITY)
            .is_infinite());
        // 0 * inf is NaN, and this type will hand that straight to the layout.
        assert!(NormalizedPercentage::new(0.0)
            .resolve(f32::INFINITY)
            .is_nan());

        // Display renders back on the 0-100 scale.
        assert_eq!(NormalizedPercentage::new(0.5).to_string(), "50%");
        assert_eq!(NormalizedPercentage::new(0.0).to_string(), "0%");
        assert!(!NormalizedPercentage::new(f32::NAN).to_string().is_empty());
        assert!(!NormalizedPercentage::new(f32::INFINITY)
            .to_string()
            .is_empty());
    }

    #[test]
    fn resolve_with_context_reads_the_right_reference_for_each_property() {
        let ctx = distinct_context(); // element 32 / parent 8 / root 4, block 800x600,
                                      // element 200x100, viewport 1000x500

        // em: the element's own font-size, EXCEPT on font-size, where it is the
        // parent's. Getting this backwards is the classic CSS 2.1 §15.7 bug.
        assert_eq!(
            PixelValue::em(2.0).resolve_with_context(&ctx, PropertyContext::Margin),
            64.0
        );
        assert_eq!(
            PixelValue::em(2.0).resolve_with_context(&ctx, PropertyContext::FontSize),
            16.0
        );

        // rem: always the root, whatever the property.
        for pc in ALL_PROPERTY_CONTEXTS {
            assert_eq!(
                PixelValue::rem(2.0).resolve_with_context(&ctx, pc),
                8.0,
                "rem must ignore the property context ({pc:?})"
            );
        }

        // %: the reference depends entirely on the property.
        let pct = PixelValue::percent(50.0);
        assert_eq!(
            pct.resolve_with_context(&ctx, PropertyContext::Width),
            400.0,
            "width % -> containing block WIDTH"
        );
        assert_eq!(
            pct.resolve_with_context(&ctx, PropertyContext::Height),
            300.0,
            "height % -> containing block HEIGHT"
        );
        assert_eq!(
            pct.resolve_with_context(&ctx, PropertyContext::Margin),
            400.0,
            "margin % -> containing block WIDTH, even vertically (CSS 2.1 §8.3)"
        );
        assert_eq!(
            pct.resolve_with_context(&ctx, PropertyContext::Padding),
            400.0,
            "padding % -> containing block WIDTH, even vertically (CSS 2.1 §8.4)"
        );
        assert_eq!(
            pct.resolve_with_context(&ctx, PropertyContext::Other),
            400.0
        );
        assert_eq!(
            pct.resolve_with_context(&ctx, PropertyContext::FontSize),
            4.0,
            "font-size % -> PARENT font size"
        );
        assert_eq!(
            pct.resolve_with_context(&ctx, PropertyContext::BorderRadius),
            100.0,
            "border-radius % -> the element's own box"
        );
        assert_eq!(
            pct.resolve_with_context(&ctx, PropertyContext::Transform),
            100.0
        );
        assert_eq!(
            pct.resolve_with_context(&ctx, PropertyContext::BorderWidth),
            0.0,
            "% is invalid on border-width (CSS Backgrounds 3 §4.1) -> 0"
        );
    }

    #[test]
    fn resolve_with_context_percent_without_an_element_size_is_zero() {
        // element_size is None during the first layout pass; the % arms that read
        // it must degrade to 0 instead of unwrapping.
        let ctx = ResolutionContext {
            element_size: None,
            ..distinct_context()
        };
        assert_eq!(
            PixelValue::percent(50.0)
                .resolve_with_context(&ctx, PropertyContext::BorderRadius),
            0.0
        );
        assert_eq!(
            PixelValue::percent(50.0).resolve_with_context(&ctx, PropertyContext::Transform),
            0.0
        );
    }

    #[test]
    fn resolve_with_context_absolute_units_ignore_the_context_entirely() {
        let sane = distinct_context();
        let poisoned = ResolutionContext {
            element_font_size: f32::NAN,
            parent_font_size: f32::INFINITY,
            root_font_size: f32::NEG_INFINITY,
            containing_block_size: PhysicalSize::new(f32::NAN, f32::NAN),
            element_size: Some(PhysicalSize::new(f32::INFINITY, f32::NAN)),
            viewport_size: PhysicalSize::new(f32::NAN, f32::INFINITY),
        };

        let absolutes = [
            PixelValue::px(10.0),
            PixelValue::pt(10.0),
            PixelValue::inch(10.0),
            PixelValue::cm(10.0),
            PixelValue::mm(10.0),
        ];
        for v in absolutes {
            for pc in ALL_PROPERTY_CONTEXTS {
                let a = v.resolve_with_context(&sane, pc);
                let b = v.resolve_with_context(&poisoned, pc);
                assert_eq!(a, b, "{:?} must not read the context ({pc:?})", v.metric);
                assert!(a.is_finite());
            }
        }

        // ...and they agree with the documented conversion factors.
        assert_eq!(
            PixelValue::px(10.0).resolve_with_context(&sane, PropertyContext::Width),
            10.0
        );
        assert!(approx(
            PixelValue::inch(1.0).resolve_with_context(&sane, PropertyContext::Width),
            96.0
        ));
        assert!(approx(
            PixelValue::pt(72.0).resolve_with_context(&sane, PropertyContext::Width),
            96.0
        ));
        assert!(approx(
            PixelValue::cm(2.54).resolve_with_context(&sane, PropertyContext::Width),
            96.0
        ));
        assert!(approx(
            PixelValue::mm(25.4).resolve_with_context(&sane, PropertyContext::Width),
            96.0
        ));
    }

    #[test]
    fn resolve_with_context_viewport_units_use_the_viewport() {
        let ctx = distinct_context(); // viewport 1000x500

        assert_eq!(
            PixelValue::from_metric(SizeMetric::Vw, 10.0)
                .resolve_with_context(&ctx, PropertyContext::Width),
            100.0
        );
        assert_eq!(
            PixelValue::from_metric(SizeMetric::Vh, 10.0)
                .resolve_with_context(&ctx, PropertyContext::Width),
            50.0
        );
        assert_eq!(
            PixelValue::from_metric(SizeMetric::Vmin, 10.0)
                .resolve_with_context(&ctx, PropertyContext::Width),
            50.0,
            "vmin must take the SMALLER viewport dimension"
        );
        assert_eq!(
            PixelValue::from_metric(SizeMetric::Vmax, 10.0)
                .resolve_with_context(&ctx, PropertyContext::Width),
            100.0,
            "vmax must take the LARGER viewport dimension"
        );

        // A zero viewport (the default context) is not a division-by-zero trap:
        // the /100 is on the viewport side, so this is a plain 0.
        let zero_vp = ResolutionContext::default_const();
        for metric in [
            SizeMetric::Vw,
            SizeMetric::Vh,
            SizeMetric::Vmin,
            SizeMetric::Vmax,
        ] {
            assert_eq!(
                PixelValue::from_metric(metric, 100.0)
                    .resolve_with_context(&zero_vp, PropertyContext::Width),
                0.0,
                "{metric:?} against a 0x0 viewport must be 0"
            );
        }

        // A non-finite viewport propagates rather than panicking.
        let nan_vp = ResolutionContext {
            viewport_size: PhysicalSize::new(f32::NAN, f32::NAN),
            ..distinct_context()
        };
        assert!(PixelValue::from_metric(SizeMetric::Vw, 10.0)
            .resolve_with_context(&nan_vp, PropertyContext::Width)
            .is_nan());
        // NOTE: f32::min/max return the non-NaN operand, so vmin/vmax against a
        // half-NaN viewport silently pick the finite axis instead of poisoning.
        let half_nan_vp = ResolutionContext {
            viewport_size: PhysicalSize::new(f32::NAN, 500.0),
            ..distinct_context()
        };
        assert_eq!(
            PixelValue::from_metric(SizeMetric::Vmin, 10.0)
                .resolve_with_context(&half_nan_vp, PropertyContext::Width),
            50.0
        );
    }

    #[test]
    fn resolve_with_context_never_panics_on_extreme_values() {
        let ctx = distinct_context();
        for metric in ALL_METRICS {
            for v in EXTREME_F32 {
                for pc in ALL_PROPERTY_CONTEXTS {
                    // The only contract here is "returns, deterministically".
                    let _ = PixelValue::from_metric(metric, v).resolve_with_context(&ctx, pc);
                }
            }
        }
    }

    #[test]
    fn resolution_context_default_matches_default_const() {
        // Two hand-written constructors for the same thing: they must not drift.
        let a = ResolutionContext::default();
        let b = ResolutionContext::default_const();

        assert_eq!(a.element_font_size, b.element_font_size);
        assert_eq!(a.parent_font_size, b.parent_font_size);
        assert_eq!(a.root_font_size, b.root_font_size);
        assert_eq!(a.containing_block_size, b.containing_block_size);
        assert_eq!(a.element_size, b.element_size);
        assert_eq!(a.viewport_size, b.viewport_size);

        // The default font size is the CSS "medium" keyword (16px).
        assert_eq!(a.element_font_size, DEFAULT_FONT_SIZE);
        assert!(a.element_size.is_none());
    }

    #[test]
    fn logical_and_physical_sizes_round_trip() {
        let logical = CssLogicalSize::new(800.0, 600.0);
        assert_eq!(logical.to_physical(), PhysicalSize::new(800.0, 600.0));
        assert_eq!(logical.to_physical().to_logical(), logical);

        let physical = PhysicalSize::new(1920.0, 1080.0);
        assert_eq!(physical.to_logical(), CssLogicalSize::new(1920.0, 1080.0));
        assert_eq!(physical.to_logical().to_physical(), physical);

        // In horizontal writing mode inline==width and block==height; a swapped
        // mapping would survive a square, so use a non-square size.
        assert_eq!(CssLogicalSize::new(800.0, 600.0).to_physical().width, 800.0);
        assert_eq!(PhysicalSize::new(800.0, 600.0).to_logical().block_size, 600.0);

        // These are transparent f32 carriers: no sanitizing, no panics.
        let nan = PhysicalSize::new(f32::NAN, f32::INFINITY);
        assert!(nan.to_logical().inline_size.is_nan());
        assert!(nan.to_logical().block_size.is_infinite());
    }

    // ======================================== serializers and round-trips ===

    #[test]
    fn every_rendering_of_a_pixel_value_agrees() {
        // Display, Debug, PrintAsCssValue and FormatAsCssValue are four separate
        // impls of the same string; they must not drift apart.
        for metric in ALL_METRICS {
            let v = PixelValue::from_metric(metric, 1.5);
            let display = v.to_string();
            assert_eq!(format!("{v:?}"), display, "Debug != Display for {metric:?}");
            assert_eq!(v.print_as_css_value(), display);
            assert_eq!(as_css_value(v), display);
            assert!(display.starts_with("1.5"), "{display} lost its number");
            assert!(display.len() > 3, "{display} lost its unit");
        }

        assert_eq!(PixelValue::px(10.0).to_string(), "10px");
        assert_eq!(PixelValue::percent(50.0).to_string(), "50%");
        assert_eq!(PixelValue::zero().to_string(), "0px");
        assert_eq!(
            PixelValue::from_metric(SizeMetric::Vmin, 12.0).to_string(),
            "12vmin"
        );

        // The no-percent wrapper delegates to the inner value.
        let np = PixelValueNoPercent::from(PixelValue::px(10.0));
        assert_eq!(np.to_string(), "10px");
        assert_eq!(format!("{np:?}"), "10px");
        assert_eq!(PixelValueNoPercent::zero().to_string(), "0px");
    }

    #[test]
    fn display_never_leaks_nan_or_infinity_into_css() {
        // A stylesheet containing "NaNpx" would be a serializer bug. The isize
        // encoding is what prevents it -- pin that for every metric and every
        // pathological input.
        for metric in ALL_METRICS {
            for v in EXTREME_F32 {
                let s = PixelValue::from_metric(metric, v).to_string();
                assert!(
                    !s.contains("NaN") && !s.contains("inf"),
                    "{metric:?} with input {v} serialized to {s:?}"
                );
                assert!(!s.is_empty());
            }
        }
        assert_eq!(PixelValue::px(f32::NAN).to_string(), "0px");
    }

    #[test]
    fn pixel_values_round_trip_through_css_for_every_metric_but_vmin() {
        // encode == decode: print_as_css_value -> parse_pixel_value -> same value.
        for metric in ALL_METRICS {
            if metric == SizeMetric::Vmin {
                continue; // known-broken suffix table; see the vmin test above
            }
            for number in [0.0_f32, 1.0, 1.5, -20.0, 0.001, 12345.0] {
                let original = PixelValue::from_metric(metric, number);
                let css = original.print_as_css_value();
                let reparsed = parse_pixel_value(&css).unwrap_or_else(|e| {
                    panic!("{css:?} (from {metric:?} {number}) failed to re-parse: {e:?}")
                });
                assert_eq!(reparsed, original, "round-trip broke for {css:?}");
                // ...and re-printing is idempotent.
                assert_eq!(reparsed.print_as_css_value(), css);
            }
        }

        // The no-percent parser round-trips everything except % (and vmin).
        for metric in ALL_METRICS {
            if metric == SizeMetric::Vmin || metric == SizeMetric::Percent {
                continue;
            }
            let original = PixelValueNoPercent::from(PixelValue::from_metric(metric, 7.0));
            let css = original.to_string();
            assert_eq!(
                parse_pixel_value_no_percent(&css).unwrap(),
                original,
                "no-percent round-trip broke for {css:?}"
            );
        }

        // ...and the with-auto wrapper round-trips its keywords and its lengths.
        for (css, expected) in [
            ("auto", PixelValueWithAuto::Auto),
            ("none", PixelValueWithAuto::None),
            ("initial", PixelValueWithAuto::Initial),
            ("inherit", PixelValueWithAuto::Inherit),
        ] {
            assert_eq!(parse_pixel_value_with_auto(css).unwrap(), expected);
        }
        let exact = PixelValue::em(1.5);
        assert_eq!(
            parse_pixel_value_with_auto(&exact.print_as_css_value()).unwrap(),
            PixelValueWithAuto::Exact(exact)
        );
    }

    #[test]
    fn format_as_rust_code_emits_a_reconstructible_literal() {
        assert_eq!(
            PixelValue::px(10.0).format_as_rust_code(0),
            "PixelValue { metric: Px, number: FloatValue::new(10) }"
        );
        assert_eq!(
            PixelValue::percent(-1.5).format_as_rust_code(4),
            "PixelValue { metric: Percent, number: FloatValue::new(-1.5) }"
        );
        // Even a pathological input must emit compilable code, never "NaN".
        let nan = PixelValue::from_metric(SizeMetric::Vmax, f32::NAN).format_as_rust_code(0);
        assert_eq!(nan, "PixelValue { metric: Vmax, number: FloatValue::new(0) }");
        assert!(!PixelValue::px(f32::INFINITY)
            .format_as_rust_code(0)
            .contains("inf"));
    }

    #[test]
    fn border_thickness_constants_match_the_css_keywords() {
        // thin/medium/thick are hand-encoded as raw FloatValue bit patterns, so a
        // change to FP_PRECISION_MULTIPLIER would silently rescale them.
        assert_eq!(THIN_BORDER_THICKNESS, PixelValue::px(1.0));
        assert_eq!(MEDIUM_BORDER_THICKNESS, PixelValue::px(3.0));
        assert_eq!(THICK_BORDER_THICKNESS, PixelValue::px(5.0));

        assert_eq!(THIN_BORDER_THICKNESS.number.get(), 1.0);
        assert_eq!(MEDIUM_BORDER_THICKNESS.number.get(), 3.0);
        assert_eq!(THICK_BORDER_THICKNESS.number.get(), 5.0);
        assert_eq!(THIN_BORDER_THICKNESS.number.number() as f32, MULT);

        assert!(THIN_BORDER_THICKNESS < MEDIUM_BORDER_THICKNESS);
        assert!(MEDIUM_BORDER_THICKNESS < THICK_BORDER_THICKNESS);
        assert_eq!(THIN_BORDER_THICKNESS.to_string(), "1px");
    }

    #[test]
    fn ord_is_lexicographic_by_metric_then_number_not_by_resolved_size() {
        // PixelValue derives Ord over (metric, number). That means 100px sorts
        // BELOW 1em even though it is far larger once resolved. Anything that
        // sorts or range-queries these values needs to know that.
        assert!(PixelValue::px(100.0) < PixelValue::em(1.0));
        assert!(PixelValue::px(1.0) < PixelValue::px(2.0));
        assert!(PixelValue::percent(1.0) > PixelValue::mm(9999.0));

        // Eq/Hash agree, including across the sanitized encodings.
        let a = PixelValue::px(1.5);
        let b = PixelValue::px(1.5);
        assert_eq!(a, b);
        assert_eq!(hash_of(&a), hash_of(&b));
        assert_ne!(hash_of(&PixelValue::px(1.0)), hash_of(&PixelValue::em(1.0)));

        // Two values that differ only below the 1/1000 quantum collide -- by
        // design, since that is what makes PixelValue hashable at all.
        assert_eq!(PixelValue::px(1.0001), PixelValue::px(1.0002));
        assert_eq!(
            hash_of(&PixelValue::px(1.0001)),
            hash_of(&PixelValue::px(1.0002))
        );
    }

    // ====================================================== system metrics ===

    #[test]
    fn system_metric_ref_css_strings_round_trip() {
        for r in ALL_SYSTEM_REFS {
            let css = r.as_css_str();
            assert!(
                css.starts_with("system:"),
                "{css:?} is missing the system: prefix"
            );
            assert_eq!(r.to_string(), css, "Display must match as_css_str");
            assert_eq!(as_css_value(r), css);

            // from_css_str takes the name WITHOUT the prefix.
            let name = css.strip_prefix("system:").unwrap();
            assert_eq!(
                SystemMetricRef::from_css_str(name),
                Some(r),
                "{name:?} must parse back to {r:?}"
            );
            // Serialize-parse-serialize is stable.
            assert_eq!(SystemMetricRef::from_css_str(name).unwrap().as_css_str(), css);

            // Footgun worth pinning: feeding the *full* CSS string back in fails,
            // because from_css_str does not strip the prefix itself.
            assert_eq!(SystemMetricRef::from_css_str(css), None);
        }

        assert_eq!(SystemMetricRef::default(), SystemMetricRef::ButtonRadius);
    }

    #[test]
    fn system_metric_ref_from_css_str_rejects_everything_else() {
        for input in [
            "",
            "   ",
            "\t\n",
            " button-radius ",     // no trimming
            "Button-Radius",       // case-sensitive
            "button_radius",       // wrong separator
            "button-padding",      // the spelling the docs advertise; not a real one
            "button-radius;x",
            "\u{1F600}",
            "b\u{0301}utton-radius",
        ] {
            assert_eq!(
                SystemMetricRef::from_css_str(input),
                None,
                "{input:?} must not resolve to a system metric"
            );
        }

        // Long input is rejected without hanging.
        assert_eq!(
            SystemMetricRef::from_css_str(&"a".repeat(100_000)),
            None
        );
        assert_eq!(SystemMetricRef::from_css_str(&"(".repeat(10_000)), None);
    }

    #[test]
    fn system_metric_ref_resolve_maps_each_variant_to_its_own_field() {
        // Every field gets a distinct value, so a mis-wired arm cannot pass.
        let metrics = populated_metrics();
        let expected = [
            (SystemMetricRef::ButtonRadius, 1.0),
            (SystemMetricRef::ButtonBorderWidth, 2.0),
            (SystemMetricRef::ButtonPaddingHorizontal, 3.0),
            (SystemMetricRef::ButtonPaddingVertical, 4.0),
            (SystemMetricRef::TitlebarHeight, 5.0),
            (SystemMetricRef::TitlebarButtonWidth, 6.0),
            (SystemMetricRef::TitlebarPadding, 7.0),
            (SystemMetricRef::SafeAreaTop, 8.0),
            (SystemMetricRef::SafeAreaBottom, 9.0),
            (SystemMetricRef::SafeAreaLeft, 10.0),
            (SystemMetricRef::SafeAreaRight, 11.0),
        ];
        for (r, px) in expected {
            assert_eq!(
                r.resolve(&metrics),
                Some(PixelValue::px(px)),
                "{r:?} resolved to the wrong field"
            );
        }

        // An unpopulated SystemMetrics yields None for every variant (no unwraps).
        let empty = SystemMetrics::default();
        for r in ALL_SYSTEM_REFS {
            assert_eq!(r.resolve(&empty), None, "{r:?} must be None when unset");
        }
    }

    #[test]
    fn pixel_value_or_system_resolves_and_falls_back() {
        let metrics = populated_metrics();
        let empty = SystemMetrics::default();
        let fallback = PixelValue::px(99.0);

        // A concrete value ignores the system metrics entirely.
        let concrete = PixelValueOrSystem::value(PixelValue::px(10.0));
        assert_eq!(concrete.resolve(&metrics, fallback), PixelValue::px(10.0));
        assert_eq!(concrete.resolve(&empty, fallback), PixelValue::px(10.0));

        // A system ref takes the metric when present...
        let sys = PixelValueOrSystem::system(SystemMetricRef::ButtonRadius);
        assert_eq!(sys.resolve(&metrics, fallback), PixelValue::px(1.0));
        // ...and the fallback when absent, for every variant.
        for r in ALL_SYSTEM_REFS {
            assert_eq!(
                PixelValueOrSystem::system(r).resolve(&empty, fallback),
                fallback,
                "{r:?} must fall back when the metric is unset"
            );
        }

        // Extreme fallbacks stay finite (they went through FloatValue too).
        let nan_fallback = PixelValue::px(f32::NAN);
        assert_eq!(
            sys.resolve(&empty, nan_fallback).number.get(),
            0.0
        );

        // Constructors / conversions / default.
        assert_eq!(
            PixelValueOrSystem::default(),
            PixelValueOrSystem::Value(PixelValue::zero())
        );
        assert_eq!(
            PixelValueOrSystem::from(PixelValue::em(2.0)),
            PixelValueOrSystem::Value(PixelValue::em(2.0))
        );
        assert_eq!(
            PixelValueOrSystem::default().resolve(&metrics, fallback),
            PixelValue::zero()
        );
    }

    #[test]
    fn pixel_value_or_system_renders_both_arms() {
        let concrete = PixelValueOrSystem::value(PixelValue::px(10.0));
        assert_eq!(concrete.to_string(), "10px");
        assert_eq!(as_css_value(concrete), "10px");

        let sys = PixelValueOrSystem::system(SystemMetricRef::TitlebarHeight);
        assert_eq!(sys.to_string(), "system:titlebar-height");
        assert_eq!(as_css_value(sys), "system:titlebar-height");

        assert_eq!(PixelValueOrSystem::default().to_string(), "0px");

        // No arm can serialize a non-finite number.
        for v in EXTREME_F32 {
            let s = PixelValueOrSystem::value(PixelValue::px(v)).to_string();
            assert!(!s.contains("NaN") && !s.contains("inf"), "leaked {s:?}");
        }
    }
}

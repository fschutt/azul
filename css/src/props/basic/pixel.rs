use core::fmt;
use std::num::ParseFloatError;
use crate::corety::AzString;

use crate::props::{
    basic::{error::ParseFloatErrorWithInput, FloatValue, SizeMetric},
    formatter::FormatAsCssValue,
};

/// Default/fallback font size in pixels, used when no font-size is specified.
///
/// This is the same as the CSS "medium" keyword and matches browser defaults:
/// - CSS 2.1 §15.7: "medium" is the user's preferred font size
/// - All major browsers default to 16px
/// - W3C HTML5: The default font-size of the root element is 16px
///
/// This constant is used in two scenarios:
/// 1. As fallback when no explicit font-size is found in the cascade
/// 2. In legacy `to_pixels()` for em/rem conversion when no context available
///
/// **Research:**
/// - Chrome/Firefox/Safari: 16px default
/// - CSS font-size keywords: medium = 16px (derived from 13.33px * 1.2)
/// - Can be overridden by user preferences (browser settings)
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
    pub const fn new(value: f32) -> Self {
        Self(value)
    }

    /// Create a percentage from an unnormalized value (0-100 scale)
    ///
    /// This divides by 100 internally, so you should use this when converting
    /// from CSS percentage syntax like "50%" which is stored as 50.0.
    #[inline]
    pub fn from_unnormalized(value: f32) -> Self {
        Self(value / 100.0)
    }

    /// Get the raw normalized value (0.0-1.0)
    #[inline]
    pub const fn get(self) -> f32 {
        self.0
    }

    /// Resolve this percentage against a containing block size
    ///
    /// This multiplies the normalized percentage by the containing block size.
    /// For example, 50% (0.5) of 640px = 320px.
    #[inline]
    pub fn resolve(self, containing_block_size: f32) -> f32 {
        self.0 * containing_block_size
    }
}

impl fmt::Display for NormalizedPercentage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
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
    pub const fn new(inline_size: f32, block_size: f32) -> Self {
        Self {
            inline_size,
            block_size,
        }
    }

    /// Convert to physical size (width, height) in horizontal writing mode
    #[inline]
    pub const fn to_physical(self) -> PhysicalSize {
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
    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }

    /// Convert to logical size in horizontal writing mode
    #[inline]
    pub const fn to_logical(self) -> CssLogicalSize {
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
    pub const fn default_const() -> Self {
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

    /// Create a context with only font-size information (for font-relative units)
    #[inline]
    pub const fn for_fonts(
        element_font_size: f32,
        parent_font_size: f32,
        root_font_size: f32,
    ) -> Self {
        Self {
            element_font_size,
            parent_font_size,
            root_font_size,
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

    /// Create a context with containing block information (for percentage units)
    #[inline]
    pub const fn with_containing_block(mut self, containing_block_size: PhysicalSize) -> Self {
        self.containing_block_size = containing_block_size;
        self
    }

    /// Create a context with element size information (for border-radius, transforms)
    #[inline]
    pub const fn with_element_size(mut self, element_size: PhysicalSize) -> Self {
        self.element_size = Some(element_size);
        self
    }

    /// Create a context with viewport size information (for vw, vh, vmin, vmax units)
    #[inline]
    pub const fn with_viewport_size(mut self, viewport_size: PhysicalSize) -> Self {
        self.viewport_size = viewport_size;
        self
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
    fn format_as_css_value(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}{}", self.number, self.metric)
    }
}

impl crate::css::PrintAsCssValue for PixelValue {
    fn print_as_css_value(&self) -> String {
        format!("{}{}", self.number, self.metric)
    }
}

impl crate::format_rust_code::FormatAsRustCode for PixelValue {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "PixelValue {{ metric: {:?}, number: FloatValue::new({}) }}",
            self.metric,
            self.number.get()
        )
    }
}

impl fmt::Debug for PixelValue {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}{}", self.number, self.metric)
    }
}

// Manual Debug implementation, because the auto-generated one is nearly unreadable
impl fmt::Display for PixelValue {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}{}", self.number, self.metric)
    }
}

impl PixelValue {
    #[inline]
    pub const fn zero() -> Self {
        const ZERO_PX: PixelValue = PixelValue::const_px(0);
        ZERO_PX
    }

    /// Same as `PixelValue::px()`, but only accepts whole numbers,
    /// since using `f32` in const fn is not yet stabilized.
    #[inline]
    pub const fn const_px(value: isize) -> Self {
        Self::const_from_metric(SizeMetric::Px, value)
    }

    /// Same as `PixelValue::em()`, but only accepts whole numbers,
    /// since using `f32` in const fn is not yet stabilized.
    #[inline]
    pub const fn const_em(value: isize) -> Self {
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
    pub const fn const_em_fractional(pre_comma: isize, post_comma: isize) -> Self {
        Self::const_from_metric_fractional(SizeMetric::Em, pre_comma, post_comma)
    }

    /// Same as `PixelValue::pt()`, but only accepts whole numbers,
    /// since using `f32` in const fn is not yet stabilized.
    #[inline]
    pub const fn const_pt(value: isize) -> Self {
        Self::const_from_metric(SizeMetric::Pt, value)
    }

    /// Creates a pt value from a fractional number in const context.
    #[inline]
    pub const fn const_pt_fractional(pre_comma: isize, post_comma: isize) -> Self {
        Self::const_from_metric_fractional(SizeMetric::Pt, pre_comma, post_comma)
    }

    /// Same as `PixelValue::pt()`, but only accepts whole numbers,
    /// since using `f32` in const fn is not yet stabilized.
    #[inline]
    pub const fn const_percent(value: isize) -> Self {
        Self::const_from_metric(SizeMetric::Percent, value)
    }

    /// Same as `PixelValue::in()`, but only accepts whole numbers,
    /// since using `f32` in const fn is not yet stabilized.
    #[inline]
    pub const fn const_in(value: isize) -> Self {
        Self::const_from_metric(SizeMetric::In, value)
    }

    /// Same as `PixelValue::in()`, but only accepts whole numbers,
    /// since using `f32` in const fn is not yet stabilized.
    #[inline]
    pub const fn const_cm(value: isize) -> Self {
        Self::const_from_metric(SizeMetric::Cm, value)
    }

    /// Same as `PixelValue::in()`, but only accepts whole numbers,
    /// since using `f32` in const fn is not yet stabilized.
    #[inline]
    pub const fn const_mm(value: isize) -> Self {
        Self::const_from_metric(SizeMetric::Mm, value)
    }

    #[inline]
    pub const fn const_from_metric(metric: SizeMetric, value: isize) -> Self {
        Self {
            metric,
            number: FloatValue::const_new(value),
        }
    }

    /// Creates a PixelValue from a fractional number in const context.
    ///
    /// # Arguments
    /// * `metric` - The size metric (Px, Em, Pt, etc.)
    /// * `pre_comma` - The integer part
    /// * `post_comma` - The fractional part as digits
    #[inline]
    pub const fn const_from_metric_fractional(
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
    pub fn px(value: f32) -> Self {
        Self::from_metric(SizeMetric::Px, value)
    }

    #[inline]
    pub fn em(value: f32) -> Self {
        Self::from_metric(SizeMetric::Em, value)
    }

    #[inline]
    pub fn inch(value: f32) -> Self {
        Self::from_metric(SizeMetric::In, value)
    }

    #[inline]
    pub fn cm(value: f32) -> Self {
        Self::from_metric(SizeMetric::Cm, value)
    }

    #[inline]
    pub fn mm(value: f32) -> Self {
        Self::from_metric(SizeMetric::Mm, value)
    }

    #[inline]
    pub fn pt(value: f32) -> Self {
        Self::from_metric(SizeMetric::Pt, value)
    }

    #[inline]
    pub fn percent(value: f32) -> Self {
        Self::from_metric(SizeMetric::Percent, value)
    }

    #[inline]
    pub fn rem(value: f32) -> Self {
        Self::from_metric(SizeMetric::Rem, value)
    }

    #[inline]
    pub fn from_metric(metric: SizeMetric, value: f32) -> Self {
        Self {
            metric,
            number: FloatValue::new(value),
        }
    }

    #[inline]
    pub fn interpolate(&self, other: &Self, t: f32) -> Self {
        if self.metric == other.metric {
            Self {
                metric: self.metric,
                number: self.number.interpolate(&other.number, t),
            }
        } else {
            // Interpolate between different metrics by converting to px
            // Note: Uses DEFAULT_FONT_SIZE for em/rem - acceptable for animation fallback
            let self_px_interp = self.to_pixels_internal(0.0, DEFAULT_FONT_SIZE);
            let other_px_interp = other.to_pixels_internal(0.0, DEFAULT_FONT_SIZE);
            Self::from_metric(
                SizeMetric::Px,
                self_px_interp + (other_px_interp - self_px_interp) * t,
            )
        }
    }

    /// Returns the value of the SizeMetric as a normalized percentage (0.0 = 0%, 1.0 = 100%)
    ///
    /// Returns `Some(NormalizedPercentage)` if this is a percentage value, `None` otherwise.
    /// The returned `NormalizedPercentage` is already normalized to 0.0-1.0 range,
    /// so you should multiply it directly with the containing block size.
    #[inline]
    pub fn to_percent(&self) -> Option<NormalizedPercentage> {
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
    pub fn to_pixels_internal(&self, percent_resolve: f32, em_resolve: f32) -> f32 {
        match self.metric {
            SizeMetric::Px => self.number.get(),
            SizeMetric::Pt => self.number.get() * PT_TO_PX,
            SizeMetric::In => self.number.get() * 96.0,
            SizeMetric::Cm => self.number.get() * 96.0 / 2.54,
            SizeMetric::Mm => self.number.get() * 96.0 / 25.4,
            SizeMetric::Em => self.number.get() * em_resolve,
            SizeMetric::Rem => self.number.get() * em_resolve,
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
    pub fn resolve_with_context(
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
                let reference = match property_context {
                    // Font-size %: refers to parent's font-size (CSS 2.1 §15.7)
                    PropertyContext::FontSize => context.parent_font_size,

                    // Width and horizontal properties: containing block width (CSS 2.1 §10.3)
                    PropertyContext::Width => context.containing_block_size.width,

                    // Height and vertical properties: containing block height (CSS 2.1 §10.5)
                    PropertyContext::Height => context.containing_block_size.height,

                    // Margins: ALWAYS containing block WIDTH, even for top/bottom! (CSS 2.1 §8.3)
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
                        context.element_size.map(|s| s.width).unwrap_or(0.0)
                    }

                    // Transforms: element's own dimensions (CSS Transforms §20.1)
                    PropertyContext::Transform => {
                        context.element_size.map(|s| s.width).unwrap_or(0.0)
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

/// Same as PixelValue, but doesn't allow a "%" sign
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
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl ::core::fmt::Debug for PixelValueNoPercent {
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        write!(f, "{}", self)
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
    pub fn to_pixels_internal(&self, em_resolve: f32) -> f32 {
        self.inner.to_pixels_internal(0.0, em_resolve)
    }

    #[inline]
    pub const fn zero() -> Self {
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

#[derive(Clone, PartialEq)]
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

/// Wrapper for NoValueGiven error in pixel value parsing.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct PixelNoValueGivenError {
    pub value: String,
    pub metric: SizeMetric,
}

/// Owned version of CssPixelValueParseError.
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum CssPixelValueParseErrorOwned {
    EmptyString,
    NoValueGiven(PixelNoValueGivenError),
    ValueParseErr(ParseFloatErrorWithInput),
    InvalidPixelValue(AzString),
}

impl<'a> CssPixelValueParseError<'a> {
    pub fn to_contained(&self) -> CssPixelValueParseErrorOwned {
        match self {
            CssPixelValueParseError::EmptyString => CssPixelValueParseErrorOwned::EmptyString,
            CssPixelValueParseError::NoValueGiven(s, metric) => {
                CssPixelValueParseErrorOwned::NoValueGiven(PixelNoValueGivenError { value: s.to_string(), metric: *metric })
            }
            CssPixelValueParseError::ValueParseErr(err, s) => {
                CssPixelValueParseErrorOwned::ValueParseErr(ParseFloatErrorWithInput { error: err.clone(), input: s.to_string() })
            }
            CssPixelValueParseError::InvalidPixelValue(s) => {
                CssPixelValueParseErrorOwned::InvalidPixelValue(s.to_string().into())
            }
        }
    }
}

impl CssPixelValueParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssPixelValueParseError<'a> {
        match self {
            CssPixelValueParseErrorOwned::EmptyString => CssPixelValueParseError::EmptyString,
            CssPixelValueParseErrorOwned::NoValueGiven(e) => {
                CssPixelValueParseError::NoValueGiven(e.value.as_str(), e.metric)
            }
            CssPixelValueParseErrorOwned::ValueParseErr(e) => {
                CssPixelValueParseError::ValueParseErr(e.error.clone(), e.input.as_str())
            }
            CssPixelValueParseErrorOwned::InvalidPixelValue(s) => {
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
        if input.ends_with(match_val) {
            let value = &input[..input.len() - match_val.len()];
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

    match input.trim().parse::<f32>() {
        Ok(o) => Ok(PixelValue::px(o)),
        Err(_) => Err(CssPixelValueParseError::InvalidPixelValue(input)),
    }
}

pub fn parse_pixel_value<'a>(input: &'a str) -> Result<PixelValue, CssPixelValueParseError<'a>> {
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

pub fn parse_pixel_value_no_percent<'a>(
    input: &'a str,
) -> Result<PixelValueNoPercent, CssPixelValueParseError<'a>> {
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
pub fn parse_pixel_value_with_auto<'a>(
    input: &'a str,
) -> Result<PixelValueWithAuto, CssPixelValueParseError<'a>> {
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
pub enum SystemMetricRef {
    /// Button corner radius (system:button-radius)
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

impl Default for SystemMetricRef {
    fn default() -> Self {
        SystemMetricRef::ButtonRadius
    }
}

impl SystemMetricRef {
    /// Resolve this system metric reference against actual system metrics.
    pub fn resolve(&self, metrics: &crate::system::SystemMetrics) -> Option<PixelValue> {
        match self {
            SystemMetricRef::ButtonRadius => metrics.corner_radius.as_option().copied(),
            SystemMetricRef::ButtonPaddingHorizontal => metrics.button_padding_horizontal.as_option().copied(),
            SystemMetricRef::ButtonPaddingVertical => metrics.button_padding_vertical.as_option().copied(),
            SystemMetricRef::ButtonBorderWidth => metrics.border_width.as_option().copied(),
            SystemMetricRef::TitlebarHeight => metrics.titlebar.height.as_option().copied(),
            SystemMetricRef::TitlebarButtonWidth => metrics.titlebar.button_area_width.as_option().copied(),
            SystemMetricRef::TitlebarPadding => metrics.titlebar.padding_horizontal.as_option().copied(),
            SystemMetricRef::SafeAreaTop => metrics.titlebar.safe_area.top.as_option().copied(),
            SystemMetricRef::SafeAreaBottom => metrics.titlebar.safe_area.bottom.as_option().copied(),
            SystemMetricRef::SafeAreaLeft => metrics.titlebar.safe_area.left.as_option().copied(),
            SystemMetricRef::SafeAreaRight => metrics.titlebar.safe_area.right.as_option().copied(),
        }
    }

    /// Returns the CSS string representation of this system metric reference.
    pub fn as_css_str(&self) -> &'static str {
        match self {
            SystemMetricRef::ButtonRadius => "system:button-radius",
            SystemMetricRef::ButtonPaddingHorizontal => "system:button-padding-horizontal",
            SystemMetricRef::ButtonPaddingVertical => "system:button-padding-vertical",
            SystemMetricRef::ButtonBorderWidth => "system:button-border-width",
            SystemMetricRef::TitlebarHeight => "system:titlebar-height",
            SystemMetricRef::TitlebarButtonWidth => "system:titlebar-button-width",
            SystemMetricRef::TitlebarPadding => "system:titlebar-padding",
            SystemMetricRef::SafeAreaTop => "system:safe-area-top",
            SystemMetricRef::SafeAreaBottom => "system:safe-area-bottom",
            SystemMetricRef::SafeAreaLeft => "system:safe-area-left",
            SystemMetricRef::SafeAreaRight => "system:safe-area-right",
        }
    }

    /// Parse a system metric reference from a CSS string (without the "system:" prefix).
    pub fn from_css_str(s: &str) -> Option<Self> {
        match s {
            "button-radius" => Some(SystemMetricRef::ButtonRadius),
            "button-padding-horizontal" => Some(SystemMetricRef::ButtonPaddingHorizontal),
            "button-padding-vertical" => Some(SystemMetricRef::ButtonPaddingVertical),
            "button-border-width" => Some(SystemMetricRef::ButtonBorderWidth),
            "titlebar-height" => Some(SystemMetricRef::TitlebarHeight),
            "titlebar-button-width" => Some(SystemMetricRef::TitlebarButtonWidth),
            "titlebar-padding" => Some(SystemMetricRef::TitlebarPadding),
            "safe-area-top" => Some(SystemMetricRef::SafeAreaTop),
            "safe-area-bottom" => Some(SystemMetricRef::SafeAreaBottom),
            "safe-area-left" => Some(SystemMetricRef::SafeAreaLeft),
            "safe-area-right" => Some(SystemMetricRef::SafeAreaRight),
            _ => None,
        }
    }
}

impl fmt::Display for SystemMetricRef {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.as_css_str())
    }
}

impl FormatAsCssValue for SystemMetricRef {
    fn format_as_css_value(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.as_css_str())
    }
}

/// A pixel value reference that can be either a concrete value or a system metric.
/// System metrics are lazily evaluated at runtime based on the user's system theme.
/// 
/// CSS syntax: `10px`, `1.5em`, `system:button-padding`, etc.
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C, u8)]
pub enum PixelValueOrSystem {
    /// A concrete pixel value.
    Value(PixelValue),
    /// A reference to a system metric, resolved at runtime.
    System(SystemMetricRef),
}

impl Default for PixelValueOrSystem {
    fn default() -> Self {
        PixelValueOrSystem::Value(PixelValue::zero())
    }
}

impl From<PixelValue> for PixelValueOrSystem {
    fn from(value: PixelValue) -> Self {
        PixelValueOrSystem::Value(value)
    }
}

impl PixelValueOrSystem {
    /// Create a new PixelValueOrSystem from a concrete value.
    pub const fn value(v: PixelValue) -> Self {
        PixelValueOrSystem::Value(v)
    }
    
    /// Create a new PixelValueOrSystem from a system metric reference.
    pub const fn system(s: SystemMetricRef) -> Self {
        PixelValueOrSystem::System(s)
    }
    
    /// Resolve the pixel value against a SystemMetrics struct.
    /// Returns the system metric if available, or falls back to the provided default.
    pub fn resolve(&self, system_metrics: &crate::system::SystemMetrics, fallback: PixelValue) -> PixelValue {
        match self {
            PixelValueOrSystem::Value(v) => *v,
            PixelValueOrSystem::System(ref_type) => ref_type.resolve(system_metrics).unwrap_or(fallback),
        }
    }
    
    /// Returns the concrete value if available, or a default fallback for system metrics.
    pub fn to_pixel_value_with_fallback(&self, fallback: PixelValue) -> PixelValue {
        match self {
            PixelValueOrSystem::Value(v) => *v,
            PixelValueOrSystem::System(_) => fallback,
        }
    }
    
    /// Returns the concrete value if available, or zero for system metrics.
    pub fn to_pixel_value_default(&self) -> PixelValue {
        self.to_pixel_value_with_fallback(PixelValue::zero())
    }
}

impl fmt::Display for PixelValueOrSystem {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            PixelValueOrSystem::Value(v) => write!(f, "{}", v),
            PixelValueOrSystem::System(s) => write!(f, "{}", s),
        }
    }
}

impl FormatAsCssValue for PixelValueOrSystem {
    fn format_as_css_value(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            PixelValueOrSystem::Value(v) => v.format_as_css_value(f),
            PixelValueOrSystem::System(s) => s.format_as_css_value(f),
        }
    }
}

/// Parse a pixel value that may include system metric references.
/// 
/// Accepts: `10px`, `1.5em`, `system:button-padding`, etc.
#[cfg(feature = "parser")]
pub fn parse_pixel_value_or_system<'a>(
    input: &'a str,
) -> Result<PixelValueOrSystem, CssPixelValueParseError<'a>> {
    let input = input.trim();
    
    // Check for system metric reference
    if let Some(metric_name) = input.strip_prefix("system:") {
        if let Some(metric_ref) = SystemMetricRef::from_css_str(metric_name) {
            return Ok(PixelValueOrSystem::System(metric_ref));
        } else {
            return Err(CssPixelValueParseError::InvalidPixelValue(input));
        }
    }
    
    // Parse as regular pixel value
    Ok(PixelValueOrSystem::Value(parse_pixel_value(input)?))
}

#[cfg(all(test, feature = "parser"))]
mod tests {
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

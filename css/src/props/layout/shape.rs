//! CSS properties for flowing content around shapes (CSS Shapes Module).
//!
//! Defines [`ShapeOutside`], [`ShapeInside`], [`ClipPath`], [`ShapeMargin`],
//! and [`ShapeImageThreshold`]. Note: `ClipPath` belongs to CSS Masking but
//! is co-located here for convenience.

use alloc::string::{String, ToString};

use crate::{
    props::{
        basic::{
            length::{parse_float_value, FloatValue},
            pixel::{
                parse_pixel_value, CssPixelValueParseError,
                PixelValue,
            },
        },
        formatter::PrintAsCssValue,
    },
    shape::CssShape,
};
#[allow(variant_size_differences)] // repr(C,u8) FFI enum: boxing the large variant would change the C ABI (api.json bindings); size disparity accepted
/// CSS shape-outside property for wrapping text around shapes
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
#[derive(Default)]
pub enum ShapeOutside {
    #[default]
    None,
    Shape(CssShape),
}

impl Eq for ShapeOutside {}
impl core::hash::Hash for ShapeOutside {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        core::mem::discriminant(self).hash(state);
        if let Self::Shape(s) = self {
            s.hash(state);
        }
    }
}
impl PartialOrd for ShapeOutside {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for ShapeOutside {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        match (self, other) {
            (Self::None, Self::None) => core::cmp::Ordering::Equal,
            (Self::None, Self::Shape(_)) => core::cmp::Ordering::Less,
            (Self::Shape(_), Self::None) => core::cmp::Ordering::Greater,
            (Self::Shape(a), Self::Shape(b)) => a.cmp(b),
        }
    }
}


impl PrintAsCssValue for ShapeOutside {
    fn print_as_css_value(&self) -> String {
        match self {
            Self::None => "none".to_string(),
            Self::Shape(shape) => shape.print_as_css_value(),
        }
    }
}
#[allow(variant_size_differences)] // repr(C,u8) FFI enum: boxing the large variant would change the C ABI (api.json bindings); size disparity accepted
/// CSS shape-inside property for flowing text within shapes
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
#[derive(Default)]
pub enum ShapeInside {
    #[default]
    None,
    Shape(CssShape),
}

impl Eq for ShapeInside {}
impl core::hash::Hash for ShapeInside {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        core::mem::discriminant(self).hash(state);
        if let Self::Shape(s) = self {
            s.hash(state);
        }
    }
}
impl PartialOrd for ShapeInside {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for ShapeInside {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        match (self, other) {
            (Self::None, Self::None) => core::cmp::Ordering::Equal,
            (Self::None, Self::Shape(_)) => core::cmp::Ordering::Less,
            (Self::Shape(_), Self::None) => core::cmp::Ordering::Greater,
            (Self::Shape(a), Self::Shape(b)) => a.cmp(b),
        }
    }
}


impl PrintAsCssValue for ShapeInside {
    fn print_as_css_value(&self) -> String {
        match self {
            Self::None => "none".to_string(),
            Self::Shape(shape) => shape.print_as_css_value(),
        }
    }
}
#[allow(variant_size_differences)] // repr(C,u8) FFI enum: boxing the large variant would change the C ABI (api.json bindings); size disparity accepted
/// CSS clip-path property for clipping element rendering
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
#[derive(Default)]
pub enum ClipPath {
    #[default]
    None,
    Shape(CssShape),
}

impl Eq for ClipPath {}
impl core::hash::Hash for ClipPath {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        core::mem::discriminant(self).hash(state);
        if let Self::Shape(s) = self {
            s.hash(state);
        }
    }
}
impl PartialOrd for ClipPath {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for ClipPath {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        match (self, other) {
            (Self::None, Self::None) => core::cmp::Ordering::Equal,
            (Self::None, Self::Shape(_)) => core::cmp::Ordering::Less,
            (Self::Shape(_), Self::None) => core::cmp::Ordering::Greater,
            (Self::Shape(a), Self::Shape(b)) => a.cmp(b),
        }
    }
}


impl PrintAsCssValue for ClipPath {
    fn print_as_css_value(&self) -> String {
        match self {
            Self::None => "none".to_string(),
            Self::Shape(shape) => shape.print_as_css_value(),
        }
    }
}

/// CSS `shape-margin` property — adds margin to the shape-outside exclusion area.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct ShapeMargin {
    pub inner: PixelValue,
}

impl Default for ShapeMargin {
    fn default() -> Self {
        Self {
            inner: PixelValue::zero(),
        }
    }
}

impl PrintAsCssValue for ShapeMargin {
    fn print_as_css_value(&self) -> String {
        self.inner.print_as_css_value()
    }
}

/// CSS `shape-image-threshold` property — alpha threshold for image-based shapes.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct ShapeImageThreshold {
    pub inner: FloatValue,
}

impl Default for ShapeImageThreshold {
    fn default() -> Self {
        Self {
            inner: FloatValue::const_new(0),
        }
    }
}

impl PrintAsCssValue for ShapeImageThreshold {
    fn print_as_css_value(&self) -> String {
        self.inner.to_string()
    }
}

// Formatting to Rust code
impl crate::codegen::format::FormatAsRustCode for ShapeOutside {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        match self {
            Self::None => String::from("ShapeOutside::None"),
            Self::Shape(s) => {
                let mut r = String::from("ShapeOutside::Shape(");
                r.push_str(&s.format_as_rust_code());
                r.push(')');
                r
            }
        }
    }
}

impl crate::codegen::format::FormatAsRustCode for ShapeInside {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        match self {
            Self::None => String::from("ShapeInside::None"),
            Self::Shape(s) => {
                let mut r = String::from("ShapeInside::Shape(");
                r.push_str(&s.format_as_rust_code());
                r.push(')');
                r
            }
        }
    }
}

impl crate::codegen::format::FormatAsRustCode for ClipPath {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        match self {
            Self::None => String::from("ClipPath::None"),
            Self::Shape(s) => {
                let mut r = String::from("ClipPath::Shape(");
                r.push_str(&s.format_as_rust_code());
                r.push(')');
                r
            }
        }
    }
}

impl crate::codegen::format::FormatAsRustCode for ShapeMargin {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "ShapeMargin {{ inner: {} }}",
            crate::codegen::format::format_pixel_value(&self.inner)
        )
    }
}

impl crate::codegen::format::FormatAsRustCode for ShapeImageThreshold {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "ShapeImageThreshold {{ inner: {} }}",
            crate::codegen::format::format_float_value(&self.inner)
        )
    }
}

// --- PARSERS ---
#[cfg(feature = "parser")]
pub mod parser {
    use core::num::ParseFloatError;

    #[allow(clippy::wildcard_imports)] // parser submodule reuses the parent module's value types
    use super::*;
    use crate::shape_parser::{parse_shape, ShapeParseError};

    /// Parser for shape-outside property
    /// # Errors
    ///
    /// Returns an error if `input` is not a valid CSS `shape-outside` value.
    pub fn parse_shape_outside(input: &str) -> Result<ShapeOutside, ShapeParseError> {
        let trimmed = input.trim();
        if trimmed == "none" {
            Ok(ShapeOutside::None)
        } else {
            let shape = parse_shape(trimmed)?;
            Ok(ShapeOutside::Shape(shape))
        }
    }

    /// Parser for shape-inside property
    /// # Errors
    ///
    /// Returns an error if `input` is not a valid CSS `shape-inside` value.
    pub fn parse_shape_inside(input: &str) -> Result<ShapeInside, ShapeParseError> {
        let trimmed = input.trim();
        if trimmed == "none" {
            Ok(ShapeInside::None)
        } else {
            let shape = parse_shape(trimmed)?;
            Ok(ShapeInside::Shape(shape))
        }
    }

    /// Parser for clip-path property
    /// # Errors
    ///
    /// Returns an error if `input` is not a valid CSS `clip-path` value.
    pub fn parse_clip_path(input: &str) -> Result<ClipPath, ShapeParseError> {
        let trimmed = input.trim();
        if trimmed == "none" {
            Ok(ClipPath::None)
        } else {
            let shape = parse_shape(trimmed)?;
            Ok(ClipPath::Shape(shape))
        }
    }

    /// Parser for shape-margin property
    /// # Errors
    ///
    /// Returns an error if `input` is not a valid CSS `shape-margin` value.
    pub fn parse_shape_margin(input: &str) -> Result<ShapeMargin, CssPixelValueParseError<'_>> {
        Ok(ShapeMargin {
            inner: parse_pixel_value(input)?,
        })
    }

    /// Parser for shape-image-threshold property
    /// # Errors
    ///
    /// Returns an error if `input` is not a valid CSS `shape-image-threshold` value.
    pub fn parse_shape_image_threshold(
        input: &str,
    ) -> Result<ShapeImageThreshold, ParseFloatError> {
        let val = parse_float_value(input)?;
        // value should be clamped between 0.0 and 1.0
        let clamped = val.get().clamp(0.0, 1.0);
        Ok(ShapeImageThreshold {
            inner: FloatValue::new(clamped),
        })
    }
}

#[cfg(feature = "parser")]
pub use parser::*;

#[cfg(all(test, feature = "parser"))]
mod tests {
    // Tests assert that parsed values equal the exact source literals.
    #![allow(clippy::float_cmp)]
    use super::*;

    #[test]
    fn test_parse_shape_properties() {
        // Test shape-outside
        assert!(matches!(
            parse_shape_outside("none").unwrap(),
            ShapeOutside::None
        ));
        assert!(matches!(
            parse_shape_outside("circle(50px)").unwrap(),
            ShapeOutside::Shape(_)
        ));

        // Test shape-inside
        assert!(matches!(
            parse_shape_inside("none").unwrap(),
            ShapeInside::None
        ));
        assert!(matches!(
            parse_shape_inside("circle(100px at 50px 50px)").unwrap(),
            ShapeInside::Shape(_)
        ));

        // Test clip-path
        assert!(matches!(parse_clip_path("none").unwrap(), ClipPath::None));
        assert!(matches!(
            parse_clip_path("polygon(0 0, 100px 0, 100px 100px, 0 100px)").unwrap(),
            ClipPath::Shape(_)
        ));

        // Test existing properties
        assert_eq!(
            parse_shape_margin("10px").unwrap().inner,
            PixelValue::px(10.0)
        );
        assert_eq!(parse_shape_image_threshold("0.5").unwrap().inner.get(), 0.5);
    }
}

#[cfg(all(test, feature = "parser"))]
mod autotest_generated {
    //! Adversarial tests for the five `shape.rs` parsers.
    //!
    //! Three of these tests are *characterization* tests: they pin down current
    //! behaviour that is a genuine defect in code this module calls into
    //! (`shape_parser` / `props::basic::pixel`). Each is marked `KNOWN BUG`, and
    //! each is written so that it FAILS THE DAY THE BUG IS FIXED, with a message
    //! saying what to replace it with. They are tripwires, not endorsements.

    // float_cmp: parsed values are compared against the exact literals they were
    // built from. eq_op: several tests compare a value with itself on purpose —
    // reflexivity of PartialEq is precisely what is under test.
    #![allow(clippy::float_cmp, clippy::eq_op)]

    use core::{cmp::Ordering, hash::Hash};

    use super::*;
    use crate::{
        corety::OptionF32,
        props::basic::length::SizeMetric,
        shape::{ShapeCircle, ShapeEllipse, ShapeInset, ShapePath, ShapePolygon},
        shape_parser::ShapeParseError,
    };

    /// Inputs that USED to make `shape_parser::parse_path` panic (a lone `"` argument
    /// satisfies both `starts_with('"')` and `ends_with('"')`, so the parser sliced
    /// `[1..0]`). Now fixed — these return `Err`. Kept as a corpus of formerly-panicking
    /// inputs; `path_lone_quote_returns_err_not_panic` asserts the graceful rejection,
    /// and the fuzz guards below tolerate them for free.
    const KNOWN_PANIC_INPUTS: &[&str] = &["path(\")", "path( \" )"];

    fn is_known_panic(input: &str) -> bool {
        KNOWN_PANIC_INPUTS.contains(&input)
    }

    /// Every input the shape-function parsers must survive: malformed, huge,
    /// boundary-numeric and non-ASCII. Includes the known-panic family above so
    /// the corpus stays honest; callers filter it explicitly.
    fn nasty_corpus() -> Vec<String> {
        let mut corpus: Vec<String> = [
            // empty / whitespace (incl. U+00A0, which `str::trim` also strips)
            "",
            " ",
            "      ",
            "\t",
            "\n",
            "\r\n",
            "\t \n ",
            "\u{a0}",
            // keyword handling
            "none",
            " none ",
            "NONE",
            "None",
            "nonee",
            "none none",
            "none;",
            // bare punctuation / unbalanced parens
            "(",
            ")",
            "()",
            ")(",
            "((",
            "))",
            "(()",
            "())",
            "!@#$%^&*",
            ";;;;",
            ",,,,",
            "\0",
            "\0\0(\0)\0",
            "\u{7f}",
            // structurally broken function calls
            "circle",
            "circle(",
            "circle)",
            "circle()",
            "circle( )",
            "circle(50px",
            "circle 50px)",
            "circle(50px))",
            "((circle(50px)))",
            "unknown(50px)",
            "square(1px)",
            "(50px)",
            " (50px) ",
            "circle(;)",
            "circle(,)",
            // leading / trailing junk
            "circle(50px);garbage",
            "circle(50px)garbage",
            "junk circle(50px)",
            "circle(50px) circle(50px)",
            // boundary numbers
            "circle(0)",
            "circle(-0)",
            "circle(0px)",
            "circle(-0px)",
            "circle(-50px)",
            "circle(50)",
            "circle(50%)",
            "circle(NaN)",
            "circle(nan)",
            "circle(inf)",
            "circle(-inf)",
            "circle(infpx)",
            "circle(NaNpx)",
            "circle(1e400px)",
            "circle(-1e400)",
            "circle(9223372036854775807px)",
            "circle(-9223372036854775808)",
            "circle(0x10px)",
            "circle(1_000px)",
            "circle(+5px)",
            "circle(.5px)",
            "circle(5.px)",
            // arity edges
            "circle(50px at)",
            "circle(50px at 1px)",
            "circle(50px at 1px 2px 3px)",
            "circle(50px AT 1px 2px)",
            "ellipse()",
            "ellipse(1px)",
            "ellipse(1px 2px)",
            "ellipse(1px 2px at 3px 4px)",
            "ellipse(a b)",
            "polygon()",
            "polygon(,)",
            "polygon(0 0)",
            "polygon(0 0, 1 1)",
            "polygon(0 0, 1 1, 2 2)",
            "polygon(0 0, 1 1, 2 2,)",
            "polygon(nonzero,)",
            "polygon(nonzero, 0 0, 1 1, 2 2)",
            "polygon(evenodd,0 0,1 1,2 2)",
            "polygon(nonzero 0 0, 1 1, 2 2)",
            "polygon(0, 1, 2)",
            "polygon(x y, x y, x y)",
            "inset()",
            "inset( )",
            "inset(10px)",
            "inset(1px 2px 3px 4px 5px)",
            "inset(round)",
            "inset(round 5px)",
            "inset(10px round)",
            "inset(10px round 5px)",
            "inset(10px round 5px 6px)",
            "inset(roundround)",
            "path()",
            "path(abc)",
            "path(\"\")",
            "path(\"M 0 0 Z\")",
            "path(\"\"\")",
            "path(\"🙂\")",
            // non-ASCII: `parse_function` slices on the byte offsets of '(' and
            // ')', so every multibyte input here is a char-boundary probe.
            "🙂",
            "🙂(1px)",
            "circle(🙂)",
            "circle(🙂px)",
            "circle(1px at 🙂 🙂)",
            "cïrcle(1px)",
            "e\u{0301}(1px)",
            "\u{202e}circle(1px)",
            "circle(١٢٣px)",
            "\u{1f600}\u{1f600}(\u{1f600})",
        ]
        .iter()
        .map(|s| (*s).to_string())
        .collect();

        for panicking in KNOWN_PANIC_INPUTS {
            corpus.push((*panicking).to_string());
        }

        // Pathological sizes: must terminate, must not overflow the stack.
        corpus.push("a".repeat(1_000_000));
        corpus.push("(".repeat(100_000));
        corpus.push(format!("circle({}px)", "9".repeat(10_000)));
        corpus.push("circle(".repeat(10_000) + &")".repeat(10_000));
        corpus.push(format!(
            "polygon({}1px 1px)",
            "1px 1px, ".repeat(10_000)
        ));
        corpus.push(format!("path(\"{}\")", "M 0 0 ".repeat(10_000)));

        corpus
    }

    fn hash_of<T: Hash>(value: &T) -> u64 {
        use core::hash::Hasher;
        use std::collections::hash_map::DefaultHasher;

        let mut hasher = DefaultHasher::new();
        value.hash(&mut hasher);
        hasher.finish()
    }

    fn clip_shape(input: &str) -> CssShape {
        match parse_clip_path(input) {
            Ok(ClipPath::Shape(shape)) => shape,
            other => panic!("expected a shape for {input:?}, got {other:?}"),
        }
    }

    // ---------------------------------------------------------------------
    // KNOWN BUGS — characterization tests (see module docs)
    // ---------------------------------------------------------------------

    /// KNOWN BUG (`shape_parser::parse_path`): a lone `"` as the argument passes
    /// *both* the `starts_with('"')` and `ends_with('"')` guards, so
    /// `&args[1..args.len() - 1]` slices `[1..0]` and panics with
    /// "slice index starts at 1 but ends at 0".
    ///
    /// It is reachable from all three public shape parsers, from untrusted CSS:
    /// `clip-path: path(")`. The correct result is
    /// `Err(ShapeParseError::InvalidSyntax(_))`.
    ///
    /// FIXED: `parse_path` now requires `args.len() >= 2` before slicing, so a lone
    /// `"` argument returns `Err(InvalidSyntax)` instead of panicking on the reversed
    /// `[1..0]` slice. These inputs are reachable from untrusted CSS (`clip-path:
    /// path(")`), so the graceful-rejection guarantee matters.
    #[test]
    fn path_lone_quote_returns_err_not_panic() {
        for input in KNOWN_PANIC_INPUTS {
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                parse_clip_path(input)
            }));
            match outcome {
                Ok(res) => assert!(res.is_err(), "{input:?} must be rejected, got {res:?}"),
                Err(_) => panic!("parse_clip_path({input:?}) still panics — the slice bug regressed"),
            }
        }
    }

    /// KNOWN BUG (`props::basic::pixel::parse_pixel_value`): the metric table is
    /// scanned in order and tries `("in", In)` *before* `("vmin", Vmin)`. Since
    /// "vmin" ends with "in", `10vmin` strips to `10vm`, which fails to parse as
    /// an f32 — so the valid CSS unit `vmin` is rejected outright. `vmax`, `vw`
    /// and `vh` are unaffected (no earlier metric is a suffix of them).
    ///
    /// This is not shape-specific: every property that routes through
    /// `parse_pixel_value` (width, margin, padding, …) rejects `vmin` too.
    ///
    /// WHEN pixel.rs IS FIXED (match longest metric first, or move vmax/vmin
    /// ahead of "in"), this test fails — replace it with:
    ///     `assert_eq!(parse_shape_margin("10vmin").unwrap().inner.metric`, `SizeMetric::Vmin`);
    #[test]
    fn known_bug_vmin_unit_is_rejected_by_metric_table_order() {
        // FIXED (as this pin's own message instructed): "10vmin" now parses to Vmin.
        assert_eq!(
            parse_shape_margin("10vmin").unwrap().inner.metric,
            SizeMetric::Vmin
        );

        // The sibling viewport units do work, which is what makes the bug easy
        // to miss: only the unit that *ends in an earlier metric* is broken.
        assert_eq!(
            parse_shape_margin("10vmax").unwrap().inner.metric,
            SizeMetric::Vmax
        );
        assert_eq!(
            parse_shape_margin("10vw").unwrap().inner.metric,
            SizeMetric::Vw
        );
        assert_eq!(
            parse_shape_margin("10vh").unwrap().inner.metric,
            SizeMetric::Vh
        );
    }

    /// A NaN f32 in a shape used to break the `Eq`/`Ord` contracts: the property
    /// enums derived `PartialEq` (raw compare, NaN != NaN) while hand-writing
    /// `Ord`/`Hash` as NaN-Equal (`to_bits`), so `a == a` was false yet `cmp` said
    /// `Equal`. Fixed: `PartialEq` is now hand-written to match `Ord`, so a
    /// preserved NaN length stays reflexive and consistent with Hash/Ord.
    #[test]
    fn nan_shape_is_reflexive_and_consistent_across_eq_ord_hash() {
        let a = parse_clip_path("circle(NaN)").expect("NaN is a preserved length");
        let b = parse_clip_path("circle(NaN)").expect("NaN is a preserved length");
        assert_eq!(a, a, "Eq must be reflexive for a NaN shape");
        assert_eq!(a, b);
        assert_eq!(a.cmp(&b), Ordering::Equal);
        assert_eq!(a.partial_cmp(&b), Some(Ordering::Equal));
        assert_eq!(hash_of(&a), hash_of(&b));
    }

    /// Contrast with the bug above: `ShapeMargin` / `ShapeImageThreshold` store
    /// their numbers as `FloatValue` (an isize-encoded fixed-point), and the
    /// f32 -> isize cast maps NaN to 0. So no NaN can survive into these two
    /// types and their derived `Eq` really is reflexive.
    #[test]
    fn floatvalue_encoding_makes_margin_and_threshold_nan_free() {
        let threshold = parse_shape_image_threshold("NaN").expect("f32 parses NaN");
        assert!(threshold.inner.get().is_finite());
        assert_eq!(threshold.inner.get(), 0.0);
        assert_eq!(threshold, threshold);
        assert_eq!(hash_of(&threshold), hash_of(&threshold));

        // "NaNpx" is *accepted* (leniency worth tightening) but cannot produce a
        // NaN PixelValue. Written to also pass once the parser rejects it.
        if let Ok(margin) = parse_shape_margin("NaNpx") {
            assert!(
                margin.inner.number.get().is_finite(),
                "NaN leaked into a PixelValue"
            );
            assert_eq!(margin.inner.number.get(), 0.0);
            assert_eq!(margin, margin);
        } else { /* rejecting "NaNpx" outright would be more correct */ }
    }

    // ---------------------------------------------------------------------
    // Panic / hang safety across the whole corpus
    // ---------------------------------------------------------------------

    /// The headline invariant: no input may panic any shape-function parser.
    ///
    /// Written as "the set of panicking inputs is a subset of the known-bug set",
    /// so it keeps passing (and keeps guarding) after `parse_path` is fixed.
    #[test]
    fn shape_parsers_never_panic_on_hostile_input() {
        let mut unexpected: Vec<String> = Vec::new();

        for input in nasty_corpus() {
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _ = parse_shape_outside(&input);
                let _ = parse_shape_inside(&input);
                let _ = parse_clip_path(&input);
            }));

            if outcome.is_err() && !is_known_panic(&input) {
                let preview: String = input.chars().take(48).collect();
                unexpected.push(preview);
            }
        }

        assert!(
            unexpected.is_empty(),
            "shape parsers panicked on input(s) outside the known-bug set: \
             {unexpected:?}"
        );
    }

    /// Same invariant for the two numeric parsers — these have no known panics,
    /// so the bar is absolute.
    #[test]
    fn numeric_parsers_never_panic_on_hostile_input() {
        for input in nasty_corpus() {
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _ = parse_shape_margin(&input);
                let _ = parse_shape_image_threshold(&input);
            }));
            assert!(
                outcome.is_ok(),
                "numeric parser panicked on {:?}",
                input.chars().take(48).collect::<String>()
            );
        }
    }

    /// All three properties delegate to the same `parse_shape`, so they must
    /// accept exactly the same language and produce the same shape. Compared via
    /// `Ord` rather than `==` because NaN shapes are not self-equal (see
    /// `known_bug_nan_shape_breaks_eq_ord_consistency`).
    #[test]
    fn the_three_shape_properties_agree_on_every_input() {
        for input in nasty_corpus() {
            if is_known_panic(&input) {
                continue;
            }

            let outside = parse_shape_outside(&input);
            let inside = parse_shape_inside(&input);
            let clip = parse_clip_path(&input);

            assert_eq!(
                outside.is_ok(),
                inside.is_ok(),
                "shape-outside and shape-inside disagree on {input:?}"
            );
            assert_eq!(
                outside.is_ok(),
                clip.is_ok(),
                "shape-outside and clip-path disagree on {input:?}"
            );

            if let (Ok(ShapeOutside::Shape(a)), Ok(ShapeInside::Shape(b)), Ok(ClipPath::Shape(c))) =
                (&outside, &inside, &clip)
            {
                assert_eq!(a.cmp(b), Ordering::Equal, "different shape for {input:?}");
                assert_eq!(a.cmp(c), Ordering::Equal, "different shape for {input:?}");
            }
        }
    }

    /// A million-character input, a 10 000-deep paren nest and a 10 000-point
    /// polygon must all terminate. `parse_shape` is iterative, so nesting must
    /// not grow the stack; if this ever hangs or overflows, the test never
    /// returns and the suite times out rather than passing silently.
    #[test]
    fn pathological_sizes_terminate_without_stack_overflow() {
        let million = "a".repeat(1_000_000);
        assert!(matches!(
            parse_clip_path(&million),
            Err(ShapeParseError::InvalidSyntax(_))
        ));

        let nested = "circle(".repeat(10_000) + &")".repeat(10_000);
        assert!(parse_clip_path(&nested).is_err());

        let unclosed = "(".repeat(100_000);
        assert!(matches!(
            parse_clip_path(&unclosed),
            Err(ShapeParseError::InvalidSyntax(_))
        ));

        let big_polygon = format!("polygon({}1px 1px)", "1px 1px, ".repeat(10_000));
        match clip_shape(&big_polygon) {
            CssShape::Polygon(ShapePolygon { points }) => {
                assert_eq!(points.as_ref().len(), 10_001);
            }
            other => panic!("expected Polygon, got {other:?}"),
        }

        // 10 000 digits overflow f32 to infinity rather than erroring — the
        // shape keeps a raw f32, so this is where a non-finite radius gets in.
        let huge_radius = format!("circle({}px)", "9".repeat(10_000));
        match clip_shape(&huge_radius) {
            CssShape::Circle(ShapeCircle { radius, .. }) => {
                assert!(radius.is_infinite() && radius.is_sign_positive());
            }
            other => panic!("expected Circle, got {other:?}"),
        }
    }

    // ---------------------------------------------------------------------
    // parse_shape_outside / parse_shape_inside / parse_clip_path
    // ---------------------------------------------------------------------

    #[test]
    fn empty_and_whitespace_only_input_is_empty_input_error() {
        for input in ["", " ", "      ", "\t", "\n", "\r\n", "\t \n ", "\u{a0}"] {
            assert_eq!(
                parse_shape_outside(input),
                Err(ShapeParseError::EmptyInput),
                "shape-outside {input:?}"
            );
            assert_eq!(
                parse_shape_inside(input),
                Err(ShapeParseError::EmptyInput),
                "shape-inside {input:?}"
            );
            assert_eq!(
                parse_clip_path(input),
                Err(ShapeParseError::EmptyInput),
                "clip-path {input:?}"
            );
        }
    }

    /// Positive control, plus the one piece of trimming the parsers do promise.
    #[test]
    fn none_keyword_parses_and_is_trimmed() {
        for input in ["none", " none ", "\tnone\n", "  none\r\n"] {
            assert_eq!(parse_shape_outside(input), Ok(ShapeOutside::None));
            assert_eq!(parse_shape_inside(input), Ok(ShapeInside::None));
            assert_eq!(parse_clip_path(input), Ok(ClipPath::None));
        }
    }

    /// `none` is matched case-sensitively, which is not CSS-conformant (CSS
    /// keywords are ASCII case-insensitive). Asserted as an invariant that holds
    /// either way — uppercase must never silently become a *shape*.
    #[test]
    fn uppercase_none_never_yields_a_shape() {
        for input in ["NONE", "None", "nOnE"] {
            match parse_clip_path(input) {
                Ok(ClipPath::None) | Err(_) => {}
                Ok(other) => panic!("{input:?} parsed as a shape: {other:?}"),
            }
        }
        // Current behaviour: rejected as an unparseable function.
        assert!(parse_clip_path("NONE").is_err());
    }

    #[test]
    fn garbage_and_broken_parens_are_rejected() {
        // No '(' at all.
        for input in ["!@#$%^&*", ";;;;", ",,,,", "circle", "\u{7f}", "🙂"] {
            assert!(
                matches!(
                    parse_clip_path(input),
                    Err(ShapeParseError::InvalidSyntax(_))
                ),
                "expected InvalidSyntax for {input:?}"
            );
        }

        // '(' but no ')'.
        assert!(matches!(
            parse_clip_path("circle(50px"),
            Err(ShapeParseError::InvalidSyntax(_))
        ));
        // ')' before '('.
        assert!(matches!(
            parse_clip_path(")("),
            Err(ShapeParseError::InvalidSyntax(_))
        ));
        // Empty function name.
        assert!(matches!(
            parse_clip_path("()"),
            Err(ShapeParseError::UnknownFunction(_))
        ));
        // Unknown function names carry the offending name.
        assert_eq!(
            parse_clip_path("square(1px)"),
            Err(ShapeParseError::UnknownFunction("square".to_string()))
        );
        assert_eq!(
            parse_clip_path("junk circle(50px)"),
            Err(ShapeParseError::UnknownFunction("junk circle".to_string()))
        );
    }

    /// Trailing junk after the closing paren is currently *ignored*:
    /// `parse_function` takes `rfind(')')` and never checks that the input ends
    /// there, so `circle(50px);garbage` parses as a plain circle. That is a
    /// leniency bug (a declaration with trailing garbage should be dropped), but
    /// it is not a memory-safety issue.
    ///
    /// Asserted as the invariant that must hold either way: the parser may
    /// reject the input, but it must never return a *different* shape than the
    /// prefix describes.
    #[test]
    fn trailing_junk_is_ignored_but_never_changes_the_shape() {
        for input in [
            "circle(50px);garbage",
            "circle(50px)garbage",
            "circle(50px)🙂",
        ] {
            match parse_clip_path(input) {
                Ok(ClipPath::Shape(CssShape::Circle(ShapeCircle { center, radius }))) => {
                    assert_eq!(radius, 50.0, "{input:?}");
                    assert_eq!(center.x, 0.0);
                    assert_eq!(center.y, 0.0);
                }
                Err(_) => { /* rejecting trailing junk would be more correct */ }
                other => panic!("{input:?} produced an unexpected value: {other:?}"),
            }
        }
    }

    /// `parse_function` slices `input` at the *byte* offsets of '(' and ')'.
    /// Those are ASCII, so they can never land inside a multibyte sequence — but
    /// only if nothing else slices. These probe that.
    #[test]
    fn multibyte_input_does_not_panic_and_is_rejected() {
        for input in [
            "🙂(1px)",
            "circle(🙂)",
            "circle(🙂px)",
            "circle(1px at 🙂 🙂)",
            "cïrcle(1px)",
            "e\u{0301}(1px)",
            "\u{202e}circle(1px)",
            "circle(١٢٣px)",
            "\u{1f600}\u{1f600}(\u{1f600})",
        ] {
            assert!(
                parse_clip_path(input).is_err(),
                "expected Err for {input:?}"
            );
        }

        // Multibyte *inside* a quoted path is data, and is preserved verbatim.
        match clip_shape("path(\"🙂\")") {
            CssShape::Path(ShapePath { data }) => assert_eq!(data.as_str(), "🙂"),
            other => panic!("expected Path, got {other:?}"),
        }
    }

    #[test]
    fn circle_boundary_numbers() {
        // Unitless and `%` are both accepted; `%` is silently treated as a raw
        // number (parse_length has a TODO — it needs the container size).
        for input in ["circle(50px)", "circle(50)", "circle(50%)"] {
            match clip_shape(input) {
                CssShape::Circle(ShapeCircle { radius, .. }) => assert_eq!(radius, 50.0),
                other => panic!("expected Circle, got {other:?}"),
            }
        }

        // Zero, signed zero, and negative radii are all accepted. A negative
        // radius is invalid per CSS Shapes; it is stored as-is.
        for (input, expected) in [
            ("circle(0px)", 0.0_f32),
            ("circle(-0px)", -0.0_f32),
            ("circle(-50px)", -50.0_f32),
            ("circle(+5px)", 5.0_f32),
            ("circle(.5px)", 0.5_f32),
            ("circle(5.px)", 5.0_f32),
        ] {
            match clip_shape(input) {
                CssShape::Circle(ShapeCircle { radius, .. }) => {
                    assert_eq!(radius, expected, "{input:?}");
                }
                other => panic!("expected Circle, got {other:?}"),
            }
        }

        // f32 overflow saturates to infinity instead of erroring.
        for input in ["circle(1e400px)", "circle(inf)", "circle(infpx)"] {
            match clip_shape(input) {
                CssShape::Circle(ShapeCircle { radius, .. }) => {
                    assert!(radius.is_infinite(), "{input:?} -> {radius}");
                }
                other => panic!("expected Circle, got {other:?}"),
            }
        }

        // i64::MAX survives as an f32 approximation, no overflow panic.
        match clip_shape("circle(9223372036854775807px)") {
            CssShape::Circle(ShapeCircle { radius, .. }) => {
                assert!(radius.is_finite() && radius > 9.0e18);
            }
            other => panic!("expected Circle, got {other:?}"),
        }

        // Rust-only / C-only numeric literals are NOT valid CSS numbers.
        for input in ["circle(0x10px)", "circle(1_000px)"] {
            assert!(
                matches!(
                    parse_clip_path(input),
                    Err(ShapeParseError::InvalidNumber(_))
                ),
                "expected InvalidNumber for {input:?}"
            );
        }
    }

    /// `circle()` needs 4 parts *and* `parts[1] == "at"` before it reads a
    /// center; anything else silently falls back to the origin rather than
    /// erroring. Pin that down — a partial `at` clause is not a parse error.
    #[test]
    fn circle_at_clause_arity_falls_back_to_origin() {
        for input in [
            "circle(50px at)",
            "circle(50px at 1px)",
            "circle(50px AT 1px 2px)",
        ] {
            match clip_shape(input) {
                CssShape::Circle(ShapeCircle { center, radius }) => {
                    assert_eq!(radius, 50.0);
                    assert_eq!((center.x, center.y), (0.0, 0.0), "{input:?}");
                }
                other => panic!("expected Circle, got {other:?}"),
            }
        }

        // A complete `at` clause is honoured; extra trailing parts are ignored.
        for input in ["circle(50px at 1px 2px)", "circle(50px at 1px 2px 3px)"] {
            match clip_shape(input) {
                CssShape::Circle(ShapeCircle { center, radius }) => {
                    assert_eq!(radius, 50.0);
                    assert_eq!((center.x, center.y), (1.0, 2.0), "{input:?}");
                }
                other => panic!("expected Circle, got {other:?}"),
            }
        }

        assert!(matches!(
            parse_clip_path("circle()"),
            Err(ShapeParseError::MissingParameter(_))
        ));
    }

    #[test]
    fn ellipse_requires_two_radii() {
        for input in ["ellipse()", "ellipse(1px)"] {
            assert!(
                matches!(
                    parse_clip_path(input),
                    Err(ShapeParseError::MissingParameter(_))
                ),
                "expected MissingParameter for {input:?}"
            );
        }

        match clip_shape("ellipse(1px 2px)") {
            CssShape::Ellipse(ShapeEllipse {
                center,
                radius_x,
                radius_y,
            }) => {
                assert_eq!((radius_x, radius_y), (1.0, 2.0));
                assert_eq!((center.x, center.y), (0.0, 0.0));
            }
            other => panic!("expected Ellipse, got {other:?}"),
        }

        match clip_shape("ellipse(1px 2px at 3px 4px)") {
            CssShape::Ellipse(ShapeEllipse {
                center,
                radius_x,
                radius_y,
            }) => {
                assert_eq!((radius_x, radius_y), (1.0, 2.0));
                assert_eq!((center.x, center.y), (3.0, 4.0));
            }
            other => panic!("expected Ellipse, got {other:?}"),
        }

        assert!(matches!(
            parse_clip_path("ellipse(a b)"),
            Err(ShapeParseError::InvalidNumber(_))
        ));
    }

    #[test]
    fn polygon_needs_three_complete_points() {
        // Fewer than 3 points, empty args, and a trailing comma all fail.
        for input in [
            "polygon()",
            "polygon(,)",
            "polygon(0 0)",
            "polygon(0 0, 1 1)",
            "polygon(0 0, 1 1, 2 2,)",
            "polygon(0, 1, 2)",
            "polygon(nonzero,)",
        ] {
            assert!(
                parse_clip_path(input).is_err(),
                "expected Err for {input:?}"
            );
        }

        match clip_shape("polygon(0 0, 1 1, 2 2)") {
            CssShape::Polygon(ShapePolygon { points }) => {
                assert_eq!(points.as_ref().len(), 3);
                assert_eq!((points.as_ref()[2].x, points.as_ref()[2].y), (2.0, 2.0));
            }
            other => panic!("expected Polygon, got {other:?}"),
        }

        // The optional fill-rule prefix is accepted (and ignored) only when it
        // is immediately followed by a comma.
        for input in [
            "polygon(nonzero, 0 0, 1 1, 2 2)",
            "polygon(evenodd,0 0,1 1,2 2)",
        ] {
            match clip_shape(input) {
                CssShape::Polygon(ShapePolygon { points }) => {
                    assert_eq!(points.as_ref().len(), 3, "{input:?}");
                }
                other => panic!("expected Polygon, got {other:?}"),
            }
        }
        assert!(matches!(
            parse_clip_path("polygon(nonzero 0 0, 1 1, 2 2)"),
            Err(ShapeParseError::InvalidNumber(_))
        ));
        assert!(matches!(
            parse_clip_path("polygon(x y, x y, x y)"),
            Err(ShapeParseError::InvalidNumber(_))
        ));
    }

    #[test]
    fn inset_shorthand_and_round_keyword() {
        // 1/2/3/4-value shorthand, same rules as margin/padding.
        let cases: [(&str, [f32; 4]); 4] = [
            ("inset(10px)", [10.0, 10.0, 10.0, 10.0]),
            ("inset(1px 2px)", [1.0, 2.0, 1.0, 2.0]),
            ("inset(1px 2px 3px)", [1.0, 2.0, 3.0, 2.0]),
            ("inset(1px 2px 3px 4px)", [1.0, 2.0, 3.0, 4.0]),
        ];
        for (input, [top, right, bottom, left]) in cases {
            match clip_shape(input) {
                CssShape::Inset(ShapeInset {
                    inset_top,
                    inset_right,
                    inset_bottom,
                    inset_left,
                    border_radius,
                }) => {
                    assert_eq!(
                        [inset_top, inset_right, inset_bottom, inset_left],
                        [top, right, bottom, left],
                        "{input:?}"
                    );
                    assert!(matches!(border_radius, OptionF32::None));
                }
                other => panic!("expected Inset, got {other:?}"),
            }
        }

        // 5 values is rejected; no values is rejected.
        assert!(matches!(
            parse_clip_path("inset(1px 2px 3px 4px 5px)"),
            Err(ShapeParseError::InvalidSyntax(_))
        ));
        assert!(matches!(
            parse_clip_path("inset()"),
            Err(ShapeParseError::MissingParameter(_))
        ));
        assert!(matches!(
            parse_clip_path("inset( )"),
            Err(ShapeParseError::MissingParameter(_))
        ));

        // `round` with a missing / unparseable radius errors rather than
        // panicking on the `args[round_pos + 5..]` slice.
        for input in [
            "inset(round)",
            "inset(10px round)",
            "inset(roundround)",
            "inset(10px round 5px 6px)",
        ] {
            assert!(
                matches!(
                    parse_clip_path(input),
                    Err(ShapeParseError::InvalidNumber(_))
                ),
                "expected InvalidNumber for {input:?}"
            );
        }

        match clip_shape("inset(10px round 5px)") {
            CssShape::Inset(ShapeInset { border_radius, .. }) => {
                assert!(matches!(border_radius, OptionF32::Some(r) if r == 5.0));
            }
            other => panic!("expected Inset, got {other:?}"),
        }
    }

    #[test]
    fn path_data_must_be_quoted_and_is_stored_verbatim() {
        // Unquoted / half-quoted path data is rejected.
        for input in ["path()", "path(abc)", "path(\"abc)", "path(abc\")"] {
            assert!(
                matches!(
                    parse_clip_path(input),
                    Err(ShapeParseError::InvalidSyntax(_))
                ),
                "expected InvalidSyntax for {input:?}"
            );
        }

        // An empty quoted path is valid and yields empty data (this is the
        // len == 2 neighbour of the len == 1 panic in the known-bug test).
        match clip_shape("path(\"\")") {
            CssShape::Path(ShapePath { data }) => assert_eq!(data.as_str(), ""),
            other => panic!("expected Path, got {other:?}"),
        }

        // Path data is never interpreted, so it can contain anything — including
        // the parens that `parse_function` scans for, thanks to rfind(')').
        match clip_shape("path(\"M 0 0 (L) 1 1 Z\")") {
            CssShape::Path(ShapePath { data }) => {
                assert_eq!(data.as_str(), "M 0 0 (L) 1 1 Z");
            }
            other => panic!("expected Path, got {other:?}"),
        }
    }

    // ---------------------------------------------------------------------
    // Round-trip: print_as_css_value -> parse -> identical value
    // ---------------------------------------------------------------------

    #[test]
    fn shape_properties_round_trip_through_their_css_representation() {
        let inputs = [
            "none",
            "circle(50px)",
            "circle(50px at 10px 20px)",
            "circle(-1px at -2px -3px)",
            "ellipse(1px 2px)",
            "ellipse(1px 2px at 3px 4px)",
            "polygon(0px 0px, 100px 0px, 100px 100px)",
            "polygon(0px 0px, 1px 1px, 2px 2px, 3px 3px, 4px 4px)",
            "inset(1px 2px 3px 4px)",
            "inset(10px round 5px)",
            "path(\"M 0 0 L 1 1 Z\")",
        ];

        for input in inputs {
            let outside = parse_shape_outside(input).expect(input);
            let printed = outside.print_as_css_value();
            assert_eq!(
                parse_shape_outside(&printed).as_ref(),
                Ok(&outside),
                "shape-outside round-trip changed the value: {input:?} -> {printed:?}"
            );
            // Printing must also be idempotent, not just re-parseable.
            assert_eq!(
                parse_shape_outside(&printed).expect(input).print_as_css_value(),
                printed
            );

            let inside = parse_shape_inside(input).expect(input);
            let printed = inside.print_as_css_value();
            assert_eq!(parse_shape_inside(&printed).as_ref(), Ok(&inside), "{input:?}");

            let clip = parse_clip_path(input).expect(input);
            let printed = clip.print_as_css_value();
            assert_eq!(parse_clip_path(&printed).as_ref(), Ok(&clip), "{input:?}");
        }
    }

    #[test]
    fn none_prints_as_the_none_keyword() {
        assert_eq!(ShapeOutside::None.print_as_css_value(), "none");
        assert_eq!(ShapeInside::None.print_as_css_value(), "none");
        assert_eq!(ClipPath::None.print_as_css_value(), "none");
    }

    #[test]
    fn margin_and_threshold_round_trip() {
        for input in ["0px", "10px", "-5px", "1.5em", "50%", "2rem", "12pt", "1in"] {
            let margin = parse_shape_margin(input).expect(input);
            let printed = margin.print_as_css_value();
            assert_eq!(
                parse_shape_margin(&printed).expect(&printed),
                margin,
                "shape-margin round-trip changed the value: {input:?} -> {printed:?}"
            );
        }

        for input in ["0", "0.5", "1", "0.001", "0.999"] {
            let threshold = parse_shape_image_threshold(input).expect(input);
            let printed = threshold.print_as_css_value();
            assert_eq!(
                parse_shape_image_threshold(&printed).expect(&printed),
                threshold,
                "shape-image-threshold round-trip changed the value: {input:?} -> {printed:?}"
            );
        }
    }

    // ---------------------------------------------------------------------
    // parse_shape_margin
    // ---------------------------------------------------------------------

    #[test]
    fn margin_empty_and_whitespace_only_is_empty_string_error() {
        for input in ["", " ", "     ", "\t", "\n", "\r\n", "\u{a0}"] {
            assert!(
                matches!(
                    parse_shape_margin(input),
                    Err(CssPixelValueParseError::EmptyString)
                ),
                "expected EmptyString for {input:?}"
            );
        }
    }

    #[test]
    fn margin_valid_units_map_to_the_right_metric() {
        // NOTE: `vmin` is missing here on purpose — it is broken. See
        // `known_bug_vmin_unit_is_rejected_by_metric_table_order`.
        let cases = [
            ("10px", SizeMetric::Px),
            ("10em", SizeMetric::Em),
            ("10rem", SizeMetric::Rem),
            ("10pt", SizeMetric::Pt),
            ("10in", SizeMetric::In),
            ("10cm", SizeMetric::Cm),
            ("10mm", SizeMetric::Mm),
            ("10%", SizeMetric::Percent),
            ("10vw", SizeMetric::Vw),
            ("10vh", SizeMetric::Vh),
            ("10vmax", SizeMetric::Vmax),
            // Unitless numbers are accepted and default to px.
            ("10", SizeMetric::Px),
        ];

        for (input, metric) in cases {
            let margin = parse_shape_margin(input).expect(input);
            assert_eq!(margin.inner.metric, metric, "{input:?}");
            assert_eq!(margin.inner.number.get(), 10.0, "{input:?}");
        }

        // Whitespace around the value and between number and unit is tolerated.
        assert_eq!(
            parse_shape_margin("  10px  ").expect("padded").inner,
            PixelValue::px(10.0)
        );
        assert_eq!(
            parse_shape_margin("10 px").expect("inner space").inner,
            PixelValue::px(10.0)
        );
    }

    #[test]
    fn margin_rejects_malformed_and_junk_suffixed_values() {
        // A unit with no number.
        assert!(matches!(
            parse_shape_margin("px"),
            Err(CssPixelValueParseError::NoValueGiven(_, SizeMetric::Px))
        ));
        // A number with a bad number part in front of a known unit.
        assert!(matches!(
            parse_shape_margin("abcpx"),
            Err(CssPixelValueParseError::ValueParseErr(_, "abc"))
        ));
        // Neither a known unit nor a bare number.
        for input in [
            "10px;garbage",
            "garbage",
            "10 20px",
            "10px 20px",
            "10PX",
            "10Px",
            "10px🙂",
            "🙂px",
            "!@#$",
        ] {
            assert!(
                parse_shape_margin(input).is_err(),
                "expected Err for {input:?}"
            );
        }
    }

    /// Unit matching is ASCII-case-sensitive, which is not CSS-conformant.
    /// Asserted so it holds either way: uppercase must never yield a *wrong*
    /// value, only the right one or an error.
    #[test]
    fn margin_uppercase_units_never_yield_a_wrong_value() {
        for input in ["10PX", "10Px", "10EM", "10%"] {
            if let Ok(margin) = parse_shape_margin(input) { assert_eq!(margin.inner.number.get(), 10.0, "{input:?}") } else { /* current behaviour for the uppercase forms */ }
        }
    }

    /// The `FloatValue` isize encoding is the only thing standing between a
    /// hostile stylesheet and a non-finite length in layout. Nothing that parses
    /// may produce a NaN or infinite `PixelValue`.
    #[test]
    fn margin_never_produces_a_non_finite_value() {
        let extremes = [
            "0px",
            "-0px",
            "0",
            "-0",
            "NaNpx",
            "nanpx",
            "infpx",
            "-infpx",
            "inf",
            "-inf",
            "1e400px",
            "-1e400px",
            "1e38px",
            "-1e38px",
            "9223372036854775807px",
            "-9223372036854775808px",
            "340282350000000000000000000000000000000px",
            "0.0000000000000000001px",
        ];

        for input in extremes {
            if let Ok(margin) = parse_shape_margin(input) {
                let value = margin.inner.number.get();
                assert!(
                    value.is_finite(),
                    "{input:?} produced a non-finite PixelValue: {value}"
                );
            }
        }

        // Saturation, specifically: `1e400` parses to f32::INFINITY (Rust's f32
        // FromStr does not error on overflow), and the `f32 as isize` cast in
        // FloatValue::new then saturates at the isize bound — which is exactly
        // what keeps the infinity from reaching layout.
        let saturated = parse_shape_margin("1e400px").expect("f32 overflow parses as inf");
        assert_eq!(saturated.inner.number.number(), isize::MAX);
        assert!(saturated.inner.number.get().is_finite());

        let saturated_neg = parse_shape_margin("-1e400px").expect("parses as -inf");
        assert_eq!(saturated_neg.inner.number.number(), isize::MIN);
        assert!(saturated_neg.inner.number.get().is_finite());

        // NaN saturates to 0 rather than to a bound.
        assert_eq!(
            parse_shape_margin("NaNpx")
                .expect("NaN currently parses")
                .inner
                .number
                .number(),
            0
        );
    }

    /// `FloatValue` keeps 3 decimal places and *truncates* toward zero — sizes
    /// below 0.001 collapse to exactly 0. Worth pinning: it silently changes
    /// authored values.
    #[test]
    fn margin_quantizes_to_three_decimals_by_truncation() {
        assert_eq!(
            parse_shape_margin("0.001px").expect("0.001").inner.number.get(),
            0.001
        );
        // 0.0005 does NOT round up to 0.001 — it truncates to 0.
        assert_eq!(
            parse_shape_margin("0.0005px").expect("0.0005").inner.number.get(),
            0.0
        );
        assert_eq!(
            parse_shape_margin("0.0009px").expect("0.0009").inner.number.get(),
            0.0
        );

        let truncated = parse_shape_margin("1.9999px").expect("1.9999").inner.number.get();
        assert!(
            (truncated - 1.999).abs() < 1.0e-6,
            "expected truncation to 1.999, got {truncated}"
        );
    }

    // ---------------------------------------------------------------------
    // parse_shape_image_threshold
    // ---------------------------------------------------------------------

    #[test]
    fn threshold_empty_whitespace_and_garbage_are_errors() {
        for input in [
            "", " ", "   ", "\t\n", "abc", "0.5px", "50%", "1,0", "0.5.5", "🙂", "--1",
        ] {
            assert!(
                parse_shape_image_threshold(input).is_err(),
                "expected Err for {input:?}"
            );
        }
    }

    #[test]
    fn threshold_parses_and_trims_valid_values() {
        assert_eq!(parse_shape_image_threshold("0").expect("0").inner.get(), 0.0);
        assert_eq!(parse_shape_image_threshold("1").expect("1").inner.get(), 1.0);
        assert_eq!(
            parse_shape_image_threshold("0.5").expect("0.5").inner.get(),
            0.5
        );
        assert_eq!(
            parse_shape_image_threshold("  0.5  ")
                .expect("padded")
                .inner
                .get(),
            0.5
        );
        // f32 accepts these spellings; CSS numbers do too.
        assert_eq!(
            parse_shape_image_threshold("+0.5").expect("+0.5").inner.get(),
            0.5
        );
        assert_eq!(
            parse_shape_image_threshold("5e-1").expect("5e-1").inner.get(),
            0.5
        );
        assert_eq!(
            parse_shape_image_threshold(".5").expect(".5").inner.get(),
            0.5
        );
    }

    /// The documented contract: the result is clamped to `0.0 ..= 1.0`. Assert it
    /// as a hard invariant over every input that parses at all — including the
    /// ones that reach `clamp` as infinities.
    #[test]
    fn threshold_is_always_clamped_to_zero_one_and_finite() {
        let extremes = [
            "0", "-0", "1", "-1", "2", "1.0001", "-0.0001", "100", "1e10", "-1e10", "1e38",
            "1e400", "-1e400", "inf", "-inf", "infinity", "-infinity", "NaN", "nan", "-NaN",
            "9223372036854775807", "-9223372036854775808", "1e-45", "-1e-45", "0.0000001",
        ];

        for input in extremes {
            let Ok(threshold) = parse_shape_image_threshold(input) else {
                continue;
            };
            let value = threshold.inner.get();
            assert!(
                value.is_finite(),
                "{input:?} produced a non-finite threshold: {value}"
            );
            assert!(
                (0.0..=1.0).contains(&value),
                "{input:?} escaped the [0, 1] clamp: {value}"
            );
        }

        // Direction of the clamp, specifically.
        assert_eq!(parse_shape_image_threshold("2").expect("2").inner.get(), 1.0);
        assert_eq!(
            parse_shape_image_threshold("-1").expect("-1").inner.get(),
            0.0
        );
        assert_eq!(
            parse_shape_image_threshold("inf").expect("inf").inner.get(),
            1.0
        );
        assert_eq!(
            parse_shape_image_threshold("-inf").expect("-inf").inner.get(),
            0.0
        );
        // NaN is neutralised by the isize encoding *before* it reaches clamp
        // (`f32 as isize` maps NaN to 0), so it lands on 0.0 rather than
        // propagating or panicking.
        assert_eq!(
            parse_shape_image_threshold("NaN").expect("NaN").inner.get(),
            0.0
        );
    }

    /// Same 0.001 truncation as `ShapeMargin`: a threshold below 0.001 becomes a
    /// fully transparent 0.
    #[test]
    fn threshold_quantizes_to_three_decimals() {
        assert_eq!(
            parse_shape_image_threshold("0.001")
                .expect("0.001")
                .inner
                .get(),
            0.001
        );
        assert_eq!(
            parse_shape_image_threshold("0.0005")
                .expect("0.0005")
                .inner
                .get(),
            0.0
        );
        let truncated = parse_shape_image_threshold("0.9999")
            .expect("0.9999")
            .inner
            .get();
        assert!(
            (truncated - 0.999).abs() < 1.0e-6,
            "expected truncation to 0.999, got {truncated}"
        );
    }

    #[test]
    fn threshold_survives_a_ten_thousand_digit_number() {
        let huge = "9".repeat(10_000);
        assert_eq!(
            parse_shape_image_threshold(&huge)
                .expect("overflows to inf, clamps to 1")
                .inner
                .get(),
            1.0
        );

        let tiny = format!("0.{}1", "0".repeat(10_000));
        assert_eq!(
            parse_shape_image_threshold(&tiny)
                .expect("underflows to 0")
                .inner
                .get(),
            0.0
        );
    }

    // ---------------------------------------------------------------------
    // Type invariants: Default, Ord/PartialOrd agreement, Hash
    // ---------------------------------------------------------------------

    #[test]
    fn defaults_are_none_and_zero() {
        assert_eq!(ShapeOutside::default(), ShapeOutside::None);
        assert_eq!(ShapeInside::default(), ShapeInside::None);
        assert_eq!(ClipPath::default(), ClipPath::None);
        assert_eq!(ShapeMargin::default().inner, PixelValue::zero());
        assert_eq!(ShapeMargin::default().inner.number.get(), 0.0);
        assert_eq!(ShapeImageThreshold::default().inner.get(), 0.0);

        // The defaults are exactly what the minimal CSS text parses to.
        assert_eq!(parse_clip_path("none").expect("none"), ClipPath::default());
        assert_eq!(
            parse_shape_margin("0px").expect("0px"),
            ShapeMargin::default()
        );
        assert_eq!(
            parse_shape_image_threshold("0").expect("0"),
            ShapeImageThreshold::default()
        );
    }

    /// `None` sorts before any shape, and the hand-written `Ord` must agree with
    /// the `PartialOrd` that delegates to it.
    #[test]
    fn none_sorts_before_shape_and_ord_agrees_with_partial_ord() {
        let shape_clip = parse_clip_path("circle(1px)").expect("circle");
        let shape_out = parse_shape_outside("circle(1px)").expect("circle");
        let shape_in = parse_shape_inside("circle(1px)").expect("circle");

        assert_eq!(ClipPath::None.cmp(&shape_clip), Ordering::Less);
        assert_eq!(shape_clip.cmp(&ClipPath::None), Ordering::Greater);
        assert_eq!(ClipPath::None.cmp(&ClipPath::None), Ordering::Equal);
        assert_eq!(
            ClipPath::None.partial_cmp(&shape_clip),
            Some(ClipPath::None.cmp(&shape_clip))
        );

        assert_eq!(ShapeOutside::None.cmp(&shape_out), Ordering::Less);
        assert_eq!(
            ShapeOutside::None.partial_cmp(&shape_out),
            Some(Ordering::Less)
        );

        assert_eq!(ShapeInside::None.cmp(&shape_in), Ordering::Less);
        assert_eq!(
            ShapeInside::None.partial_cmp(&shape_in),
            Some(Ordering::Less)
        );
    }

    /// `CssShape`'s `Ord` is hand-written with explicit cross-variant arms whose
    /// ORDER encodes the variant ranking. Check it is a strict, antisymmetric
    /// total order across all five variants — a merged/reordered arm would show
    /// up here as two variants comparing Less in both directions.
    #[test]
    fn css_shape_variant_ordering_is_antisymmetric() {
        let shapes = [
            parse_clip_path("circle(1px)").expect("circle"),
            parse_clip_path("ellipse(1px 2px)").expect("ellipse"),
            parse_clip_path("polygon(0 0, 1 1, 2 2)").expect("polygon"),
            parse_clip_path("inset(1px)").expect("inset"),
            parse_clip_path("path(\"Z\")").expect("path"),
        ];

        for (i, a) in shapes.iter().enumerate() {
            assert_eq!(a.cmp(a), Ordering::Equal, "variant {i} is not self-equal");

            for (j, b) in shapes.iter().enumerate() {
                let forward = a.cmp(b);
                let backward = b.cmp(a);
                assert_eq!(
                    forward,
                    backward.reverse(),
                    "cmp is not antisymmetric for variants {i} and {j}"
                );
                if i < j {
                    assert_eq!(
                        forward,
                        Ordering::Less,
                        "variant {i} should sort before variant {j}"
                    );
                }
            }
        }
    }

    /// Hash must agree with equality for the values that *are* self-equal (i.e.
    /// everything except the NaN shapes covered by the known-bug test), and the
    /// discriminant must take part so `None` and a shape don't collide.
    #[test]
    fn hash_agrees_with_equality() {
        let a = parse_clip_path("circle(50px at 1px 2px)").expect("circle");
        let b = parse_clip_path("circle(50px at 1px 2px)").expect("circle");
        assert_eq!(a, b);
        assert_eq!(hash_of(&a), hash_of(&b));

        let different = parse_clip_path("circle(51px at 1px 2px)").expect("circle");
        assert_ne!(a, different);
        assert_ne!(hash_of(&a), hash_of(&different));

        // The discriminant takes part in the hash, so None and a shape do not
        // collide (the manual Hash impls would be easy to write without it).
        assert_ne!(hash_of(&ClipPath::None), hash_of(&a));
        assert_eq!(hash_of(&ClipPath::None), hash_of(&ClipPath::None));

        let outside = parse_shape_outside("circle(50px at 1px 2px)").expect("circle");
        assert_ne!(hash_of(&ShapeOutside::None), hash_of(&outside));

        let margin = parse_shape_margin("10px").expect("10px");
        assert_eq!(
            hash_of(&margin),
            hash_of(&ShapeMargin {
                inner: PixelValue::px(10.0)
            })
        );
        assert_ne!(
            hash_of(&margin),
            hash_of(&parse_shape_margin("10em").expect("10em"))
        );
    }
}

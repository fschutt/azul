//! CSS `calc()` expression evaluator.
//!
//! This module implements a two-pass stack-machine evaluator for `calc()` expressions.
//! It resolves `CalcAstItem` slices (flat, parenthesised AST) into a single `f32` pixel value.
//!
//! **Resolution context**: Em/rem units are resolved using per-node font sizes that are
//! captured lazily during style translation and stored alongside the AST pointer passed
//! to taffy. Percentages use the `basis` value provided by taffy (container width/height).

use azul_css::props::{
    basic::{
        pixel::PT_TO_PX,
        PixelValue, SizeMetric,
    },
    layout::dimensions::{CalcAstItem, CalcAstItemVec},
};

/// CSS reference pixels per inch (96 px/in per CSS spec).
pub(super) const PX_PER_INCH: f32 = 96.0;
/// Centimetres per inch.
pub(super) const CM_PER_INCH: f32 = 2.54;
/// Millimetres per inch.
pub(super) const MM_PER_INCH: f32 = 25.4;

/// Font-size context captured at style-translation time and stored alongside the calc AST.
///
/// Taffy's `resolve_calc_value` callback only receives `(*const (), f32)` — no node id.
/// We therefore bundle the per-node font sizes into the heap-pinned data that the opaque
/// pointer references, so the evaluator can resolve `em` / `rem` correctly.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct CalcResolveContext {
    /// The calc AST items (flat stack-machine representation).
    pub items: CalcAstItemVec,
    /// Element's computed `font-size` in px — used for `em` resolution.
    pub em_size: f32,
    /// Root element's computed `font-size` in px — used for `rem` resolution.
    pub rem_size: f32,
}
// Tiny enum (Num(f32) = 4B vs Op(CalcOp) = 1B); the "large" variant is a bare
// f32, so boxing it would add a pointer + heap allocation — strictly worse than
// the few bytes of size disparity. Accepted.
#[allow(variant_size_differences)]
/// Internal intermediate representation: a number or an operator (after value resolution).
#[derive(Clone, Debug)]
enum CalcFlatItem {
    Num(f32),
    Op(CalcOp),
}

/// Arithmetic operators.
#[derive(Clone, Copy, Debug, PartialEq)]
enum CalcOp {
    Add,
    Sub,
    Mul,
    Div,
}

/// Evaluate a `CalcResolveContext` using the given `basis` (the "100 %" reference value,
/// e.g. containing-block width for `width: calc(…)`).
#[inline(never)] // M12.7: keep out of calc_used_size — its loop/jump-table inlined into the huge fn forces a remill PC-dispatch loop that mis-delivers auto_w
#[must_use] pub fn evaluate_calc(ctx: &CalcResolveContext, basis: f32) -> f32 {
    evaluate_calc_ast(ctx.items.as_slice(), basis, ctx.em_size, ctx.rem_size)
}

/// Stack-machine evaluator for a flat `CalcAstItem` slice.
///
/// `basis`    — the "100 %" reference value (e.g. containing-block width).
/// `em_size`  — element's computed font-size (for `em`).
/// `rem_size` — root element's computed font-size (for `rem`).
///
/// Two-pass approach with correct operator precedence:
///   Pass 1: evaluate `*` and `/`
///   Pass 2: evaluate `+` and `-`
/// Parenthesised sub-expressions are resolved recursively.
#[inline(never)] // M12.7: keep out of calc_used_size — its loop/jump-table inlined into the huge fn forces a remill PC-dispatch loop that mis-delivers auto_w
fn evaluate_calc_ast(
    items: &[CalcAstItem],
    basis: f32,
    em_size: f32,
    rem_size: f32,
) -> f32 {
    // Convert into a working vec of resolved numbers and operators.
    let mut flat: Vec<CalcFlatItem> = Vec::with_capacity(items.len());
    let mut i = 0;
    while i < items.len() {
        match &items[i] {
            CalcAstItem::Value(pv) => {
                flat.push(CalcFlatItem::Num(resolve_pixel_value(
                    pv, basis, em_size, rem_size,
                )));
            }
            CalcAstItem::Add => flat.push(CalcFlatItem::Op(CalcOp::Add)),
            CalcAstItem::Sub => flat.push(CalcFlatItem::Op(CalcOp::Sub)),
            CalcAstItem::Mul => flat.push(CalcFlatItem::Op(CalcOp::Mul)),
            CalcAstItem::Div => flat.push(CalcFlatItem::Op(CalcOp::Div)),
            CalcAstItem::BraceOpen => {
                // Find matching BraceClose and recurse
                let start = i + 1;
                let mut depth = 1u32;
                let mut j = start;
                while j < items.len() && depth > 0 {
                    match &items[j] {
                        CalcAstItem::BraceOpen => depth += 1,
                        CalcAstItem::BraceClose => depth -= 1,
                        _ => {}
                    }
                    if depth > 0 {
                        j += 1;
                    }
                }
                // items[start..j] is the inner sub-expression (excl. braces)
                let sub_val = evaluate_calc_ast(&items[start..j], basis, em_size, rem_size);
                flat.push(CalcFlatItem::Num(sub_val));
                i = j; // skip past the closing brace
            }
            CalcAstItem::BraceClose => { /* shouldn't happen at top level */ }
        }
        i += 1;
    }

    // Pass 1: resolve * and /
    let mut pass2: Vec<CalcFlatItem> = Vec::with_capacity(flat.len());
    let mut k = 0;
    while k < flat.len() {
        if let CalcFlatItem::Op(op @ (CalcOp::Mul | CalcOp::Div)) = &flat[k] {
            // Apply to previous Num in pass2 and next Num in flat
            if let (Some(CalcFlatItem::Num(lhs)), Some(CalcFlatItem::Num(rhs))) =
                (pass2.last(), flat.get(k + 1))
            {
                let result = match op {
                    CalcOp::Mul => lhs * rhs,
                    CalcOp::Div => {
                        if *rhs == 0.0 {
                            0.0
                        } else {
                            lhs / rhs
                        }
                    }
                    _ => unreachable!(),
                };
                *pass2.last_mut().unwrap() = CalcFlatItem::Num(result);
                k += 2; // skip operator + rhs
                continue;
            }
        }
        pass2.push(flat[k].clone());
        k += 1;
    }

    // Pass 2: resolve + and -
    let mut result = match pass2.first() {
        Some(CalcFlatItem::Num(v)) => *v,
        _ => return 0.0,
    };
    let mut m = 1;
    while m < pass2.len() {
        if let (CalcFlatItem::Op(op), Some(CalcFlatItem::Num(rhs))) =
            (&pass2[m], pass2.get(m + 1))
        {
            match op {
                CalcOp::Add => result += rhs,
                CalcOp::Sub => result -= rhs,
                _ => {} // already handled in pass 1
            }
            m += 2;
        } else {
            m += 1;
        }
    }

    result
}

/// Resolve a single `PixelValue` to `f32` pixels inside a `calc()` expression.
///
/// - `basis`    — the "100 %" reference (containing-block width or height)
/// - `em_size`  — element's computed font-size (for `em` units)
/// - `rem_size` — root element's computed font-size (for `rem` units)
///
/// NOTE: this variant has no viewport context, so viewport units (`vw`/`vh`/
/// `vmin`/`vmax`) fall back to their raw number (i.e. `50vw` → `50px`). Callers
/// that may see viewport units must use [`resolve_pixel_value_with_viewport`]
/// (or [`resolve_pixel_value_no_percent_with_viewport`]) instead.
#[must_use] pub fn resolve_pixel_value(
    pv: &PixelValue,
    basis: f32,
    em_size: f32,
    rem_size: f32,
) -> f32 {
    match pv.metric {
        SizeMetric::Px => pv.number.get(),
        SizeMetric::Pt => pv.number.get() * PT_TO_PX,
        SizeMetric::In => pv.number.get() * PX_PER_INCH,
        SizeMetric::Cm => pv.number.get() * PX_PER_INCH / CM_PER_INCH,
        SizeMetric::Mm => pv.number.get() * PX_PER_INCH / MM_PER_INCH,
        SizeMetric::Em => pv.number.get() * em_size,
        SizeMetric::Rem => pv.number.get() * rem_size,
        SizeMetric::Percent => basis * (pv.number.get() / 100.0),
        SizeMetric::Vw | SizeMetric::Vh | SizeMetric::Vmin | SizeMetric::Vmax => {
            // Viewport units: fallback — proper resolution requires viewport context
            pv.number.get()
        }
    }
}

/// Like `resolve_pixel_value`, but with proper viewport unit resolution.
#[inline(never)] // M12.7: keep out of calc_used_size — its loop/jump-table inlined into the huge fn forces a remill PC-dispatch loop that mis-delivers auto_w
#[must_use] pub fn resolve_pixel_value_with_viewport(
    pv: &PixelValue,
    basis: f32,
    em_size: f32,
    rem_size: f32,
    viewport_width: f32,
    viewport_height: f32,
) -> f32 {
    match pv.metric {
        SizeMetric::Vw => pv.number.get() / 100.0 * viewport_width,
        SizeMetric::Vh => pv.number.get() / 100.0 * viewport_height,
        SizeMetric::Vmin => pv.number.get() / 100.0 * viewport_width.min(viewport_height),
        SizeMetric::Vmax => pv.number.get() / 100.0 * viewport_width.max(viewport_height),
        _ => resolve_pixel_value(pv, basis, em_size, rem_size),
    }
}

/// Resolve a `PixelValue` to pixels, returning `None` for percentage and viewport units.
#[must_use] pub fn resolve_pixel_value_no_percent(
    pv: &PixelValue,
    em_size: f32,
    rem_size: f32,
) -> Option<f32> {
    match pv.metric {
        SizeMetric::Px => Some(pv.number.get()),
        SizeMetric::Pt => Some(pv.number.get() * PT_TO_PX),
        SizeMetric::In => Some(pv.number.get() * PX_PER_INCH),
        SizeMetric::Cm => Some(pv.number.get() * PX_PER_INCH / CM_PER_INCH),
        SizeMetric::Mm => Some(pv.number.get() * PX_PER_INCH / MM_PER_INCH),
        SizeMetric::Em => Some(pv.number.get() * em_size),
        SizeMetric::Rem => Some(pv.number.get() * rem_size),
        SizeMetric::Percent
        | SizeMetric::Vw
        | SizeMetric::Vh
        | SizeMetric::Vmin
        | SizeMetric::Vmax => None,
    }
}

/// Like `resolve_pixel_value_no_percent`, but resolves viewport units using
/// the provided viewport dimensions. Returns `None` only for percentages.
#[must_use] pub fn resolve_pixel_value_no_percent_with_viewport(
    pv: &PixelValue,
    em_size: f32,
    rem_size: f32,
    viewport_width: f32,
    viewport_height: f32,
) -> Option<f32> {
    match pv.metric {
        SizeMetric::Vw => Some(pv.number.get() / 100.0 * viewport_width),
        SizeMetric::Vh => Some(pv.number.get() / 100.0 * viewport_height),
        SizeMetric::Vmin => Some(pv.number.get() / 100.0 * viewport_width.min(viewport_height)),
        SizeMetric::Vmax => Some(pv.number.get() / 100.0 * viewport_width.max(viewport_height)),
        _ => resolve_pixel_value_no_percent(pv, em_size, rem_size),
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::cast_precision_loss, clippy::unreadable_literal)]
mod autotest_generated {
    use azul_css::props::basic::FP_PRECISION_MULTIPLIER;

    use super::*;

    // ------------------------------------------------------------------
    // Fixtures
    // ------------------------------------------------------------------

    /// Every `SizeMetric` variant, so "for all metrics" invariants stay honest
    /// if a new unit is added to the enum (the array length breaks the build).
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

    /// Metrics that resolve to an absolute length without any layout context.
    const ABSOLUTE_METRICS: [SizeMetric; 7] = [
        SizeMetric::Px,
        SizeMetric::Pt,
        SizeMetric::Em,
        SizeMetric::Rem,
        SizeMetric::In,
        SizeMetric::Cm,
        SizeMetric::Mm,
    ];

    const VIEWPORT_METRICS: [SizeMetric; 4] = [
        SizeMetric::Vw,
        SizeMetric::Vh,
        SizeMetric::Vmin,
        SizeMetric::Vmax,
    ];

    /// `FloatValue` is fixed-point (`isize` scaled by `FP_PRECISION_MULTIPLIER`),
    /// so any `f32` too large to fit saturates to this magnitude on `get()`.
    fn saturated_magnitude() -> f32 {
        (isize::MAX as f32) / FP_PRECISION_MULTIPLIER
    }

    fn assert_close(actual: f32, expected: f32) {
        let tolerance = 1e-3 * expected.abs().max(1.0);
        assert!(
            (actual - expected).abs() <= tolerance,
            "expected ~{expected}, got {actual}",
        );
    }

    fn val(metric: SizeMetric, n: f32) -> CalcAstItem {
        CalcAstItem::Value(PixelValue::from_metric(metric, n))
    }

    fn px(n: f32) -> CalcAstItem {
        val(SizeMetric::Px, n)
    }

    const EM: f32 = 16.0;
    const REM: f32 = 10.0;

    fn eval(items: &[CalcAstItem], basis: f32) -> f32 {
        evaluate_calc_ast(items, basis, EM, REM)
    }

    fn ctx(items: Vec<CalcAstItem>, em_size: f32, rem_size: f32) -> CalcResolveContext {
        CalcResolveContext {
            items: CalcAstItemVec::from(items),
            em_size,
            rem_size,
        }
    }

    // ==================================================================
    // PixelValue encoding invariants
    //
    // These underpin every resolver below: because `FloatValue` stores an
    // `isize` and `f32 as isize` saturates (NaN -> 0, +/-inf -> MIN/MAX),
    // a `PixelValue` can never *carry* a NaN or an infinity. Any non-finite
    // result therefore has to come from `basis` / `em_size` / `viewport_*`.
    // ==================================================================

    #[test]
    fn pixel_value_number_is_always_finite_even_for_nan_and_inf() {
        for metric in ALL_METRICS {
            for hostile in [
                f32::NAN,
                f32::INFINITY,
                f32::NEG_INFINITY,
                f32::MAX,
                f32::MIN,
                f32::MIN_POSITIVE,
                -0.0,
            ] {
                let n = PixelValue::from_metric(metric, hostile).number.get();
                assert!(
                    n.is_finite(),
                    "PixelValue::from_metric({metric:?}, {hostile}) produced non-finite {n}",
                );
            }
        }
    }

    #[test]
    fn pixel_value_nan_encodes_as_zero() {
        assert_eq!(PixelValue::px(f32::NAN).number.get(), 0.0);
    }

    #[test]
    fn pixel_value_infinities_saturate_to_fixed_point_bounds() {
        let sat = saturated_magnitude();
        assert_close(PixelValue::px(f32::INFINITY).number.get(), sat);
        assert_close(PixelValue::px(f32::NEG_INFINITY).number.get(), -sat);
        // f32::MAX * 1000.0 already overflows to inf before the cast, so it
        // lands on the same saturation point rather than on f32::MAX.
        assert_close(PixelValue::px(f32::MAX).number.get(), sat);
        assert_close(PixelValue::px(f32::MIN).number.get(), -sat);
    }

    // ==================================================================
    // resolve_pixel_value
    // ==================================================================

    #[test]
    fn resolve_pixel_value_zero_is_zero_for_every_metric() {
        for metric in ALL_METRICS {
            let pv = PixelValue::from_metric(metric, 0.0);
            let got = resolve_pixel_value(&pv, 500.0, EM, REM);
            assert_eq!(got, 0.0, "0 {metric:?} should resolve to 0px, got {got}");
        }
    }

    #[test]
    fn resolve_pixel_value_absolute_unit_conversions() {
        assert_eq!(resolve_pixel_value(&PixelValue::px(10.0), 0.0, EM, REM), 10.0);
        assert_close(
            resolve_pixel_value(&PixelValue::pt(12.0), 0.0, EM, REM),
            12.0 * PT_TO_PX,
        );
        // 1in == 96px, 2.54cm == 1in, 25.4mm == 1in.
        assert_close(resolve_pixel_value(&PixelValue::inch(1.0), 0.0, EM, REM), 96.0);
        assert_close(resolve_pixel_value(&PixelValue::cm(2.54), 0.0, EM, REM), 96.0);
        assert_close(resolve_pixel_value(&PixelValue::mm(25.4), 0.0, EM, REM), 96.0);
    }

    #[test]
    fn resolve_pixel_value_em_and_rem_use_their_own_font_size() {
        assert_eq!(resolve_pixel_value(&PixelValue::em(2.0), 0.0, 16.0, 10.0), 32.0);
        assert_eq!(resolve_pixel_value(&PixelValue::rem(2.0), 0.0, 16.0, 10.0), 20.0);
    }

    #[test]
    fn resolve_pixel_value_percent_uses_basis() {
        assert_eq!(resolve_pixel_value(&PixelValue::percent(50.0), 200.0, EM, REM), 100.0);
        assert_eq!(resolve_pixel_value(&PixelValue::percent(0.0), 200.0, EM, REM), 0.0);
        assert_eq!(resolve_pixel_value(&PixelValue::percent(100.0), 0.0, EM, REM), 0.0);
    }

    #[test]
    fn resolve_pixel_value_handles_negative_inputs_deterministically() {
        assert_eq!(resolve_pixel_value(&PixelValue::px(-10.0), 0.0, EM, REM), -10.0);
        assert_eq!(resolve_pixel_value(&PixelValue::em(-2.0), 0.0, 16.0, REM), -32.0);
        // Negative percentage of a positive basis, and positive percentage of a
        // negative basis, both yield a negative length (no clamping here).
        assert_eq!(resolve_pixel_value(&PixelValue::percent(-50.0), 200.0, EM, REM), -100.0);
        assert_eq!(resolve_pixel_value(&PixelValue::percent(50.0), -200.0, EM, REM), -100.0);
    }

    #[test]
    fn resolve_pixel_value_viewport_units_fall_back_to_raw_number() {
        // Documented fallback: with no viewport context, `50vw` degrades to `50px`.
        for metric in VIEWPORT_METRICS {
            let pv = PixelValue::from_metric(metric, 50.0);
            assert_eq!(
                resolve_pixel_value(&pv, 1000.0, EM, REM),
                50.0,
                "{metric:?} should fall back to its raw number",
            );
        }
    }

    #[test]
    fn resolve_pixel_value_is_finite_for_any_pixel_value_in_a_sane_context() {
        // The core safety property: because a PixelValue can never *carry* a NaN
        // or an infinity, no value — not even one built from f32::MAX — can turn
        // a realistic basis/em/rem into a non-finite length. (An absurd basis
        // *can* still overflow; that is pinned separately below.)
        for metric in ALL_METRICS {
            for n in [0.0, -0.0, 1.0, -1.0, f32::MAX, f32::MIN, f32::NAN, f32::INFINITY] {
                let pv = PixelValue::from_metric(metric, n);
                for basis in [0.0_f32, -1000.0, 1920.0, 1_000_000.0] {
                    let got = resolve_pixel_value(&pv, basis, EM, REM);
                    assert!(
                        got.is_finite(),
                        "{metric:?} {n} @ basis {basis} produced non-finite {got}",
                    );
                }
            }
        }
    }

    #[test]
    fn resolve_pixel_value_saturated_percent_of_huge_basis_overflows_to_inf_not_panic() {
        // 100% of f32::MAX stays finite; 200% of it overflows.
        let hundred = resolve_pixel_value(&PixelValue::percent(100.0), f32::MAX, EM, REM);
        assert_eq!(hundred, f32::MAX);

        let two_hundred = resolve_pixel_value(&PixelValue::percent(200.0), f32::MAX, EM, REM);
        assert!(
            two_hundred.is_infinite() && two_hundred.is_sign_positive(),
            "expected +inf on overflow, got {two_hundred}",
        );
    }

    #[test]
    fn resolve_pixel_value_nan_basis_only_pollutes_percentages() {
        // A NaN basis must not leak into units that never read it.
        assert_eq!(resolve_pixel_value(&PixelValue::px(10.0), f32::NAN, EM, REM), 10.0);
        assert_eq!(resolve_pixel_value(&PixelValue::em(1.0), f32::NAN, 16.0, REM), 16.0);
        assert!(resolve_pixel_value(&PixelValue::percent(50.0), f32::NAN, EM, REM).is_nan());
    }

    #[test]
    fn resolve_pixel_value_zero_times_infinite_context_is_nan_not_zero() {
        // IEEE-754 quirk worth pinning: `0em` against an infinite font-size (and
        // `0%` against an infinite basis) is 0 * inf == NaN, NOT 0. Callers that
        // treat "zero length" as always-safe do not get a zero here.
        assert!(resolve_pixel_value(&PixelValue::em(0.0), 0.0, f32::INFINITY, REM).is_nan());
        assert!(resolve_pixel_value(&PixelValue::percent(0.0), f32::INFINITY, EM, REM).is_nan());
        // ...whereas a non-zero multiplier gives a plain infinity.
        let inf = resolve_pixel_value(&PixelValue::em(1.0), 0.0, f32::INFINITY, REM);
        assert!(inf.is_infinite() && inf.is_sign_positive());
    }

    // ==================================================================
    // resolve_pixel_value_with_viewport
    // ==================================================================

    #[test]
    fn resolve_with_viewport_resolves_viewport_units() {
        let (vw, vh) = (1000.0_f32, 800.0_f32);
        let r = |m: SizeMetric, n: f32| {
            resolve_pixel_value_with_viewport(&PixelValue::from_metric(m, n), 0.0, EM, REM, vw, vh)
        };
        assert_eq!(r(SizeMetric::Vw, 50.0), 500.0);
        assert_eq!(r(SizeMetric::Vh, 50.0), 400.0);
        assert_eq!(r(SizeMetric::Vmin, 100.0), 800.0);
        assert_eq!(r(SizeMetric::Vmax, 100.0), 1000.0);
    }

    #[test]
    fn resolve_with_viewport_delegates_non_viewport_metrics_verbatim() {
        for metric in ALL_METRICS {
            if VIEWPORT_METRICS.contains(&metric) {
                continue;
            }
            let pv = PixelValue::from_metric(metric, 33.75);
            assert_eq!(
                resolve_pixel_value_with_viewport(&pv, 500.0, EM, REM, 1920.0, 1080.0),
                resolve_pixel_value(&pv, 500.0, EM, REM),
                "{metric:?} must not be affected by viewport context",
            );
        }
    }

    #[test]
    fn resolve_with_viewport_zero_and_negative_viewport() {
        for metric in VIEWPORT_METRICS {
            let pv = PixelValue::from_metric(metric, 50.0);
            assert_eq!(
                resolve_pixel_value_with_viewport(&pv, 0.0, EM, REM, 0.0, 0.0),
                0.0,
                "{metric:?} against a 0x0 viewport should be 0px",
            );
        }
        // A negative viewport is nonsense but must stay deterministic, not panic.
        let vw50 = PixelValue::from_metric(SizeMetric::Vw, 50.0);
        assert_eq!(
            resolve_pixel_value_with_viewport(&vw50, 0.0, EM, REM, -100.0, -100.0),
            -50.0,
        );
        // min/max still order correctly with negatives: min(-100, -50) == -100.
        let vmin100 = PixelValue::from_metric(SizeMetric::Vmin, 100.0);
        let vmax100 = PixelValue::from_metric(SizeMetric::Vmax, 100.0);
        assert_eq!(
            resolve_pixel_value_with_viewport(&vmin100, 0.0, EM, REM, -100.0, -50.0),
            -100.0,
        );
        assert_eq!(
            resolve_pixel_value_with_viewport(&vmax100, 0.0, EM, REM, -100.0, -50.0),
            -50.0,
        );
    }

    #[test]
    fn resolve_with_viewport_nan_viewport_is_swallowed_by_vmin_vmax_but_not_vw_vh() {
        // f32::min / f32::max return the *other* operand when one side is NaN,
        // so a half-NaN viewport silently produces a finite vmin/vmax...
        let vmin = PixelValue::from_metric(SizeMetric::Vmin, 100.0);
        let vmax = PixelValue::from_metric(SizeMetric::Vmax, 100.0);
        assert_eq!(
            resolve_pixel_value_with_viewport(&vmin, 0.0, EM, REM, f32::NAN, 800.0),
            800.0,
        );
        assert_eq!(
            resolve_pixel_value_with_viewport(&vmax, 0.0, EM, REM, f32::NAN, 800.0),
            800.0,
        );
        // ...while vw/vh propagate the NaN. Both are defined; neither panics.
        let vw = PixelValue::from_metric(SizeMetric::Vw, 50.0);
        assert!(resolve_pixel_value_with_viewport(&vw, 0.0, EM, REM, f32::NAN, 800.0).is_nan());

        // Fully-NaN viewport: vmin/vmax have no finite operand to fall back on.
        assert!(
            resolve_pixel_value_with_viewport(&vmin, 0.0, EM, REM, f32::NAN, f32::NAN).is_nan()
        );
    }

    #[test]
    fn resolve_with_viewport_infinite_viewport_does_not_panic() {
        let vw = PixelValue::from_metric(SizeMetric::Vw, 50.0);
        let got = resolve_pixel_value_with_viewport(&vw, 0.0, EM, REM, f32::INFINITY, 0.0);
        assert!(got.is_infinite() && got.is_sign_positive());

        // 0vw of an infinite viewport is 0 * inf == NaN, same quirk as `0em`.
        let vw0 = PixelValue::from_metric(SizeMetric::Vw, 0.0);
        assert!(
            resolve_pixel_value_with_viewport(&vw0, 0.0, EM, REM, f32::INFINITY, 0.0).is_nan()
        );
    }

    // ==================================================================
    // resolve_pixel_value_no_percent (+ viewport variant)
    // ==================================================================

    #[test]
    fn no_percent_returns_none_for_percent_and_viewport_units() {
        for metric in [
            SizeMetric::Percent,
            SizeMetric::Vw,
            SizeMetric::Vh,
            SizeMetric::Vmin,
            SizeMetric::Vmax,
        ] {
            let pv = PixelValue::from_metric(metric, 50.0);
            assert_eq!(
                resolve_pixel_value_no_percent(&pv, EM, REM),
                None,
                "{metric:?} needs layout context, so it must be None",
            );
        }
    }

    #[test]
    fn no_percent_agrees_with_resolve_pixel_value_on_absolute_metrics() {
        // Invariant: where `no_percent` returns Some, the value must be exactly
        // what `resolve_pixel_value` computes — for *any* basis, since none of
        // these units read it.
        for metric in ABSOLUTE_METRICS {
            for n in [0.0, 1.0, -7.5, 1234.5, f32::MAX, f32::MIN, f32::NAN] {
                let pv = PixelValue::from_metric(metric, n);
                let some = resolve_pixel_value_no_percent(&pv, EM, REM);
                assert_eq!(
                    some,
                    Some(resolve_pixel_value(&pv, 999.0, EM, REM)),
                    "{metric:?} {n} disagrees between the two resolvers",
                );
                assert!(some.unwrap().is_finite());
            }
        }
    }

    #[test]
    fn no_percent_with_viewport_returns_none_only_for_percent() {
        for metric in ALL_METRICS {
            let pv = PixelValue::from_metric(metric, 50.0);
            let got = resolve_pixel_value_no_percent_with_viewport(&pv, EM, REM, 1000.0, 800.0);
            if metric == SizeMetric::Percent {
                assert_eq!(got, None, "% must stay None without a basis");
            } else {
                assert!(got.is_some(), "{metric:?} should resolve with a viewport");
                assert!(got.unwrap().is_finite());
            }
        }
    }

    #[test]
    fn no_percent_with_viewport_agrees_with_resolve_with_viewport() {
        for metric in ALL_METRICS {
            if metric == SizeMetric::Percent {
                continue;
            }
            let pv = PixelValue::from_metric(metric, 25.0);
            assert_eq!(
                resolve_pixel_value_no_percent_with_viewport(&pv, EM, REM, 1920.0, 1080.0),
                Some(resolve_pixel_value_with_viewport(&pv, 0.0, EM, REM, 1920.0, 1080.0)),
                "{metric:?} disagrees between the viewport resolvers",
            );
        }
    }

    #[test]
    fn no_percent_with_viewport_zero_viewport_yields_some_zero_not_none() {
        for metric in VIEWPORT_METRICS {
            let pv = PixelValue::from_metric(metric, 100.0);
            assert_eq!(
                resolve_pixel_value_no_percent_with_viewport(&pv, EM, REM, 0.0, 0.0),
                Some(0.0),
                "{metric:?} against a 0x0 viewport is a defined 0px, not None",
            );
        }
    }

    // ==================================================================
    // evaluate_calc_ast — structure & precedence
    // ==================================================================

    #[test]
    fn eval_empty_ast_is_zero() {
        assert_eq!(eval(&[], 500.0), 0.0);
    }

    #[test]
    fn eval_single_value() {
        assert_eq!(eval(&[px(42.0)], 500.0), 42.0);
        assert_eq!(eval(&[val(SizeMetric::Percent, 10.0)], 500.0), 50.0);
    }

    #[test]
    fn eval_basic_addition_and_subtraction() {
        // calc(100% - 20px) with a 500px containing block
        let items = [val(SizeMetric::Percent, 100.0), CalcAstItem::Sub, px(20.0)];
        assert_eq!(eval(&items, 500.0), 480.0);
    }

    #[test]
    fn eval_add_and_sub_are_left_associative() {
        let items = [px(10.0), CalcAstItem::Sub, px(3.0), CalcAstItem::Sub, px(2.0)];
        assert_eq!(eval(&items, 0.0), 5.0, "must be (10-3)-2, not 10-(3-2)");
    }

    #[test]
    fn eval_div_is_left_associative() {
        let items = [px(10.0), CalcAstItem::Div, px(4.0), CalcAstItem::Div, px(2.0)];
        assert_eq!(eval(&items, 0.0), 1.25, "must be (10/4)/2, not 10/(4/2)");
    }

    #[test]
    fn eval_mul_binds_tighter_than_add() {
        // 1 + 2 * 3 == 7, not 9
        let items = [px(1.0), CalcAstItem::Add, px(2.0), CalcAstItem::Mul, px(3.0)];
        assert_eq!(eval(&items, 0.0), 7.0);

        // 2 * 3 + 4 == 10 (operator on the left of the sum)
        let items = [px(2.0), CalcAstItem::Mul, px(3.0), CalcAstItem::Add, px(4.0)];
        assert_eq!(eval(&items, 0.0), 10.0);
    }

    #[test]
    fn eval_div_binds_tighter_than_sub() {
        // 10 - 6 / 2 == 7, not 2
        let items = [px(10.0), CalcAstItem::Sub, px(6.0), CalcAstItem::Div, px(2.0)];
        assert_eq!(eval(&items, 0.0), 7.0);
    }

    #[test]
    fn eval_braces_override_precedence() {
        // (1 + 2) * 3 == 9
        let items = [
            CalcAstItem::BraceOpen,
            px(1.0),
            CalcAstItem::Add,
            px(2.0),
            CalcAstItem::BraceClose,
            CalcAstItem::Mul,
            px(3.0),
        ];
        assert_eq!(eval(&items, 0.0), 9.0);
    }

    #[test]
    fn eval_nested_braces() {
        // ((2 + 3) * (4 - 2)) == 10
        let items = [
            CalcAstItem::BraceOpen,
            CalcAstItem::BraceOpen,
            px(2.0),
            CalcAstItem::Add,
            px(3.0),
            CalcAstItem::BraceClose,
            CalcAstItem::Mul,
            CalcAstItem::BraceOpen,
            px(4.0),
            CalcAstItem::Sub,
            px(2.0),
            CalcAstItem::BraceClose,
            CalcAstItem::BraceClose,
        ];
        assert_eq!(eval(&items, 0.0), 10.0);
    }

    #[test]
    fn eval_empty_braces_are_zero() {
        let items = [CalcAstItem::BraceOpen, CalcAstItem::BraceClose];
        assert_eq!(eval(&items, 500.0), 0.0);

        // ...and an empty brace still participates as a term: 10 + () == 10
        let items = [
            px(10.0),
            CalcAstItem::Add,
            CalcAstItem::BraceOpen,
            CalcAstItem::BraceClose,
        ];
        assert_eq!(eval(&items, 500.0), 10.0);
    }

    #[test]
    fn eval_percent_inside_braces_still_sees_the_basis() {
        // (100% - 20px) / 4  with basis 500 -> 120
        let items = [
            CalcAstItem::BraceOpen,
            val(SizeMetric::Percent, 100.0),
            CalcAstItem::Sub,
            px(20.0),
            CalcAstItem::BraceClose,
            CalcAstItem::Div,
            px(4.0),
        ];
        assert_eq!(eval(&items, 500.0), 120.0);
    }

    #[test]
    fn eval_em_and_rem_come_from_the_evaluator_args_not_the_basis() {
        // 2em + 1rem with em=10, rem=20 -> 40
        let items = [val(SizeMetric::Em, 2.0), CalcAstItem::Add, val(SizeMetric::Rem, 1.0)];
        assert_eq!(evaluate_calc_ast(&items, 0.0, 10.0, 20.0), 40.0);
    }

    // ==================================================================
    // evaluate_calc_ast — malformed / adversarial ASTs
    //
    // The AST is only ever produced by the parser, but it also crosses an FFI
    // boundary (`CalcAstItemVec` is `repr(C)`), so a hand-built or corrupted
    // item list must degrade, never panic or hang.
    // ==================================================================

    #[test]
    fn eval_unmatched_brace_open_does_not_panic_or_hang() {
        // A lone `(` — the scan for the matching `)` runs off the end.
        assert_eq!(eval(&[CalcAstItem::BraceOpen], 500.0), 0.0);
        // `(5px` — the unterminated body is still evaluated.
        assert_eq!(eval(&[CalcAstItem::BraceOpen, px(5.0)], 500.0), 5.0);
        // `1px + (2px` -> 3
        let items = [
            px(1.0),
            CalcAstItem::Add,
            CalcAstItem::BraceOpen,
            px(2.0),
        ];
        assert_eq!(eval(&items, 500.0), 3.0);
    }

    #[test]
    fn eval_stray_brace_close_is_ignored() {
        // `5px ) + 3px` -> the stray `)` is skipped at top level.
        let items = [
            px(5.0),
            CalcAstItem::BraceClose,
            CalcAstItem::Add,
            px(3.0),
        ];
        assert_eq!(eval(&items, 0.0), 8.0);

        // A leading `)` is likewise ignored.
        assert_eq!(eval(&[CalcAstItem::BraceClose, px(7.0)], 0.0), 7.0);
    }

    #[test]
    fn eval_leading_operator_collapses_to_zero() {
        // Nothing to apply the operator to -> the whole expression is 0.
        assert_eq!(eval(&[CalcAstItem::Mul, px(5.0)], 0.0), 0.0);
        assert_eq!(eval(&[CalcAstItem::Add, px(5.0)], 0.0), 0.0);
        assert_eq!(eval(&[CalcAstItem::Div, px(5.0)], 0.0), 0.0);
    }

    #[test]
    fn eval_trailing_operator_keeps_the_left_hand_side() {
        for op in [
            CalcAstItem::Add,
            CalcAstItem::Sub,
            CalcAstItem::Mul,
            CalcAstItem::Div,
        ] {
            assert_eq!(eval(&[px(10.0), op], 0.0), 10.0, "trailing {op:?} dropped the lhs");
        }
    }

    #[test]
    fn eval_operator_only_ast_is_zero() {
        let items = [
            CalcAstItem::Add,
            CalcAstItem::Sub,
            CalcAstItem::Mul,
            CalcAstItem::Div,
        ];
        assert_eq!(eval(&items, 500.0), 0.0);
    }

    #[test]
    fn eval_adjacent_values_without_an_operator_keep_only_the_first() {
        // `10px 20px` is not valid calc(); the evaluator silently drops the tail
        // rather than panicking. Pinning the current behaviour.
        assert_eq!(eval(&[px(10.0), px(20.0)], 0.0), 10.0);
    }

    #[test]
    fn eval_adjacent_operators_do_not_panic() {
        // `1px + * 2px` — the orphaned `*` survives pass 1 and is skipped in pass 2.
        let items = [
            px(1.0),
            CalcAstItem::Add,
            CalcAstItem::Mul,
            px(2.0),
        ];
        assert_eq!(eval(&items, 0.0), 1.0);
    }

    #[test]
    fn eval_deeply_nested_braces_do_not_overflow_the_stack() {
        // Brace handling recurses once per nesting level. 256 levels is far past
        // anything a real stylesheet produces and must still return cleanly.
        const DEPTH: usize = 256;
        let mut items: Vec<CalcAstItem> = vec![CalcAstItem::BraceOpen; DEPTH];
        items.push(px(1.0));
        items.resize(DEPTH * 2 + 1, CalcAstItem::BraceClose);
        assert_eq!(eval(&items, 0.0), 1.0);
    }

    #[test]
    fn eval_unbalanced_deep_braces_terminate() {
        // 256 `(` with no `)` at all: every level recurses on a strictly shorter
        // slice, so this must terminate rather than loop forever.
        let items = vec![CalcAstItem::BraceOpen; 256];
        assert_eq!(eval(&items, 0.0), 0.0);
    }

    #[test]
    fn eval_long_flat_ast_terminates_with_the_right_sum() {
        // 1000 terms of `1px + 1px + ...` — guards against a quadratic blow-up
        // or a non-advancing index in the two passes.
        const TERMS: usize = 1000;
        let mut items = Vec::with_capacity(TERMS * 2 - 1);
        for i in 0..TERMS {
            if i > 0 {
                items.push(CalcAstItem::Add);
            }
            items.push(px(1.0));
        }
        assert_eq!(eval(&items, 0.0), TERMS as f32);
    }

    // ==================================================================
    // evaluate_calc_ast — numeric edge cases
    // ==================================================================

    #[test]
    fn eval_division_by_zero_is_guarded_to_zero_not_infinity() {
        let items = [px(10.0), CalcAstItem::Div, px(0.0)];
        let got = eval(&items, 0.0);
        assert_eq!(got, 0.0);
        assert!(got.is_finite(), "10px / 0px must not be an infinity");
    }

    #[test]
    fn eval_division_by_a_zero_valued_percent_is_also_guarded() {
        // The divisor is only known after the basis is applied: 100% of a 0 basis.
        let items = [px(10.0), CalcAstItem::Div, val(SizeMetric::Percent, 100.0)];
        assert_eq!(eval(&items, 0.0), 0.0);

        // Negative zero compares equal to 0.0, so it takes the same guard and
        // yields +0.0 rather than -inf.
        let got = eval(&items, -0.0);
        assert_eq!(got, 0.0);
        assert!(got.is_finite(), "10px / (100% of -0) must not be -inf");
    }

    #[test]
    fn eval_division_by_nan_is_not_caught_by_the_zero_guard() {
        // The guard is `rhs == 0.0`, which NaN fails — so the NaN propagates.
        // Defined, not a panic, but worth pinning: NaN divisors are NOT sanitised.
        let items = [px(10.0), CalcAstItem::Div, val(SizeMetric::Percent, 100.0)];
        assert!(eval(&items, f32::NAN).is_nan());
    }

    #[test]
    fn eval_divisor_below_fixed_point_precision_truncates_into_the_zero_guard() {
        // `FloatValue` keeps 3 decimal places, so 0.0001 encodes to *exactly* 0 —
        // a divisor that is non-zero in the stylesheet but zero by the time the
        // evaluator sees it. `calc(10px / 0.0001)` is therefore 0px, not 100000px.
        assert_eq!(PixelValue::px(0.0001).number.get(), 0.0);
        let items = [px(10.0), CalcAstItem::Div, px(0.0001)];
        assert_eq!(eval(&items, 0.0), 0.0);

        // 0.001 is the smallest divisor that survives the encoding.
        assert_close(PixelValue::px(0.001).number.get(), 0.001);
        let items = [px(10.0), CalcAstItem::Div, px(0.001)];
        let got = eval(&items, 0.0);
        assert!(got.is_finite(), "10px / 0.001px should be finite, got {got}");
        assert_close(got, 10_000.0);
    }

    #[test]
    fn eval_multiplication_overflow_saturates_to_infinity_without_panicking() {
        // Chain enough saturated values that the product overflows f32 on any
        // isize width (64-bit *and* 32-bit/wasm).
        let mut items = vec![px(f32::MAX)];
        for _ in 0..7 {
            items.push(CalcAstItem::Mul);
            items.push(px(f32::MAX));
        }
        let got = eval(&items, 0.0);
        assert!(
            got.is_infinite() && got.is_sign_positive(),
            "expected +inf on overflow, got {got}",
        );
    }

    #[test]
    fn eval_inf_minus_inf_is_nan_not_a_panic() {
        // An infinite basis makes both percentages infinite; inf - inf == NaN.
        let items = [
            val(SizeMetric::Percent, 100.0),
            CalcAstItem::Sub,
            val(SizeMetric::Percent, 100.0),
        ];
        assert!(eval(&items, f32::INFINITY).is_nan());
    }

    #[test]
    fn eval_nan_basis_does_not_leak_into_an_absolute_expression() {
        let items = [px(10.0), CalcAstItem::Add, px(5.0)];
        assert_eq!(eval(&items, f32::NAN), 15.0);
        assert_eq!(eval(&items, f32::INFINITY), 15.0);
    }

    #[test]
    fn eval_negative_results_are_not_clamped() {
        // calc(20px - 100%) with a 500px basis is legitimately negative here;
        // clamping is the caller's job, not the evaluator's.
        let items = [px(20.0), CalcAstItem::Sub, val(SizeMetric::Percent, 100.0)];
        assert_eq!(eval(&items, 500.0), -480.0);
    }

    #[test]
    fn eval_viewport_units_inside_calc_use_the_no_viewport_fallback() {
        // evaluate_calc_ast routes through `resolve_pixel_value`, which has no
        // viewport context — so `50vw` inside calc() degrades to `50px`.
        let items = [val(SizeMetric::Vw, 50.0), CalcAstItem::Add, px(0.0)];
        assert_eq!(eval(&items, 1000.0), 50.0);
    }

    // ==================================================================
    // evaluate_calc (public wrapper)
    // ==================================================================

    #[test]
    fn evaluate_calc_empty_context_is_zero() {
        let c = ctx(Vec::new(), 16.0, 10.0);
        assert_eq!(evaluate_calc(&c, 500.0), 0.0);
        assert_eq!(evaluate_calc(&c, f32::NAN), 0.0);
    }

    #[test]
    fn evaluate_calc_matches_the_ast_evaluator_it_wraps() {
        let items = vec![
            val(SizeMetric::Percent, 100.0),
            CalcAstItem::Sub,
            CalcAstItem::BraceOpen,
            val(SizeMetric::Em, 2.0),
            CalcAstItem::Add,
            val(SizeMetric::Rem, 1.0),
            CalcAstItem::BraceClose,
        ];
        let c = ctx(items.clone(), 12.0, 20.0);
        // 100% of 500 - (2*12 + 1*20) == 500 - 44 == 456
        assert_eq!(evaluate_calc(&c, 500.0), 456.0);
        assert_eq!(evaluate_calc(&c, 500.0), evaluate_calc_ast(&items, 500.0, 12.0, 20.0));
    }

    #[test]
    fn evaluate_calc_forwards_its_own_em_and_rem_sizes_without_swapping_them() {
        // `2em + 1rem` is asymmetric on purpose: swapping em/rem must change the
        // answer, which a transposed-argument bug in the wrapper would not.
        let items = vec![val(SizeMetric::Em, 2.0), CalcAstItem::Add, val(SizeMetric::Rem, 1.0)];
        assert_eq!(evaluate_calc(&ctx(items.clone(), 10.0, 20.0), 0.0), 40.0);
        assert_eq!(evaluate_calc(&ctx(items, 20.0, 10.0), 0.0), 50.0);
    }

    #[test]
    fn evaluate_calc_with_hostile_font_sizes_does_not_panic() {
        let items = vec![val(SizeMetric::Em, 2.0), CalcAstItem::Add, px(1.0)];
        assert!(evaluate_calc(&ctx(items.clone(), f32::NAN, 10.0), 0.0).is_nan());

        let inf = evaluate_calc(&ctx(items.clone(), f32::INFINITY, 10.0), 0.0);
        assert!(inf.is_infinite() && inf.is_sign_positive());

        // A zero font-size is legal CSS and must give a plain 1px, not NaN.
        assert_eq!(evaluate_calc(&ctx(items, 0.0, 10.0), 0.0), 1.0);
    }

    #[test]
    fn evaluate_calc_is_pure_and_repeatable() {
        // The context is borrowed, not consumed: evaluating twice with different
        // bases must not mutate or corrupt the AST.
        let c = ctx(
            vec![val(SizeMetric::Percent, 50.0), CalcAstItem::Add, px(10.0)],
            16.0,
            10.0,
        );
        assert_eq!(evaluate_calc(&c, 100.0), 60.0);
        assert_eq!(evaluate_calc(&c, 200.0), 110.0);
        assert_eq!(evaluate_calc(&c, 100.0), 60.0);
    }
}


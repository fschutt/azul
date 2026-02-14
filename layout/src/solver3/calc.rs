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
        pixel::{DEFAULT_FONT_SIZE, PT_TO_PX},
        PixelValue, SizeMetric,
    },
    layout::dimensions::{CalcAstItem, CalcAstItemVec},
};

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
pub fn evaluate_calc(ctx: &CalcResolveContext, basis: f32) -> f32 {
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
pub fn evaluate_calc_ast(
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
                        if *rhs != 0.0 {
                            lhs / rhs
                        } else {
                            0.0
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
pub fn resolve_pixel_value(
    pv: &PixelValue,
    basis: f32,
    em_size: f32,
    rem_size: f32,
) -> f32 {
    match pv.metric {
        SizeMetric::Px => pv.number.get(),
        SizeMetric::Pt => pv.number.get() * PT_TO_PX,
        SizeMetric::In => pv.number.get() * 96.0,
        SizeMetric::Cm => pv.number.get() * 96.0 / 2.54,
        SizeMetric::Mm => pv.number.get() * 96.0 / 25.4,
        SizeMetric::Em => pv.number.get() * em_size,
        SizeMetric::Rem => pv.number.get() * rem_size,
        SizeMetric::Percent => basis * (pv.number.get() / 100.0),
        SizeMetric::Vw | SizeMetric::Vh | SizeMetric::Vmin | SizeMetric::Vmax => {
            // Viewport units: fallback — proper resolution requires viewport context
            pv.number.get()
        }
    }
}

// ============================================================================
// Unit tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use azul_css::props::basic::{FloatValue, PixelValue, SizeMetric};

    /// Helper: create a `PixelValue` from metric + number.
    fn pv(metric: SizeMetric, number: f32) -> PixelValue {
        PixelValue {
            metric,
            number: FloatValue::new(number),
        }
    }

    /// Helper: wrap `CalcAstItem`s into a `CalcAstItemVec`.
    fn items(v: Vec<CalcAstItem>) -> CalcAstItemVec {
        v.into()
    }

    /// Helper: build a `CalcResolveContext` with default font sizes.
    fn ctx(ast: Vec<CalcAstItem>) -> CalcResolveContext {
        CalcResolveContext {
            items: items(ast),
            em_size: DEFAULT_FONT_SIZE,
            rem_size: DEFAULT_FONT_SIZE,
        }
    }

    /// Helper: build a `CalcResolveContext` with custom font sizes.
    fn ctx_with_fonts(ast: Vec<CalcAstItem>, em: f32, rem: f32) -> CalcResolveContext {
        CalcResolveContext {
            items: items(ast),
            em_size: em,
            rem_size: rem,
        }
    }

    // ------------------------------------------------------------------
    // Basic value resolution
    // ------------------------------------------------------------------

    #[test]
    fn single_px_value() {
        // calc(100px)
        let c = ctx(vec![CalcAstItem::Value(pv(SizeMetric::Px, 100.0))]);
        assert_eq!(evaluate_calc(&c, 0.0), 100.0);
    }

    #[test]
    fn single_percent_value() {
        // calc(50%)  with basis=400 → 200
        let c = ctx(vec![CalcAstItem::Value(pv(SizeMetric::Percent, 50.0))]);
        assert_eq!(evaluate_calc(&c, 400.0), 200.0);
    }

    #[test]
    fn single_pt_value() {
        // calc(12pt) → 12 * 1.3333… = 16
        let c = ctx(vec![CalcAstItem::Value(pv(SizeMetric::Pt, 12.0))]);
        let result = evaluate_calc(&c, 0.0);
        assert!((result - 16.0).abs() < 0.01);
    }

    // ------------------------------------------------------------------
    // Addition / subtraction
    // ------------------------------------------------------------------

    #[test]
    fn simple_addition() {
        // calc(100px + 50px) = 150
        let c = ctx(vec![
            CalcAstItem::Value(pv(SizeMetric::Px, 100.0)),
            CalcAstItem::Add,
            CalcAstItem::Value(pv(SizeMetric::Px, 50.0)),
        ]);
        assert_eq!(evaluate_calc(&c, 0.0), 150.0);
    }

    #[test]
    fn simple_subtraction() {
        // calc(200px - 50px) = 150
        let c = ctx(vec![
            CalcAstItem::Value(pv(SizeMetric::Px, 200.0)),
            CalcAstItem::Sub,
            CalcAstItem::Value(pv(SizeMetric::Px, 50.0)),
        ]);
        assert_eq!(evaluate_calc(&c, 0.0), 150.0);
    }

    #[test]
    fn percent_minus_px() {
        // calc(100% - 20px) with basis=300 → 300 - 20 = 280
        let c = ctx(vec![
            CalcAstItem::Value(pv(SizeMetric::Percent, 100.0)),
            CalcAstItem::Sub,
            CalcAstItem::Value(pv(SizeMetric::Px, 20.0)),
        ]);
        assert_eq!(evaluate_calc(&c, 300.0), 280.0);
    }

    #[test]
    fn thirds_calc() {
        // calc(33.333% - 10px) with basis=900 → 300 - 10 = ~290
        let c = ctx(vec![
            CalcAstItem::Value(pv(SizeMetric::Percent, 33.333)),
            CalcAstItem::Sub,
            CalcAstItem::Value(pv(SizeMetric::Px, 10.0)),
        ]);
        let result = evaluate_calc(&c, 900.0);
        assert!((result - 289.997).abs() < 0.01);
    }

    // ------------------------------------------------------------------
    // Multiplication / division
    // ------------------------------------------------------------------

    #[test]
    fn simple_multiplication() {
        // calc(50px * 3) = 150
        let c = ctx(vec![
            CalcAstItem::Value(pv(SizeMetric::Px, 50.0)),
            CalcAstItem::Mul,
            CalcAstItem::Value(pv(SizeMetric::Px, 3.0)),
        ]);
        assert_eq!(evaluate_calc(&c, 0.0), 150.0);
    }

    #[test]
    fn simple_division() {
        // calc(300px / 4) = 75
        let c = ctx(vec![
            CalcAstItem::Value(pv(SizeMetric::Px, 300.0)),
            CalcAstItem::Div,
            CalcAstItem::Value(pv(SizeMetric::Px, 4.0)),
        ]);
        assert_eq!(evaluate_calc(&c, 0.0), 75.0);
    }

    #[test]
    fn division_by_zero() {
        // calc(100px / 0) → 0 (safe fallback)
        let c = ctx(vec![
            CalcAstItem::Value(pv(SizeMetric::Px, 100.0)),
            CalcAstItem::Div,
            CalcAstItem::Value(pv(SizeMetric::Px, 0.0)),
        ]);
        assert_eq!(evaluate_calc(&c, 0.0), 0.0);
    }

    // ------------------------------------------------------------------
    // Operator precedence: * / before + -
    // ------------------------------------------------------------------

    #[test]
    fn precedence_mul_before_add() {
        // calc(10px + 5px * 3) = 10 + 15 = 25
        let c = ctx(vec![
            CalcAstItem::Value(pv(SizeMetric::Px, 10.0)),
            CalcAstItem::Add,
            CalcAstItem::Value(pv(SizeMetric::Px, 5.0)),
            CalcAstItem::Mul,
            CalcAstItem::Value(pv(SizeMetric::Px, 3.0)),
        ]);
        assert_eq!(evaluate_calc(&c, 0.0), 25.0);
    }

    #[test]
    fn precedence_div_before_sub() {
        // calc(100px - 60px / 3) = 100 - 20 = 80
        let c = ctx(vec![
            CalcAstItem::Value(pv(SizeMetric::Px, 100.0)),
            CalcAstItem::Sub,
            CalcAstItem::Value(pv(SizeMetric::Px, 60.0)),
            CalcAstItem::Div,
            CalcAstItem::Value(pv(SizeMetric::Px, 3.0)),
        ]);
        assert_eq!(evaluate_calc(&c, 0.0), 80.0);
    }

    #[test]
    fn precedence_complex() {
        // calc(2px * 3 + 4px * 5) = 6 + 20 = 26
        let c = ctx(vec![
            CalcAstItem::Value(pv(SizeMetric::Px, 2.0)),
            CalcAstItem::Mul,
            CalcAstItem::Value(pv(SizeMetric::Px, 3.0)),
            CalcAstItem::Add,
            CalcAstItem::Value(pv(SizeMetric::Px, 4.0)),
            CalcAstItem::Mul,
            CalcAstItem::Value(pv(SizeMetric::Px, 5.0)),
        ]);
        assert_eq!(evaluate_calc(&c, 0.0), 26.0);
    }

    // ------------------------------------------------------------------
    // Parenthesised sub-expressions
    // ------------------------------------------------------------------

    #[test]
    fn simple_parens() {
        // calc((10px + 20px)) = 30
        let c = ctx(vec![
            CalcAstItem::BraceOpen,
            CalcAstItem::Value(pv(SizeMetric::Px, 10.0)),
            CalcAstItem::Add,
            CalcAstItem::Value(pv(SizeMetric::Px, 20.0)),
            CalcAstItem::BraceClose,
        ]);
        assert_eq!(evaluate_calc(&c, 0.0), 30.0);
    }

    #[test]
    fn parens_override_precedence() {
        // calc((10px + 5px) * 3) = 15 * 3 = 45
        let c = ctx(vec![
            CalcAstItem::BraceOpen,
            CalcAstItem::Value(pv(SizeMetric::Px, 10.0)),
            CalcAstItem::Add,
            CalcAstItem::Value(pv(SizeMetric::Px, 5.0)),
            CalcAstItem::BraceClose,
            CalcAstItem::Mul,
            CalcAstItem::Value(pv(SizeMetric::Px, 3.0)),
        ]);
        assert_eq!(evaluate_calc(&c, 0.0), 45.0);
    }

    #[test]
    fn nested_parens() {
        // calc(100px - (20px + (5px * 2)))
        // inner: 5 * 2 = 10
        // middle: 20 + 10 = 30
        // outer: 100 - 30 = 70
        let c = ctx(vec![
            CalcAstItem::Value(pv(SizeMetric::Px, 100.0)),
            CalcAstItem::Sub,
            CalcAstItem::BraceOpen,
            CalcAstItem::Value(pv(SizeMetric::Px, 20.0)),
            CalcAstItem::Add,
            CalcAstItem::BraceOpen,
            CalcAstItem::Value(pv(SizeMetric::Px, 5.0)),
            CalcAstItem::Mul,
            CalcAstItem::Value(pv(SizeMetric::Px, 2.0)),
            CalcAstItem::BraceClose,
            CalcAstItem::BraceClose,
        ]);
        assert_eq!(evaluate_calc(&c, 0.0), 70.0);
    }

    // ------------------------------------------------------------------
    // Em / rem resolution with node-local font sizes
    // ------------------------------------------------------------------

    #[test]
    fn em_with_default_font_size() {
        // calc(2em) with default em=16 → 32
        let c = ctx(vec![CalcAstItem::Value(pv(SizeMetric::Em, 2.0))]);
        assert_eq!(evaluate_calc(&c, 0.0), DEFAULT_FONT_SIZE * 2.0);
    }

    #[test]
    fn em_with_custom_font_size() {
        // calc(2em) with em=24 → 48
        let c = ctx_with_fonts(
            vec![CalcAstItem::Value(pv(SizeMetric::Em, 2.0))],
            24.0,
            16.0,
        );
        assert_eq!(evaluate_calc(&c, 0.0), 48.0);
    }

    #[test]
    fn rem_with_custom_root_font_size() {
        // calc(1.5rem) with rem=20 → 30
        let c = ctx_with_fonts(
            vec![CalcAstItem::Value(pv(SizeMetric::Rem, 1.5))],
            16.0,
            20.0,
        );
        assert_eq!(evaluate_calc(&c, 0.0), 30.0);
    }

    #[test]
    fn em_and_rem_differ() {
        // calc(1em + 1rem) with em=24, rem=20 → 24 + 20 = 44
        let c = ctx_with_fonts(
            vec![
                CalcAstItem::Value(pv(SizeMetric::Em, 1.0)),
                CalcAstItem::Add,
                CalcAstItem::Value(pv(SizeMetric::Rem, 1.0)),
            ],
            24.0,
            20.0,
        );
        assert_eq!(evaluate_calc(&c, 0.0), 44.0);
    }

    #[test]
    fn em_percent_mixed() {
        // calc(50% + 2em) with basis=200, em=12 → 100 + 24 = 124
        let c = ctx_with_fonts(
            vec![
                CalcAstItem::Value(pv(SizeMetric::Percent, 50.0)),
                CalcAstItem::Add,
                CalcAstItem::Value(pv(SizeMetric::Em, 2.0)),
            ],
            12.0,
            16.0,
        );
        assert_eq!(evaluate_calc(&c, 200.0), 124.0);
    }

    // ------------------------------------------------------------------
    // Real-world calc expressions
    // ------------------------------------------------------------------

    #[test]
    fn flexbox_three_column() {
        // calc(33.333% - 10px) per column, basis=600 → 200 - 10 = 190
        let c = ctx(vec![
            CalcAstItem::Value(pv(SizeMetric::Percent, 33.333)),
            CalcAstItem::Sub,
            CalcAstItem::Value(pv(SizeMetric::Px, 10.0)),
        ]);
        let result = evaluate_calc(&c, 600.0);
        // 33.333% of 600 = 199.998
        assert!((result - 189.998).abs() < 0.01);
    }

    #[test]
    fn sidebar_main_layout() {
        // calc(100% - 250px) for a sidebar offset, basis=1024 → 774
        let c = ctx(vec![
            CalcAstItem::Value(pv(SizeMetric::Percent, 100.0)),
            CalcAstItem::Sub,
            CalcAstItem::Value(pv(SizeMetric::Px, 250.0)),
        ]);
        assert_eq!(evaluate_calc(&c, 1024.0), 774.0);
    }

    #[test]
    fn responsive_padding() {
        // calc(1rem + 2%) with rem=16, basis=800 → 16 + 16 = 32
        let c = ctx_with_fonts(
            vec![
                CalcAstItem::Value(pv(SizeMetric::Rem, 1.0)),
                CalcAstItem::Add,
                CalcAstItem::Value(pv(SizeMetric::Percent, 2.0)),
            ],
            16.0,
            16.0,
        );
        assert_eq!(evaluate_calc(&c, 800.0), 32.0);
    }

    // ------------------------------------------------------------------
    // Edge cases
    // ------------------------------------------------------------------

    #[test]
    fn empty_expression() {
        let c = ctx(vec![]);
        assert_eq!(evaluate_calc(&c, 100.0), 0.0);
    }

    #[test]
    fn only_operator_no_values() {
        let c = ctx(vec![CalcAstItem::Add]);
        assert_eq!(evaluate_calc(&c, 100.0), 0.0);
    }

    #[test]
    fn multiple_additions() {
        // calc(10px + 20px + 30px) = 60
        let c = ctx(vec![
            CalcAstItem::Value(pv(SizeMetric::Px, 10.0)),
            CalcAstItem::Add,
            CalcAstItem::Value(pv(SizeMetric::Px, 20.0)),
            CalcAstItem::Add,
            CalcAstItem::Value(pv(SizeMetric::Px, 30.0)),
        ]);
        assert_eq!(evaluate_calc(&c, 0.0), 60.0);
    }

    #[test]
    fn cm_unit() {
        // calc(2.54cm) = 96px (1in = 2.54cm = 96px)
        let c = ctx(vec![CalcAstItem::Value(pv(SizeMetric::Cm, 2.54))]);
        let result = evaluate_calc(&c, 0.0);
        assert!((result - 96.0).abs() < 0.01);
    }

    #[test]
    fn mm_unit() {
        // calc(25.4mm) = 96px
        let c = ctx(vec![CalcAstItem::Value(pv(SizeMetric::Mm, 25.4))]);
        let result = evaluate_calc(&c, 0.0);
        assert!((result - 96.0).abs() < 0.01);
    }

    #[test]
    fn in_unit() {
        // calc(1in) = 96px
        let c = ctx(vec![CalcAstItem::Value(pv(SizeMetric::In, 1.0))]);
        assert_eq!(evaluate_calc(&c, 0.0), 96.0);
    }

    #[test]
    fn chained_mul_div() {
        // calc(100px * 2 / 4) = 200 / 4 = 50
        let c = ctx(vec![
            CalcAstItem::Value(pv(SizeMetric::Px, 100.0)),
            CalcAstItem::Mul,
            CalcAstItem::Value(pv(SizeMetric::Px, 2.0)),
            CalcAstItem::Div,
            CalcAstItem::Value(pv(SizeMetric::Px, 4.0)),
        ]);
        assert_eq!(evaluate_calc(&c, 0.0), 50.0);
    }

    // ------------------------------------------------------------------
    // resolve_pixel_value directly
    // ------------------------------------------------------------------

    #[test]
    fn resolve_px() {
        assert_eq!(resolve_pixel_value(&pv(SizeMetric::Px, 42.0), 0.0, 16.0, 16.0), 42.0);
    }

    #[test]
    fn resolve_percent() {
        assert_eq!(resolve_pixel_value(&pv(SizeMetric::Percent, 25.0), 400.0, 16.0, 16.0), 100.0);
    }

    #[test]
    fn resolve_em_custom() {
        assert_eq!(resolve_pixel_value(&pv(SizeMetric::Em, 2.0), 0.0, 20.0, 16.0), 40.0);
    }

    #[test]
    fn resolve_rem_custom() {
        assert_eq!(resolve_pixel_value(&pv(SizeMetric::Rem, 2.0), 0.0, 20.0, 18.0), 36.0);
    }
}

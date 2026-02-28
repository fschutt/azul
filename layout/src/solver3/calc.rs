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


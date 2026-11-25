//! Fewer text objects for the same page: a pass over a page's ops, run by the serializer when
//! [`PdfSaveOptions::optimize`](crate::PdfSaveOptions) is set.
//!
//! The HTML renderer gives every glyph run its own text object:
//!
//! ```text
//! /F1 12 Tf BT /Span <</ActualText (Hello)>> BDC 1 0 0 1 72 700 Tm [...] TJ EMC ET
//! /F2 12 Tf BT /Span <</ActualText ( )>> BDC     1 0 0 1 98 700 Tm [...] TJ EMC ET
//! /F1 12 Tf BT /Span <</ActualText (world)>> BDC 1 0 0 1 104 700 Tm [...] TJ EMC ET
//! ```
//!
//! and a run ends at every font change, so a line whose spaces come from a fallback font is
//! one text object per word and one per space. This pass rewrites that into
//!
//! ```text
//! /F1 12 Tf BT /Span <</ActualText (Hello world)>> BDC
//!   1 0 0 1 72 700 Tm [...] TJ /F2 12 Tf 1 0 0 1 98 700 Tm [...] TJ /F1 12 Tf 1 0 0 1 104 700 Tm [...] TJ
//! EMC ET
//! ```
//!
//! 1. A text object ends (`ET`), only text state follows (font, colour, spacing — operators a
//!    text object may contain), and the next one begins (`BT`) and positions its text with a
//!    `Tm` before showing or moving anything: the two become one text object. (`BT` resets the
//!    text matrix, so a text object that relies on that reset is left alone.)
//! 2. A font or fill colour set to the value already in effect is dropped (the state is
//!    forgotten at `Q` and at `gs`, which can restore or set it).
//! 3. `/Span` marked-content sections that only carry an `/ActualText` and follow each other on
//!    the same baseline, with only text state between them, become one section with the joined
//!    text. Copy-paste of the line yields the same string, and spans on different baselines
//!    stay separate so the line break survives.
//!
//! What a viewer draws is unchanged: the same glyphs at the same matrices in the same fonts and
//! colours.

use crate::{DictItem, Op};

/// Ops allowed inside a text object that do not draw or move: text state and colour.
fn is_text_state(op: &Op) -> bool {
    matches!(
        op,
        Op::SetFont { .. }
            | Op::SetFillColor { .. }
            | Op::SetOutlineColor { .. }
            | Op::SetTextRenderingMode { .. }
            | Op::SetCharacterSpacing { .. }
            | Op::SetWordSpacing { .. }
            | Op::SetHorizontalScaling { .. }
            | Op::SetLineOffset { .. }
            | Op::SetLineHeight { .. }
    )
}

/// True if the text object starting at `ops` sets its text matrix before anything that depends
/// on where `BT` put the text and line matrices (showing text, `Td`, `T*`, `'`, `"`).
fn positions_first(ops: &[Op]) -> bool {
    for op in ops {
        match op {
            Op::SetTextMatrix { .. } => return true,
            Op::ShowText { .. }
            | Op::SetTextCursor { .. }
            | Op::AddLineBreak
            | Op::MoveTextCursorAndSetLeading { .. }
            | Op::MoveToNextLineShowText { .. }
            | Op::SetSpacingMoveAndShowText { .. } => return false,
            Op::EndTextSection => return true, // shows nothing
            _ => {}
        }
    }
    true
}

/// The UTF-16BE `/ActualText` of a `/Span` section that carries nothing else.
fn actual_text(op: &Op) -> Option<&[u8]> {
    let Op::BeginMarkedContentWithProperties { tag, properties: DictItem::Dict { map } } = op else {
        return None;
    };
    if tag != "Span" || map.len() != 1 {
        return None;
    }
    match map.get("ActualText") {
        Some(DictItem::String { data, literal: false }) if data.starts_with(&[0xFE, 0xFF]) => Some(data),
        _ => None,
    }
}

/// Baseline (`Tm` translation y) of the first glyphs in the marked-content section that starts
/// at `ops` (just after its `BDC`).
fn span_baseline(ops: &[Op]) -> Option<f32> {
    let mut depth = 0usize;
    for op in ops {
        match op {
            Op::SetTextMatrix { matrix } if depth == 0 => return Some(matrix.as_array()[5]),
            Op::BeginMarkedContent { .. } | Op::BeginMarkedContentWithProperties { .. } => depth += 1,
            Op::EndMarkedContent | Op::EndMarkedContentWithProperties => {
                if depth == 0 {
                    return None;
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    None
}

/// The page's ops with fewer text objects, redundant state and `/ActualText` spans.
pub(crate) fn merge_text_runs(ops: &[Op]) -> Vec<Op> {
    merge_actual_text_spans(merge_text_objects(ops))
}

fn merge_text_objects(ops: &[Op]) -> Vec<Op> {
    let mut out = Vec::with_capacity(ops.len());
    let mut font = None;
    let mut fill = None;
    let mut i = 0;
    while let Some(op) = ops.get(i) {
        if let Op::EndTextSection = op {
            let mut j = i + 1;
            while ops.get(j).is_some_and(is_text_state) {
                j += 1;
            }
            if matches!(ops.get(j), Some(Op::StartTextSection))
                && ops.get(j + 1..).is_some_and(positions_first)
            {
                // keep the text object open: the state ops stay, ET and BT go
                for state in ops.get(i + 1..j).unwrap_or_default() {
                    push_state(state, &mut out, &mut font, &mut fill);
                }
                i = j + 1;
                continue;
            }
        }
        match op {
            Op::SetFont { .. } | Op::SetFillColor { .. } => push_state(op, &mut out, &mut font, &mut fill),
            Op::RestoreGraphicsState | Op::LoadGraphicsState { .. } => {
                font = None;
                fill = None;
                out.push(op.clone());
            }
            _ => out.push(op.clone()),
        }
        i += 1;
    }
    out
}

/// Pushes a state op unless it sets the font or fill colour already in effect.
fn push_state(
    op: &Op,
    out: &mut Vec<Op>,
    font: &mut Option<(crate::ops::PdfFontHandle, crate::Pt)>,
    fill: &mut Option<crate::Color>,
) {
    match op {
        Op::SetFont { font: f, size } => {
            let state = Some((f.clone(), *size));
            if *font == state {
                return;
            }
            *font = state;
        }
        Op::SetFillColor { col } => {
            if fill.as_ref() == Some(col) {
                return;
            }
            *fill = Some(col.clone());
        }
        _ => {}
    }
    out.push(op.clone());
}

fn merge_actual_text_spans(ops: Vec<Op>) -> Vec<Op> {
    let mut out: Vec<Op> = Vec::with_capacity(ops.len());
    // open marked-content sections: for an /ActualText span, its BDC's index in `out` and baseline
    let mut open: Vec<Option<(usize, Option<f32>)>> = Vec::new();
    // the last closed /ActualText span, while only text state has followed its EMC
    let mut previous: Option<(usize, usize, Option<f32>)> = None; // (BDC index, EMC index, baseline)
    for (k, op) in ops.iter().enumerate() {
        let span_text = actual_text(op);
        match op {
            Op::BeginMarkedContentWithProperties { .. } if span_text.is_some() => {
                let baseline = span_baseline(ops.get(k + 1..).unwrap_or_default());
                if let Some((bdc, emc, prev_baseline)) = previous.take() {
                    if baseline.is_some() && baseline == prev_baseline {
                        // `actual_text` only accepts text that starts with the byte order mark.
                        let more = span_text.and_then(|text| text.get(2..)).unwrap_or_default();
                        // `bdc` and `emc` were taken from `out`, which only grew since.
                        if emc < out.len() {
                            out.remove(emc);
                        }
                        if let Some(span_start) = out.get_mut(bdc) {
                            append_actual_text(span_start, more);
                        }
                        open.push(Some((bdc, baseline)));
                        continue;
                    }
                }
                open.push(Some((out.len(), baseline)));
                out.push(op.clone());
            }
            Op::BeginMarkedContent { .. } | Op::BeginMarkedContentWithProperties { .. } => {
                previous = None;
                open.push(None);
                out.push(op.clone());
            }
            Op::EndMarkedContent | Op::EndMarkedContentWithProperties => {
                out.push(op.clone());
                previous = match open.pop() {
                    Some(Some((bdc, baseline))) => {
                        Some((bdc, out.len().saturating_sub(1), baseline))
                    }
                    _ => None,
                };
            }
            _ => {
                if !is_text_state(op) {
                    previous = None;
                }
                out.push(op.clone());
            }
        }
    }
    out
}

fn append_actual_text(bdc: &mut Op, more: &[u8]) {
    if let Op::BeginMarkedContentWithProperties { properties: DictItem::Dict { map }, .. } = bdc {
        if let Some(DictItem::String { data, .. }) = map.get_mut("ActualText") {
            data.extend_from_slice(more);
        }
    }
}

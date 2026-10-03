//! The text of a text box, edited in place with AzWriter's rich text model
//! (`crate::ir`): the deck's [`TextBody`] converts to an `IrDocument` (one
//! paragraph block per paragraph), the IR renders the contenteditable DOM
//! (`ir::to_content_dom`), the engine's typing is synced back run by run
//! (`ir::set_run_text` / `ir::sync_block_text`), Enter / Backspace across
//! paragraphs are mirrored with `ir::apply_operation`, and Bold / Italic /
//! Underline over a selection with `ir::toggle_format_range`. What the IR
//! does not hold - a paragraph's bullet and level - rides along by
//! paragraph index and follows splits and merges.

use azul::{
    callbacks::CallbackInfo,
    css::DocumentOperation,
    dom::{Dom, DomNodeId, NodeType},
};
use crate::ir::{self, FormatAxis, IrAlign, IrBlock, IrDocument, IrParaStyle, IrParagraph, IrRun};

use crate::model::{Align, Paragraph, Run, TextBody};

/// The DOM id of paragraph `index` of element `element`'s text.
#[must_use]
pub fn block_id(element: u64, index: usize) -> String {
    format!("{}{element}-{index}", crate::ids::TEXT_PREFIX)
}

/// The DOM id of element `element`'s text host (the contenteditable root).
#[must_use]
pub fn host_id(element: u64) -> String {
    format!("{}{element}", crate::ids::TEXT_PREFIX)
}

fn ir_align(a: Align) -> IrAlign {
    match a {
        Align::Left => IrAlign::Left,
        Align::Center => IrAlign::Center,
        Align::Right => IrAlign::Right,
        Align::Justify => IrAlign::Justify,
    }
}

fn model_align(a: IrAlign) -> Align {
    match a {
        IrAlign::Left => Align::Left,
        IrAlign::Center => Align::Center,
        IrAlign::Right => Align::Right,
        IrAlign::Justify => Align::Justify,
    }
}

/// The body as the IR: one paragraph block per paragraph.
#[must_use]
pub fn to_ir(body: &TextBody) -> IrDocument {
    IrDocument {
        blocks: body
            .paragraphs
            .iter()
            .map(|p| {
                IrBlock::Paragraph(IrParagraph {
                    style: IrParaStyle::Body,
                    align: ir_align(p.align),
                    runs: p
                        .runs
                        .iter()
                        .map(|r| IrRun {
                            text: r.text.clone(),
                            bold: r.bold,
                            italic: r.italic,
                            underline: r.underline,
                            strike: r.strike,
                            code: false,
                            link: None,
                        })
                        .collect(),
                })
            })
            .collect(),
    }
}

/// The IR back into the body's paragraphs; paragraph `i` takes bullet and
/// level from `metas[i]` (the last one for paragraphs past its end).
pub fn set_from_ir(body: &mut TextBody, doc: &IrDocument, metas: &[(bool, u8)]) {
    let fallback = metas.last().copied().unwrap_or((false, 0));
    body.paragraphs = doc
        .blocks
        .iter()
        .enumerate()
        .map(|(i, block)| {
            let (bullet, level) = metas.get(i).copied().unwrap_or(fallback);
            let (align, runs) = match block {
                IrBlock::Paragraph(p) => (model_align(p.align), p.runs.clone()),
                IrBlock::List(l) => (
                    Align::Left,
                    l.items.iter().flat_map(|it| it.runs.clone()).collect(),
                ),
                _ => (Align::Left, Vec::new()),
            };
            Paragraph {
                runs: runs
                    .into_iter()
                    .filter(|r| !r.text.is_empty())
                    .map(|r| Run {
                        text: r.text,
                        bold: r.bold,
                        italic: r.italic,
                        underline: r.underline,
                        strike: r.strike,
                    })
                    .collect(),
                align,
                bullet,
                level,
            }
        })
        .collect();
}

fn metas(body: &TextBody) -> Vec<(bool, u8)> {
    body.paragraphs.iter().map(|p| (p.bullet, p.level)).collect()
}

/// The CSS of a paragraph block at `scale`: no UA margin, a gap after it,
/// the level's indent (bullets hang in it).
fn paragraph_css(p: &Paragraph, size: f32, scale: f32) -> String {
    let indent = if p.bullet {
        (f32::from(p.level) + 1.0) * size * 1.2 * scale
    } else {
        f32::from(p.level) * size * 1.2 * scale
    };
    format!(
        "margin: 0px 0px {gap:.1}px {indent:.1}px; padding: 0px;",
        gap = size * 0.25 * scale,
    )
}

/// The text's DOM: the IR's content DOM, each block a `<p>` (or an `<li>`
/// for a bullet) carrying its DOM id; contenteditable when `editable`.
#[must_use]
pub fn text_dom(body: &TextBody, element: u64, editable: bool, scale: f32) -> Dom {
    let mut root = ir::to_content_dom(&to_ir(body), "");
    root.set_contenteditable(editable);
    for (i, child) in root.children.as_slice_mut().iter_mut().enumerate() {
        let Some(p) = body.paragraphs.get(i) else {
            continue;
        };
        let block = child.swap_with_default();
        *child = block
            .with_node_type(if p.bullet { NodeType::Li } else { NodeType::P })
            .with_id(block_id(element, i))
            .with_css(paragraph_css(p, body.size, scale));
    }
    root.with_id(host_id(element))
}

/// Mirrors the engine's typing into `body` (the element `element` is being
/// edited): every unsynced edit inside one of its paragraphs updates that
/// paragraph's run. Marks the edits synced. Whether anything changed.
pub fn sync_typing(body: &mut TextBody, element: u64, info: &mut CallbackInfo) -> bool {
    let edits = info.get_unsynced_text_edits();
    let edits = edits.as_ref();
    if edits.is_empty() {
        return false;
    }
    let mut doc = to_ir(body);
    let mut changed = false;
    let mut max_revision = 0u64;
    for edit in edits {
        max_revision = max_revision.max(edit.revision);
        let text = edit.text.as_str();
        for i in 0..doc.blocks.len() {
            let id = block_id(element, i);
            let block_node = info.get_node_id_by_id_attribute(edit.node.dom, id.as_str());
            if block_node.into_raw() == 0 {
                continue;
            }
            let block = DomNodeId {
                dom: edit.node.dom,
                node: block_node,
            };
            let Some(rel) = info.get_node_child_index_path(block, edit.node).into_option() else {
                continue;
            };
            changed |= match rel.as_ref().first() {
                Some(&run) => ir::set_run_text(&mut doc, i, run as usize, text),
                None => ir::sync_block_text(&mut doc, &[i as u32], text),
            };
            break;
        }
    }
    info.mark_text_revision_synced(max_revision);
    if changed {
        let m = metas(body);
        set_from_ir(body, &doc, &m);
    }
    changed
}

/// Mirrors a structural edit (Enter splits a paragraph, Backspace / Delete
/// at a paragraph's edge merges two) onto `body`; the bullet and level of a
/// split paragraph go to both halves. `resume` is the edit's resume path
/// relative to the text host. The inverse operation and its resume path
/// (for the engine's undo), or `None` when the IR cannot mirror it.
pub fn apply_structural(
    body: &mut TextBody,
    op: &DocumentOperation,
    resume: &[u32],
) -> Option<(DocumentOperation, Vec<u32>)> {
    let mut doc = to_ir(body);
    let inverse = ir::apply_operation(&mut doc, op, resume)?;
    let mut m = metas(body);
    let last = resume.last().copied().unwrap_or(0) as usize;
    match op {
        DocumentOperation::SplitNode(_) => {
            let at = last.saturating_sub(1).min(m.len().saturating_sub(1));
            let meta = m.get(at).copied().unwrap_or((false, 0));
            m.insert((at + 1).min(m.len()), meta);
        }
        DocumentOperation::MergeNodes(_) => {
            if last + 1 < m.len() {
                m.remove(last + 1);
            }
        }
        _ => {}
    }
    set_from_ir(body, &doc, &m);
    Some(inverse)
}

/// Toggles `axis` over bytes `start..end` of paragraph `paragraph`.
pub fn toggle_range(body: &mut TextBody, paragraph: usize, start: usize, end: usize, axis: FormatAxis) -> bool {
    let mut doc = to_ir(body);
    if !ir::toggle_format_range(&mut doc, paragraph, start, end, axis) {
        return false;
    }
    let m = metas(body);
    set_from_ir(body, &doc, &m);
    true
}

/// Toggles `axis` over the whole text (a selected box, not in editing):
/// all of it takes the format unless all of it has it already.
pub fn toggle_all(body: &mut TextBody, axis: FormatAxis) {
    let has = |r: &Run| match axis {
        FormatAxis::Bold => r.bold,
        FormatAxis::Italic => r.italic,
        FormatAxis::Underline => r.underline,
        FormatAxis::Strike => r.strike,
    };
    let all = body
        .paragraphs
        .iter()
        .flat_map(|p| p.runs.iter())
        .all(has);
    for run in body.paragraphs.iter_mut().flat_map(|p| p.runs.iter_mut()) {
        match axis {
            FormatAxis::Bold => run.bold = !all,
            FormatAxis::Italic => run.italic = !all,
            FormatAxis::Underline => run.underline = !all,
            FormatAxis::Strike => run.strike = !all,
        }
    }
}

/// Whether every run of the text has `axis` (the ribbon's toggle state).
#[must_use]
pub fn all_have(body: &TextBody, axis: FormatAxis) -> bool {
    let mut runs = body.paragraphs.iter().flat_map(|p| p.runs.iter()).peekable();
    if runs.peek().is_none() {
        return false;
    }
    runs.all(|r| match axis {
        FormatAxis::Bold => r.bold,
        FormatAxis::Italic => r.italic,
        FormatAxis::Underline => r.underline,
        FormatAxis::Strike => r.strike,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body() -> TextBody {
        let mut b = TextBody::plain("one\ntwo", 32.0);
        b.paragraphs[1].bullet = true;
        b.paragraphs[1].level = 1;
        b.paragraphs[0].runs[0].bold = true;
        b.paragraphs[0].align = Align::Center;
        b
    }

    #[test]
    fn a_text_body_survives_the_trip_through_the_rich_text_model() {
        let b = body();
        let mut back = TextBody {
            size: b.size,
            ..TextBody::default()
        };
        set_from_ir(&mut back, &to_ir(&b), &metas(&b));
        assert_eq!(back, b);
    }

    #[test]
    fn whole_box_bold_turns_on_unless_everything_is_bold() {
        let mut b = body();
        assert!(!all_have(&b, FormatAxis::Bold));
        toggle_all(&mut b, FormatAxis::Bold);
        assert!(all_have(&b, FormatAxis::Bold));
        toggle_all(&mut b, FormatAxis::Bold);
        assert!(!all_have(&b, FormatAxis::Bold));
    }

    #[test]
    fn a_range_toggle_formats_only_its_bytes() {
        let mut b = TextBody::plain("hello world", 32.0);
        assert!(toggle_range(&mut b, 0, 6, 11, FormatAxis::Italic));
        let runs = &b.paragraphs[0].runs;
        assert_eq!(runs.len(), 2);
        assert_eq!((runs[0].text.as_str(), runs[0].italic), ("hello ", false));
        assert_eq!((runs[1].text.as_str(), runs[1].italic), ("world", true));
    }

    #[test]
    fn the_dom_ids_name_the_element_and_the_paragraph() {
        assert_eq!(block_id(42, 3), "__azshow_tb42-3");
        assert_eq!(host_id(42), "__azshow_tb42");
    }
}

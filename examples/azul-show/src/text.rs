//! The text of a text box, on azul's shared rich-text editor: the deck's
//! [`TextBody`] is shown and edited as a `RichTextDoc` (the model AzWriter,
//! AzNotes and AzMail's compose window edit) in a `RichTextEditor` - read-only
//! on the slide, in the rail, the show and the PDF; editable while the box is
//! edited, with the editor's own typing, Enter / Backspace across paragraphs,
//! formats and its ONE undo history. Every edit comes back as the editor's
//! state ([`set_from_rich`] folds it into the body).
//!
//! One block per paragraph: a bulleted paragraph is a `Bullet(level)` item,
//! the others are paragraphs. What the document does not hold - the level of
//! a paragraph without a bullet - stays with the paragraph at its index. Kinds
//! a slide's text has no place for (a heading, a numbered or check item, ...)
//! come back as paragraphs or bullets.

use azul::{
    dom::Dom,
    option::OptionString,
    str::String as AzString,
    widgets::{
        RichAlign, RichBlock, RichBlockKind, RichFormat, RichFormats, RichRun, RichTextDoc, RichTextEditor,
        RichTextEditorState,
    },
};

use crate::model::{Align, Paragraph, Run, TextBody};

/// A slide's line spacing (PowerPoint's single spacing).
pub const LINE_HEIGHT: f32 = 1.15;

/// The deepest list level a paragraph takes (PowerPoint's five).
pub const MAX_LEVEL: u8 = 4;

/// The DOM id of paragraph `index` of element `element`'s text (the
/// editor's `<host id>-<index>`).
#[must_use]
pub fn block_id(element: u64, index: usize) -> String {
    format!("{}-{index}", host_id(element))
}

/// The DOM id of element `element`'s text host (the editing host).
#[must_use]
pub fn host_id(element: u64) -> String {
    format!("{}{element}", crate::ids::TEXT_PREFIX)
}

/// The DOM id of cell `row`, `col` of table `element` (edited in place).
#[must_use]
pub fn cell_id(element: u64, row: usize, col: usize) -> String {
    format!("{}-{row}-{col}", host_id(element))
}

fn rich_align(a: Align) -> RichAlign {
    match a {
        Align::Left => RichAlign::Left,
        Align::Center => RichAlign::Center,
        Align::Right => RichAlign::Right,
        Align::Justify => RichAlign::Justify,
    }
}

fn model_align(a: RichAlign) -> Align {
    match a {
        RichAlign::Left => Align::Left,
        RichAlign::Center => Align::Center,
        RichAlign::Right => Align::Right,
        RichAlign::Justify => Align::Justify,
    }
}

/// The body as a rich-text document: one block per paragraph.
#[must_use]
pub fn to_rich(body: &TextBody) -> RichTextDoc {
    let blocks: Vec<RichBlock> = body
        .paragraphs
        .iter()
        .map(|p| RichBlock {
            kind: if p.bullet {
                RichBlockKind::Bullet(p.level.min(MAX_LEVEL))
            } else {
                RichBlockKind::Paragraph
            },
            runs: p
                .runs
                .iter()
                .filter(|r| !r.text.is_empty())
                .map(|r| RichRun {
                    text: AzString::from(r.text.as_str()),
                    link: OptionString::None,
                    formats: RichFormats {
                        bold: r.bold,
                        italic: r.italic,
                        underline: r.underline,
                        strike: r.strike,
                        code: false,
                    },
                })
                .collect::<Vec<RichRun>>()
                .into(),
            align: rich_align(p.align),
            quote_depth: 0,
        })
        .collect();
    RichTextDoc::create_from_blocks(blocks)
}

/// `doc` (the editor's document) back into the body's paragraphs. A
/// paragraph without a bullet keeps the level of the paragraph that stood at
/// its index. Whether the body changed.
pub fn set_from_rich(body: &mut TextBody, doc: &RichTextDoc) -> bool {
    let old_levels: Vec<Option<u8>> = body
        .paragraphs
        .iter()
        .map(|p| (!p.bullet).then_some(p.level))
        .collect();
    let paragraphs: Vec<Paragraph> = doc
        .blocks
        .as_ref()
        .iter()
        .enumerate()
        .map(|(i, block)| {
            let (bullet, level) = match &block.kind {
                RichBlockKind::Bullet(level) | RichBlockKind::Numbered(level) => (true, *level),
                RichBlockKind::Check(check) => (true, check.indent),
                _ => (false, old_levels.get(i).copied().flatten().unwrap_or(0)),
            };
            Paragraph {
                runs: block
                    .runs
                    .as_ref()
                    .iter()
                    .filter(|r| !r.text.as_str().is_empty())
                    .map(|r| Run {
                        text: r.text.as_str().to_string(),
                        bold: r.formats.bold,
                        italic: r.formats.italic,
                        underline: r.formats.underline,
                        strike: r.formats.strike,
                    })
                    .collect(),
                align: model_align(block.align),
                bullet,
                level: level.min(MAX_LEVEL),
            }
        })
        .collect();
    if paragraphs == body.paragraphs {
        return false;
    }
    body.paragraphs = paragraphs;
    true
}

/// The editor state of element `element`'s text: `stored` (the state the
/// editor reported last, with its history) while it still shows what the
/// body holds, else a fresh one over the body (the body changed under the
/// editor: an undo, a ribbon command on the whole box, a replace).
#[must_use]
pub fn state_for(body: &TextBody, element: u64, stored: Option<&RichTextEditorState>) -> RichTextEditorState {
    let doc = to_rich(body);
    if let Some(state) = stored.filter(|s| s.doc == doc && s.host_id.as_str() == host_id(element)) {
        return state.clone();
    }
    let mut state = RichTextEditorState::create(doc);
    state.host_id = AzString::from(host_id(element));
    state
}

/// The editor of a text at `scale` px per slide unit over `state`: the
/// body's size, a slide's spacing, no Markdown shortcuts (a slide's text has
/// no headings, quotes or code); read-only unless `editable`.
#[must_use]
pub fn editor(body: &TextBody, state: RichTextEditorState, scale: f32, editable: bool) -> RichTextEditor {
    RichTextEditor::create(state)
        .with_accessibility_name("Text")
        .with_font_size((body.size * scale).max(0.5))
        .with_paragraph_spacing(body.size * 0.25 * scale)
        .with_line_height(LINE_HEIGHT)
        .with_markdown_shortcuts(false)
        .with_read_only(!editable)
}

/// The text's DOM inside its box (the box places it and sets its font and
/// ink): the editor's content, without the editor's own padding, as tall as
/// its lines (the box aligns it top, middle or bottom).
#[must_use]
pub fn content(editor: RichTextEditor) -> Dom {
    editor
        .content_dom()
        .with_css("padding: 0px; flex-grow: 0; width: 100%; cursor: text;")
}

/// Toggles `format` over the whole text (a selected box, not in editing):
/// all of it takes the format unless all of it has it already.
pub fn toggle_all(body: &mut TextBody, format: RichFormat) {
    let all = all_have(body, format);
    for run in body.paragraphs.iter_mut().flat_map(|p| p.runs.iter_mut()) {
        if let Some(flag) = flag_of(run, format) {
            *flag = !all;
        }
    }
}

/// The run's flag for `format` (`None`: a slide's text has no inline code).
fn flag_of(run: &mut Run, format: RichFormat) -> Option<&mut bool> {
    match format {
        RichFormat::Bold => Some(&mut run.bold),
        RichFormat::Italic => Some(&mut run.italic),
        RichFormat::Underline => Some(&mut run.underline),
        RichFormat::Strike => Some(&mut run.strike),
        RichFormat::Code => None,
    }
}

/// Whether every run of the text has `format` (the ribbon's toggle state).
#[must_use]
pub fn all_have(body: &TextBody, format: RichFormat) -> bool {
    let mut runs = body.paragraphs.iter().flat_map(|p| p.runs.iter()).peekable();
    if runs.peek().is_none() {
        return false;
    }
    runs.all(|r| match format {
        RichFormat::Bold => r.bold,
        RichFormat::Italic => r.italic,
        RichFormat::Underline => r.underline,
        RichFormat::Strike => r.strike,
        RichFormat::Code => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body() -> TextBody {
        let mut b = TextBody::plain("one\ntwo\nthree", 32.0);
        b.paragraphs[1].bullet = true;
        b.paragraphs[1].level = 1;
        b.paragraphs[2].level = 2;
        b.paragraphs[0].runs[0].bold = true;
        b.paragraphs[0].align = Align::Center;
        b
    }

    #[test]
    fn a_text_body_survives_the_trip_through_the_shared_rich_text_model() {
        let b = body();
        let mut back = b.clone();
        assert!(!set_from_rich(&mut back, &to_rich(&b)), "nothing changed");
        assert_eq!(back, b);
        let doc = to_rich(&b);
        let blocks = doc.blocks.as_ref();
        assert!(matches!(blocks[1].kind, RichBlockKind::Bullet(1)), "a bullet is a list item at its level");
        assert!(matches!(blocks[0].kind, RichBlockKind::Paragraph));
        assert!(blocks[0].runs.as_ref()[0].formats.bold);
    }

    #[test]
    fn an_edit_from_the_editor_comes_back_into_the_body() {
        let mut b = body();
        let edited = RichTextDoc::create_from_blocks(vec![
            RichBlock::create_text(RichBlockKind::Paragraph, AzString::from("one!")),
            RichBlock::create_text(RichBlockKind::Numbered(3), AzString::from("two")),
            RichBlock::create_text(RichBlockKind::Heading(1), AzString::from("three")),
        ]);
        assert!(set_from_rich(&mut b, &edited));
        assert_eq!(b.paragraphs[0].runs[0].text, "one!");
        assert!(b.paragraphs[1].bullet && b.paragraphs[1].level == 3, "a numbered item is a bullet");
        assert!(!b.paragraphs[2].bullet, "a heading is a paragraph");
        assert_eq!(b.paragraphs[2].level, 2, "an unbulleted paragraph keeps its level");
    }

    #[test]
    fn the_stored_editor_state_is_used_only_while_it_shows_the_body() {
        let b = body();
        let fresh = state_for(&b, 7, None);
        assert_eq!(fresh.host_id.as_str(), "__azshow_tb7");
        assert!(fresh.doc == to_rich(&b));
        let mut kept = fresh.clone();
        kept.revision = 42;
        assert_eq!(state_for(&b, 7, Some(&kept)).revision, 42, "the editor's own state, history and all");
        let mut changed = b.clone();
        changed.paragraphs[0].runs[0].text = String::from("changed elsewhere");
        assert_eq!(state_for(&changed, 7, Some(&kept)).revision, 0, "the body changed under the editor");
    }

    #[test]
    fn whole_box_bold_turns_on_unless_everything_is_bold() {
        let mut b = body();
        assert!(!all_have(&b, RichFormat::Bold));
        toggle_all(&mut b, RichFormat::Bold);
        assert!(all_have(&b, RichFormat::Bold));
        toggle_all(&mut b, RichFormat::Bold);
        assert!(!all_have(&b, RichFormat::Bold));
    }

    #[test]
    fn the_dom_ids_name_the_element_and_the_paragraph() {
        assert_eq!(block_id(42, 3), "__azshow_tb42-3");
        assert_eq!(host_id(42), "__azshow_tb42");
        assert_eq!(cell_id(42, 1, 2), "__azshow_tb42-1-2");
    }
}

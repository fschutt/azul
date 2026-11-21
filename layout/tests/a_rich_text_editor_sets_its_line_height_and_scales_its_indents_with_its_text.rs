//! A rich-text editor takes the line height its app gives it, and its list indents scale with
//! its text.
//!
//! AzShow's text boxes move onto the shared editor (OFFICE7, wave 7: its twin rich-text model
//! `ir.rs` goes). A slide's text is set at PowerPoint's single spacing (1.15 of the font size),
//! and every view of a slide - the canvas at its zoom, a 150 px rail thumbnail, the show, the
//! PDF - is the same text at another font size. The editor drew every line at 1.5 times the
//! font size, fixed, and indented a list item by 26 + 24 px per level whatever the font size:
//! at a thumbnail's 2.5 px text a bullet item started 26 px in, past the thumbnail's middle; at
//! AzWriter's 200 % zoom its lists kept their 100 % indents.
//!
//! So: `RichTextEditor::with_line_height(factor)` (0 keeps the editor's 1.5), and an indent is
//! proportional to the font size (at the editor's 14 px default exactly the 26 + 24 px per
//! level it was).

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId},
    geom::LogicalRect,
    styled_dom::NodeHierarchyItemId,
};
use azul_css::AzString;
use azul_layout::{
    widgets::{
        rich_text::doc::{RichBlock, RichBlockKind, RichTextDoc},
        rich_text_editor::{RichTextEditor, RichTextEditorState},
    },
    window::LayoutWindow,
};

use crate::editing_harness::lay_out;

/// A paragraph (`#ed-0`), a bullet item (`#ed-1`) and its child at level 1
/// (`#ed-2`), at `font_px`. The level-1 item needs its parent: the model
/// clamps a list item to one level deeper than the item before it
/// (`RichTextDoc::normalize`), so a level-1 item right after a paragraph
/// is a level-0 one.
fn editor(font_px: f32, line_height: Option<f32>) -> Dom {
    let doc = RichTextDoc::from_blocks(vec![
        RichBlock::paragraph("One"),
        RichBlock::text(RichBlockKind::Bullet(0), "Two"),
        RichBlock::text(RichBlockKind::Bullet(1), "Three"),
    ]);
    let mut state = RichTextEditorState::create(doc);
    state.host_id = AzString::from("ed");
    let mut editor = RichTextEditor::create(state)
        .with_font_size(font_px)
        .with_paragraph_spacing(0.0)
        .with_read_only(true);
    if let Some(factor) = line_height {
        editor = editor.with_line_height(factor);
    }
    Dom::create_body()
        .with_css("margin: 0px;")
        .with_child(editor.content_dom().with_css("padding: 0px;"))
}

/// The border box of the node with the id `id`.
fn rect(lw: &LayoutWindow, id: &str) -> LogicalRect {
    let result = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the root DOM is laid out");
    let index = result
        .styled_dom
        .node_data
        .as_container()
        .internal
        .iter()
        .position(|node| node.has_id(id))
        .unwrap_or_else(|| panic!("no node #{id}"));
    lw.get_node_layout_rect(DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(index))),
    })
    .unwrap_or_else(|| panic!("#{id} has no layout rect"))
}

#[test]
fn a_rich_text_editor_takes_the_line_height_its_app_gives_it() {
    let loose = lay_out(editor(20.0, None));
    let single = lay_out(editor(20.0, Some(1.15)));
    // The editor's own spacing: 1.5 x 20 px.
    assert!(
        (rect(&loose, "ed-0").size.height - 30.0).abs() < 1.0,
        "the default line is {} px, 1.5 x 20 px is 30",
        rect(&loose, "ed-0").size.height
    );
    // 1.15 x 20 px = 23 px.
    let line = rect(&single, "ed-0").size.height;
    assert!(
        (line - 23.0).abs() < 1.0,
        "with_line_height(1.15) at 20 px: the line is {line} px, not 23"
    );
}

#[test]
fn a_rich_text_editors_list_indent_scales_with_its_text() {
    let at_14 = lay_out(editor(14.0, None));
    let at_28 = lay_out(editor(28.0, None));
    let at_3 = lay_out(editor(3.5, None));
    let indent = |lw: &LayoutWindow| rect(lw, "ed-2").origin.x - rect(lw, "ed-0").origin.x;
    // At the editor's 14 px: 26 + 24 x 1 px, as it always was.
    assert!(
        (indent(&at_14) - 50.0).abs() < 1.0,
        "a level-1 item at 14 px is indented {} px, not 50",
        indent(&at_14)
    );
    assert!(
        (indent(&at_28) - 100.0).abs() < 1.5,
        "at 28 px the indent is {} px, twice the 14 px one is 100",
        indent(&at_28)
    );
    assert!(
        (indent(&at_3) - 12.5).abs() < 1.0,
        "at a thumbnail's 3.5 px the indent is {} px, a quarter of the 14 px one is 12.5",
        indent(&at_3)
    );
}

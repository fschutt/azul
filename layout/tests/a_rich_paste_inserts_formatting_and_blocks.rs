//! Paste keeps what the source copied: bold, italic, underline, strike,
//! links, paragraphs, line breaks, lists and quotes - and never a script, a
//! style sheet or a form.
//!
//! The clipboard's HTML flavour (`ClipboardContent::html`) is what the
//! default paste inserts in a rich editing host. WPT pins the shapes in
//! `editing/data/inserthtml.js` (execCommand("insertHTML"), what a browser's
//! paste runs): inline markup lands in the paragraph at the caret
//! (`foo[]bar` + `<b>abc</b>` -> `foo<b>abc</b>bar`); a first and last
//! pasted paragraph merge with the text before and after the caret, blocks
//! between stand on their own; `<script>` and friends are dropped (the HTML
//! Standard's paste sanitizing, <https://w3c.github.io/clipboard-apis/#writing-to-clipboard>
//! "sanitize" / `editing/data/inserthtml.js`'s script rows).
//!
//! Inline formatting lands through the text pipeline (the overlay; no app
//! model needed, like typing); blocks and links are STRUCTURE, recorded for
//! the app as one `ReplaceChildren` (like Enter). Rows are data: the
//! paragraph with its caret, the HTML and plain flavours, the result.
//!
//! Before: the default paste inserted the plain text only - the HTML never
//! reached the engine, and the styled runs were dropped.

use azul_css::OptionString;
use azul_layout::managers::{
    changeset::DocumentOperation,
    selection::{ClipboardContent, StyledTextRunVec},
};

use crate::editing_harness::{markup_of_fragment, Editor};

/// `body(0) > host(1) > p(2) > ...`
const P: usize = 2;

fn clipboard(html: &str, plain: &str) -> ClipboardContent {
    ClipboardContent {
        plain_text: plain.into(),
        styled_runs: StyledTextRunVec::from_const_slice(&[]),
        html: OptionString::Some(html.into()),
    }
}

fn paste(editor: &mut Editor, html: &str, plain: &str) {
    let _ = editor.lw.paste_clipboard_content(&clipboard(html, plain));
    let _ = editor.lw.apply_text_changeset();
}

/// Inline HTML: the paragraph's content afterwards (the edit model).
const INLINE: &[(&str, &str, &str, &str)] = &[
    // inserthtml.js: `foo[]bar` + `<b>abc</b>`.
    ("<p>ab[]cd</p>", "<b>X</b>Y", "XY", "ab<b>X</b>Ycd"),
    (
        "<p>ab[]cd</p>",
        "<i>X</i><u>Y</u><s>Z</s>",
        "XYZ",
        "ab<i>X</i><u>Y</u><s>Z</s>cd",
    ),
    // `<strong>` / `<em>` / `<strike>` / `<del>` are the same formats.
    (
        "<p>ab[]cd</p>",
        "<strong>X</strong><em>Y</em><del>Z</del>",
        "XYZ",
        "ab<b>X</b><i>Y</i><s>Z</s>cd",
    ),
    // Google Docs: the formats in inline styles, inside a
    // `font-weight:normal` <b> around everything.
    (
        "<p>ab[]cd</p>",
        "<meta charset='utf-8'><b style=\"font-weight:normal;\" id=\"docs-internal-guid-1\">\
         <span style=\"font-weight:700;\">X</span><span style=\"font-style:italic;\">Y</span></b>",
        "XY",
        "ab<b>X</b><i>Y</i>cd",
    ),
    // A line break.
    ("<p>ab[]cd</p>", "X<br>Y", "X\nY", "abX<br>Ycd"),
    // One pasted paragraph is inline content: it merges into the caret's.
    ("<p>ab[]cd</p>", "<p><b>X</b></p>", "X", "ab<b>X</b>cd"),
    // Sanitized away: scripts, style sheets, forms and their controls, and a
    // `javascript:` link (its text stays).
    (
        "<p>ab[]cd</p>",
        "<script>alert(1)</script>X<style>p { color: red }</style>",
        "X",
        "abXcd",
    ),
    (
        "<p>ab[]cd</p>",
        "<form><input value=\"pw\"><button>Sign in</button></form>X",
        "X",
        "abXcd",
    ),
    (
        "<p>ab[]cd</p>",
        "<a href=\"javascript:alert(1)\">X</a>",
        "X",
        "abXcd",
    ),
];

#[test]
fn pasted_inline_html_keeps_its_formatting() {
    for &(input, html, plain, expected) in INLINE {
        let mut editor = Editor::new(input);
        paste(&mut editor, html, plain);
        assert_eq!(editor.markup_of(P), expected, "{input} + paste {html}");
    }
}

/// Blocks and links: the blocks the recorded edit puts where the caret's
/// paragraph (or the selected ones) stood.
const BLOCKS: &[(&str, &str, &str, &str)] = &[
    // inserthtml.js: the first pasted paragraph merges with the text before
    // the caret, the last with the text after it.
    (
        "<p>ab[]cd</p>",
        "<p>X</p><p>Y</p>",
        "X\nY",
        "<p>abX</p><p>Ycd</p>",
    ),
    // A list and a quote stand on their own; the paragraph is split around
    // them.
    (
        "<p>ab[]cd</p>",
        "<ul><li>X</li><li>Y</li></ul>",
        "X\nY",
        "<p>ab</p><ul><li>X</li><li>Y</li></ul><p>cd</p>",
    ),
    (
        "<p>ab[]cd</p>",
        "<blockquote><p>X</p></blockquote>",
        "X",
        "<p>ab</p><blockquote><p>X</p></blockquote><p>cd</p>",
    ),
    // A link keeps its target.
    (
        "<p>ab[]cd</p>",
        "<a href=\"https://example.com/\">X</a>",
        "X",
        "<p>ab<a href=\"https://example.com/\">X</a>cd</p>",
    ),
    // Over a selection across blocks: the pasted content at the join.
    ("<p>a[b</p><p>c]d</p>", "<b>X</b>", "X", "<p>a<b>X</b>d</p>"),
];

#[test]
fn pasted_blocks_and_links_are_recorded_as_structure() {
    for &(input, html, plain, expected) in BLOCKS {
        let mut editor = Editor::new(input);
        paste(&mut editor, html, plain);
        let edit = editor
            .lw
            .get_pending_document_edit()
            .unwrap_or_else(|| panic!("{input} + paste {html}: a structural edit is recorded"));
        let DocumentOperation::ReplaceChildren(replace) = &edit.operation else {
            panic!("{input} + paste {html}: a ReplaceChildren");
        };
        assert_eq!(
            markup_of_fragment(&replace.content),
            expected,
            "{input} + paste {html}"
        );
    }
}

/// With no HTML flavour the paste is the plain text, as before.
#[test]
fn a_paste_without_html_inserts_the_plain_text() {
    let mut editor = Editor::new("<p>ab[]cd</p>");
    let content = ClipboardContent {
        plain_text: "X".into(),
        styled_runs: StyledTextRunVec::from_const_slice(&[]),
        html: OptionString::None,
    };
    let _ = editor.lw.paste_clipboard_content(&content);
    let _ = editor.lw.apply_text_changeset();
    assert_eq!(editor.markup_of(P), "abXcd");
}

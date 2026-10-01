//! The note editor: the [`Doc`] rendered as ONE contenteditable host, and
//! the glue that folds the engine's edits back into the model.
//!
//! The host's children are the blocks (one DOM child per block), a block's
//! children its runs (one DOM child per run: a bare text node for plain
//! text, a `span` with the run's formats otherwise). So the engine's
//! child-index paths ARE model indices: `[block]` from the host, `[block,
//! run]` below it. A check item's box is the one extra child: an
//! absolutely positioned `contenteditable=false` island AFTER the runs
//! (out of the inline formatting context, walled off from the text the
//! engine edits), so the runs keep their indices.
//!
//! The engine edits, the app follows (D4 in scripts/NOTES.PROGRESS.md):
//! - typing: `TextChanged` -> `get_unsynced_text_edits` (each block's text
//!   as the user sees it) -> `Doc::sync_block_text` (the runs keep their
//!   formats) -> ack the revision; no rebuild. A Markdown shortcut typed
//!   at a block's start changes the block and rebuilds.
//! - Enter / Backspace at a block's start / a delete across blocks:
//!   `DocumentEdit` -> the split / merge / replace applied to the model ->
//!   `mark_document_edit_applied` -> rebuild; the engine puts the caret at
//!   the edit's resume point.
//! - keys the browser model leaves to the editor (Enter in a code block,
//!   Enter on an empty list item, Backspace at a list item's start, Tab in
//!   a list, the format and block shortcuts): `VirtualKeyDown` on the host,
//!   `prevent_default` when the app does it.

use std::collections::HashMap;

use azul::{
    callbacks::{CallbackInfo, RefAny, Update},
    css::{BoxOrStaticString, DocumentOperation, EventFilter, FocusEventFilter, HoverEventFilter},
    dom::{AttributeType, Dom, DomId, DomNodeId, NodeId, NodeType, TextFormat, VirtualKeyCode},
    image::ImageRef,
    svg::{CssPath, CssPathSelector},
};

use crate::{
    doc::{Block, BlockKind, Doc, Format, FormatSet, Run},
    look::Look,
};

/// The DOM id of the editing host.
pub const HOST_ID: &str = "note-body";

/// What the editor draws with.
pub struct View<'a> {
    pub doc: &'a Doc,
    /// The note's notebook (image sources are relative to its folder).
    pub notebook: &'a str,
    /// Decoded images by key.
    pub images: &'a HashMap<String, ImageRef>,
    pub look: &'a Look,
    /// The body text size in px.
    pub font_px: f32,
}

// ==== Rendering ====

/// A run as ONE child of its block.
fn run_dom(run: &Run, look: &Look) -> Dom {
    let text = Dom::create_text_do_not_use_without_block_level_wrapper(run.text.as_str());
    if run.is_plain() {
        return text;
    }
    let mut css = String::new();
    if run.bold {
        css.push_str("font-weight: bold;");
    }
    if run.italic {
        css.push_str("font-style: italic;");
    }
    match (run.underline || run.link.is_some(), run.strike) {
        (true, true) => css.push_str("text-decoration: underline line-through;"),
        (true, false) => css.push_str("text-decoration: underline;"),
        (false, true) => css.push_str("text-decoration: line-through;"),
        (false, false) => {}
    }
    if run.code {
        css.push_str(&format!(
            "font-family: monospace; background: {}; border-radius: 3px;",
            look.code_bg
        ));
    }
    if run.link.is_some() {
        css.push_str(&format!("color: {};", look.link));
    }
    Dom::create_span().with_css(css).with_child(text)
}

/// `node` with the runs of `block` as its children (none for an empty
/// block: the editing host's strut gives it its line).
fn with_runs(mut node: Dom, block: &Block, look: &Look) -> Dom {
    for run in &block.runs {
        if !run.text.is_empty() {
            node.add_child(run_dom(run, look));
        }
    }
    node
}

/// The marker of a bullet at `indent`.
fn bullet_style(indent: u8) -> &'static str {
    match indent % 3 {
        0 => "disc",
        1 => "circle",
        _ => "square",
    }
}

/// The counter style of a numbered item at `indent`.
fn number_style(indent: u8) -> &'static str {
    match indent % 3 {
        0 => "decimal",
        1 => "lower-alpha",
        _ => "lower-roman",
    }
}

/// The payload of a check box: which block it ticks.
pub struct BlockRef {
    pub app: RefAny,
    pub block: usize,
}

/// One block as ONE child of the host.
#[allow(clippy::too_many_lines)]
fn block_dom(view: &View<'_>, index: usize, block: &Block, app: &RefAny) -> Dom {
    let look = view.look;
    let px = view.font_px;
    let line = (px * 1.5).round();
    let base = format!(
        "margin: 0px; padding: 0px; white-space: pre-wrap; color: {}; font-size: {px}px; line-height: {line}px; min-height: {line}px;",
        look.text
    );
    let indent_px = |indent: u8| 26.0 + 24.0 * f32::from(indent);
    match &block.kind {
        BlockKind::Paragraph => {
            with_runs(Dom::create_p(), block, look).with_css(format!("{base} margin-bottom: 6px;"))
        }
        BlockKind::Heading(level) => {
            let (node, scale) = match level {
                1 => (Dom::create_h1(), 1.75),
                2 => (Dom::create_h2(), 1.4),
                3 => (Dom::create_h3(), 1.2),
                4 => (Dom::create_h4(), 1.1),
                5 => (Dom::create_h5(), 1.0),
                _ => (Dom::create_h6(), 0.95),
            };
            let size = (px * scale).round();
            with_runs(node, block, look).with_css(format!(
                "margin: 0px; padding: 0px; white-space: pre-wrap; color: {}; font-size: {size}px; \
                 line-height: {}px; font-weight: bold; margin-top: 10px; margin-bottom: 6px;",
                look.text,
                (size * 1.3).round()
            ))
        }
        BlockKind::Bullet(indent) => with_runs(Dom::create_li(), block, look).with_css(format!(
            "{base} display: list-item; list-style-type: {}; margin-left: {}px; margin-bottom: 2px;",
            bullet_style(*indent),
            indent_px(*indent)
        )),
        BlockKind::Numbered(indent) => {
            // The item's own number: a flat list has no `ol` to count in,
            // so each item resets the list-item counter to the number before
            // it (CSS Lists 3: the marker shows the incremented value).
            let number = view.doc.number_of(index);
            with_runs(Dom::create_li(), block, look).with_css(format!(
                "{base} display: list-item; list-style-type: {}; counter-reset: list-item {}; \
                 margin-left: {}px; margin-bottom: 2px;",
                number_style(*indent),
                number.saturating_sub(1),
                indent_px(*indent)
            ))
        }
        BlockKind::Check { indent, checked } => {
            let checked_css = if *checked {
                format!("color: {}; text-decoration: line-through;", look.muted)
            } else {
                String::new()
            };
            let item = with_runs(Dom::create_li(), block, look).with_css(format!(
                "{base} display: list-item; list-style-type: none; position: relative; \
                 padding-left: 28px; margin-left: {}px; margin-bottom: 2px; {checked_css}",
                indent_px(*indent) - 26.0
            ));
            // The box comes AFTER the runs (they keep their child indices),
            // out of the text flow and out of the editable text.
            let icon = if *checked { "check_box" } else { "check_box_outline_blank" };
            let name = if *checked { "Uncheck" } else { "Check" };
            let top = ((line - 20.0) / 2.0).max(0.0);
            item.with_child(
                Dom::create_div()
                    .with_class("note-check")
                    .with_attribute(AttributeType::ContentEditable(false))
                    .with_css(format!(
                        "position: absolute; left: 2px; top: {top}px; width: 20px; height: 20px; \
                         cursor: pointer; color: {};",
                        if *checked { look.muted } else { look.accent }
                    ))
                    .with_accessibility_name(name)
                    .with_callback(
                        EventFilter::Hover(HoverEventFilter::Click),
                        RefAny::new(BlockRef {
                            app: app.clone(),
                            block: index,
                        }),
                        on_check_click,
                    )
                    .with_child(Dom::create_icon(icon).with_css("font-size: 20px;")),
            )
        }
        BlockKind::Quote => with_runs(Dom::create_blockquote(), block, look).with_css(format!(
            "{base} margin-top: 4px; margin-bottom: 8px; padding-left: 12px; border-left: 3px solid {}; \
             color: {};",
            look.line, look.muted
        )),
        BlockKind::Code { .. } => with_runs(Dom::create_pre(), block, look).with_css(format!(
            "margin: 0px; margin-top: 4px; margin-bottom: 8px; padding: 10px 12px; white-space: pre; \
             font-family: monospace; font-size: {}px; line-height: {}px; color: {}; background: {}; \
             border-radius: 6px;",
            (px * 0.9).round(),
            (px * 1.35).round(),
            look.text,
            look.code_bg
        )),
        BlockKind::Rule => Dom::create_hr().with_css(format!(
            "margin: 12px 0px; border: none; border-top: 1px solid {}; height: 0px;",
            look.line
        )),
        BlockKind::Image { src, alt } => {
            let key = crate::store::image_key(view.notebook, src);
            let inner = match key.as_ref().and_then(|k| view.images.get(k)) {
                Some(image) => Dom::create_image(image.clone()).with_css("max-width: 100%;"),
                None => Dom::create_div()
                    .with_css(format!(
                        "padding: 16px; border: 1px dashed {}; color: {}; font-size: 13px;",
                        look.line, look.muted
                    ))
                    .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(format!(
                        "Image: {}",
                        if alt.is_empty() { src.as_str() } else { alt.as_str() }
                    ))),
            };
            Dom::create_div()
                .with_class("note-image")
                .with_attribute(AttributeType::ContentEditable(false))
                .with_css("margin-top: 6px; margin-bottom: 10px;")
                .with_accessibility_name(if alt.is_empty() { "Image" } else { alt.as_str() })
                .with_child(inner)
        }
    }
}

/// The editing host with every block of the note.
#[must_use]
pub fn host_dom(view: &View<'_>, app: &RefAny) -> Dom {
    let mut host = Dom::create_div()
        .with_id(HOST_ID)
        .with_contenteditable(true)
        .with_accessibility_name("Note text")
        .with_css(format!(
            "display: block; padding-top: 4px; padding-bottom: 48px; color: {}; font-size: {}px; \
             cursor: text;",
            view.look.text, view.font_px
        ));
    for (index, block) in view.doc.blocks.iter().enumerate() {
        host.add_child(block_dom(view, index, block, app));
    }
    host.with_callback(
        EventFilter::Focus(FocusEventFilter::TextChanged),
        app.clone(),
        on_text_changed,
    )
    .with_callback(
        EventFilter::Focus(FocusEventFilter::DocumentEdit),
        app.clone(),
        on_document_edit,
    )
    .with_callback(
        EventFilter::Focus(FocusEventFilter::VirtualKeyDown),
        app.clone(),
        on_editor_key,
    )
}

/// The note as a read-only DOM (the PDF export): the title, then the
/// blocks as the editor draws them.
#[must_use]
pub fn print_dom(view: &View<'_>, title: &str, app: &RefAny) -> Dom {
    let mut body = Dom::create_div().with_css(format!(
        "display: block; color: {}; font-size: {}px; font-family: sans-serif;",
        view.look.text, view.font_px
    ));
    body.add_child(
        Dom::create_h1_with_text(title)
            .with_css(format!("margin: 0px; margin-bottom: 14px; font-size: {}px;", (view.font_px * 2.0).round())),
    );
    for (index, block) in view.doc.blocks.iter().enumerate() {
        body.add_child(block_dom(view, index, block, app));
    }
    body
}

// ==== Mapping engine nodes to the model ====

/// The editing host of the window, if a note is open.
#[must_use]
pub fn host_node(info: &CallbackInfo, dom: DomId) -> Option<DomNodeId> {
    let node = info.get_node_id_by_id_attribute(dom, HOST_ID);
    (node.into_raw() != 0).then_some(DomNodeId { dom, node })
}

/// The DOM of the root window (the host lives there).
#[must_use]
pub const fn root_dom() -> DomId {
    DomId { inner: 0 }
}

/// The child-index path of `node` below the host: `[block]` for a block,
/// `[block, run, ..]` below one.
#[must_use]
pub fn path_in_host(info: &CallbackInfo, host: DomNodeId, node: DomNodeId) -> Option<Vec<u32>> {
    let path = info.get_node_child_index_path(host, node).into_option()?;
    Some(path.as_ref().to_vec())
}

/// The block `node` is (or is in).
#[must_use]
pub fn block_of(info: &CallbackInfo, host: DomNodeId, node: DomNodeId) -> Option<usize> {
    path_in_host(info, host, node)?.first().map(|b| *b as usize)
}

/// The engine's `NodeId` of a `DomNodeId`.
#[must_use]
pub fn node_id(node: DomNodeId) -> Option<NodeId> {
    let raw = node.node.into_raw();
    (raw != 0).then(|| NodeId::create(raw - 1))
}

/// The caret as `(block, byte in the block's text)`, when there is one.
#[must_use]
pub fn caret(info: &CallbackInfo, host: DomNodeId) -> Option<(usize, usize)> {
    let position = info.get_document_caret().into_option()?;
    let block = block_of(info, host, position.node)?;
    Some((block, position.text_byte as usize))
}

/// The selection as `(block, start, end)` spans; empty for a caret.
#[must_use]
pub fn selection(info: &CallbackInfo, host: DomNodeId) -> Vec<(usize, usize, usize)> {
    let spans = info.get_document_selection();
    spans
        .as_ref()
        .iter()
        .filter(|s| s.end_byte > s.start_byte)
        .filter_map(|s| {
            block_of(info, host, s.node).map(|b| (b, s.start_byte as usize, s.end_byte as usize))
        })
        .collect()
}

/// The bytes before run `child` of `block` (a split position given as a
/// child index).
fn bytes_before(block: &Block, child: usize) -> usize {
    block
        .runs
        .iter()
        .filter(|r| !r.text.is_empty())
        .take(child)
        .map(|r| r.text.len())
        .sum()
}

/// The string a text node holds (AzWriter's `document::box_str` is the
/// twin; the generated API hands the text over behind a pointer).
fn box_str(s: &BoxOrStaticString) -> &str {
    // SAFETY: both variants point at the AzString the node owns (or a
    // static one), alive as long as the node.
    unsafe {
        match s {
            BoxOrStaticString::Boxed(p) => (**p).as_str(),
            BoxOrStaticString::Static(p) => (**p).as_str(),
        }
    }
}

/// The text of a DOM subtree (a replacement's content), `<br>` as `\n`.
fn dom_text(dom: &Dom, out: &mut String) {
    match &dom.root.node_type {
        NodeType::Text(text) => out.push_str(box_str(text)),
        NodeType::Br => out.push('\n'),
        _ => {}
    }
    for child in dom.children.as_ref() {
        dom_text(child, out);
    }
}

/// Puts the keyboard focus back into the editor (after a toolbar button
/// took it).
pub fn focus_editor(info: &mut CallbackInfo) {
    info.set_focus_to_path(
        root_dom(),
        CssPath {
            selectors: vec![CssPathSelector::Id(HOST_ID.into())].into(),
        },
    );
}

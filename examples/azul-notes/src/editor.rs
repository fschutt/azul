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
    css::{DocumentOperation, EventFilter, FocusEventFilter, HoverEventFilter},
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
    /// The check boxes tick (the editor); a read-only view (a version, a
    /// print) draws them without.
    pub interactive: bool,
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
    let mut span = Dom::create_span();
    if let Some(url) = &run.link {
        css.push_str(&format!("color: {};", look.link));
        // Ctrl / Cmd + click opens the link (a plain click places the caret).
        span = span.with_callback(
            EventFilter::Hover(HoverEventFilter::Click),
            RefAny::new(LinkRef { url: url.clone() }),
            on_link_click,
        );
    }
    span.with_css(css).with_child(text)
}

/// The payload of a link in the text.
struct LinkRef {
    url: String,
}

/// Opens `url` with the system's handler (the browser, for a web address).
pub fn open_url(url: &str) -> bool {
    match azul::url::Url::parse(url).into_result() {
        Ok(url) => url.open(),
        Err(_) => false,
    }
}

/// Ctrl / Cmd + click on a link in the text opens it.
extern "C" fn on_link_click(mut data: RefAny, info: CallbackInfo) -> Update {
    let modifiers = info.get_key_modifiers();
    if !modifiers.primary_down() {
        return Update::DoNothing;
    }
    let Some(url) = data.downcast_ref::<LinkRef>().map(|l| l.url.clone()) else {
        return Update::DoNothing;
    };
    if open_url(&url) {
        println!("AZNOTES_OPENED_LINK {url}");
    }
    Update::DoNothing
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
            let (node, scale) = match *level {
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
            let mut check = Dom::create_div()
                .with_class("note-check")
                .with_attribute(AttributeType::ContentEditable(false))
                .with_css(format!(
                    "position: absolute; left: 2px; top: {top}px; width: 20px; height: 20px; \
                     cursor: pointer; color: {};",
                    if *checked { look.muted } else { look.accent }
                ))
                .with_accessibility_name(name)
                .with_child(Dom::create_icon(icon).with_css("font-size: 20px;"));
            if view.interactive {
                check = check.with_callback(
                    EventFilter::Hover(HoverEventFilter::Click),
                    RefAny::new(BlockRef {
                        app: app.clone(),
                        block: index,
                    }),
                    on_check_click,
                );
            }
            item.with_child(check)
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
        // `#nb-<index>`: a script (and a test) names a block by its index.
        host.add_child(block_dom(view, index, block, app).with_id(format!("nb-{index}")));
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

/// The text of a DOM subtree (a replacement's content), `<br>` as `\n`.
fn dom_text(dom: &Dom, out: &mut String) {
    match &dom.root.node_type {
        NodeType::Text(_) => {
            if let Some(text) = dom.root.node_type.get_text().into_option() {
                out.push_str(text.as_str());
            }
        }
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

/// The runs of a replacement fragment's block: its text with the formats
/// its inline elements say (b / strong, i / em, u / ins, s / del, code).
fn collect_runs(dom: &Dom, formats: FormatSet, out: &mut Vec<Run>) {
    let mut formats = formats;
    match &dom.root.node_type {
        NodeType::Text(_) => {
            let text = dom.root.node_type.get_text().into_option();
            let mut run = Run::plain(text.as_ref().map_or("", |t| t.as_str()));
            formats.apply_to(&mut run);
            crate::doc::push_run(out, run);
            return;
        }
        NodeType::Br => {
            let mut run = Run::plain("\n");
            formats.apply_to(&mut run);
            crate::doc::push_run(out, run);
            return;
        }
        NodeType::Strong | NodeType::B => formats.bold = true,
        NodeType::Em | NodeType::I => formats.italic = true,
        NodeType::U | NodeType::Ins => formats.underline = true,
        NodeType::S | NodeType::Del => formats.strike = true,
        NodeType::Code => formats.code = true,
        _ => {}
    }
    for child in dom.children.as_ref() {
        collect_runs(child, formats, out);
    }
}

/// The kind a pasted block's element stands for.
fn kind_of_element(node_type: &NodeType) -> Option<BlockKind> {
    Some(match node_type {
        NodeType::P | NodeType::Div => BlockKind::Paragraph,
        NodeType::H1 => BlockKind::Heading(1),
        NodeType::H2 => BlockKind::Heading(2),
        NodeType::H3 => BlockKind::Heading(3),
        NodeType::H4 => BlockKind::Heading(4),
        NodeType::H5 => BlockKind::Heading(5),
        NodeType::H6 => BlockKind::Heading(6),
        NodeType::Li => BlockKind::Bullet(0),
        NodeType::BlockQuote => BlockKind::Quote,
        NodeType::Pre => BlockKind::Code {
            lang: String::new(),
        },
        _ => return None,
    })
}

// ==== The editor's state ====

/// The formats text typed at a caret takes, set by a format toggle at a
/// collapsed caret (the engine keeps its own for what it paints; this is
/// the model's half): kept while text is inserted where the last insertion
/// ended, dropped by anything else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Typing {
    pub block: usize,
    pub at: usize,
    pub formats: FormatSet,
}

/// What the editor remembers between callbacks.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EditorState {
    pub typing: Option<Typing>,
    /// The block the caret was last seen in (the toolbar's target when a
    /// button took the focus).
    pub caret_block: usize,
}

impl EditorState {
    /// The typing style for an edit of `block` from `old` to `new`, moved
    /// past the inserted text; `None` (and dropped) for any other edit.
    pub fn typing_for(&mut self, block: usize, old: &str, new: &str) -> Option<FormatSet> {
        let typing = self.typing.take()?;
        let (prefix, suffix) = crate::doc::text_diff(old, new);
        let removed = old.len() - prefix - suffix;
        let inserted = new.len() - prefix - suffix;
        if typing.block != block || removed != 0 || inserted == 0 || prefix != typing.at {
            return None;
        }
        self.typing = Some(Typing {
            at: prefix + inserted,
            ..typing
        });
        Some(typing.formats)
    }
}

// ==== The callbacks ====

/// Folds the engine's unsynced text edits of the host into the open note
/// and acks them. Returns `(changed, rebuild)`: a Markdown shortcut (with
/// `shortcuts`) or a typing style the engine cannot paint asks for a new
/// DOM.
pub(crate) fn sync_text(state: &mut crate::AppState, info: &mut CallbackInfo, shortcuts: bool) -> (bool, bool) {
    let edits = info.get_unsynced_text_edits();
    let edits = edits.as_ref();
    if edits.is_empty() {
        return (false, false);
    }
    let mut max_revision = 0u64;
    let mut changed = false;
    let mut rebuild = false;
    let open = state.open.clone();
    for edit in edits {
        max_revision = max_revision.max(edit.revision);
        let Some(host) = host_node(info, edit.node.dom) else {
            continue;
        };
        let Some(block) = block_of(info, host, edit.node) else {
            continue; // not the note's text (a text field)
        };
        let Some(note) = open.as_deref().and_then(|id| state.library.get_mut(id)) else {
            continue;
        };
        let Some(old) = note.doc.blocks.get(block).map(Block::flat) else {
            continue;
        };
        let new = edit.text.as_str();
        let typing = state.editor.typing_for(block, &old, new);
        if note.doc.sync_block_text(block, new, typing) {
            changed = true;
            if typing.is_some_and(|t| t.code) {
                rebuild = true;
            }
            if shortcuts {
                let kind = note.doc.blocks[block].kind.clone();
                if let Some(shortcut) = crate::doc::typed_shortcut(&kind, &old, new) {
                    note.doc.apply_shortcut(block, &shortcut);
                    rebuild = true;
                }
            }
        }
        state.editor.caret_block = block;
    }
    info.mark_text_revision_synced(max_revision);
    (changed, rebuild)
}

/// `TextChanged` on the host: the typing goes into the model.
pub extern "C" fn on_text_changed(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut guard) = data.downcast_mut::<crate::AppState>() else {
        return Update::DoNothing;
    };
    let state = &mut *guard;
    let (changed, rebuild) = sync_text(state, &mut info, true);
    if changed {
        state.edited();
    }
    if rebuild {
        Update::RefreshDom
    } else {
        Update::DoNothing
    }
}

/// `DocumentEdit` on the host: Enter's split, Backspace's / Delete's merge,
/// a delete, type-over or paste across blocks - applied to the model, then
/// acknowledged so the engine places the caret at the edit's resume point.
pub extern "C" fn on_document_edit(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(changeset) = info.get_document_edit_clone().into_option() else {
        return Update::DoNothing;
    };
    let Some(mut guard) = data.downcast_mut::<crate::AppState>() else {
        return Update::DoNothing;
    };
    let state = &mut *guard;
    let (synced, _) = sync_text(state, &mut info, false);
    let Some(host) = host_node(&info, changeset.target.dom) else {
        return Update::DoNothing;
    };
    let caret = caret(&info, host);
    let open = state.open.clone();
    let Some(note) = open.as_deref().and_then(|id| state.library.get_mut(id)) else {
        return Update::DoNothing;
    };
    let applied = match &changeset.operation {
        DocumentOperation::SplitNode(split) => block_of(&info, host, split.node)
            .and_then(|b| {
                let at = match caret {
                    Some((cb, byte)) if cb == b => byte,
                    _ => note.doc.blocks.get(b).map_or(0, |block| {
                        bytes_before(block, split.at.child_index as usize)
                            + split.at.text_byte.into_option().unwrap_or(0) as usize
                    }),
                };
                note.doc.split_block(b, at)
            })
            .is_some(),
        DocumentOperation::MergeNodes(merge) => {
            match (block_of(&info, host, merge.first), block_of(&info, host, merge.second)) {
                (Some(first), Some(second)) if second == first + 1 => note
                    .doc
                    .merge_into_previous(second, merge.join.text_byte.into_option().is_some())
                    .is_some(),
                _ => false,
            }
        }
        DocumentOperation::ReplaceChildren(replace)
            if path_in_host(&info, host, replace.parent).is_some_and(|p| p.is_empty()) =>
        {
            let (start, end) = (replace.start as usize, replace.end as usize);
            let parts: Vec<&Dom> = replace.content.children.as_ref().iter().collect();
            if parts.len() <= 1 {
                let mut joined = String::new();
                for part in &parts {
                    dom_text(part, &mut joined);
                }
                note.doc.replace_blocks(start, end, &joined)
            } else {
                let parts = parts
                    .into_iter()
                    .map(|part| {
                        let mut runs = Vec::new();
                        collect_runs(part, FormatSet::default(), &mut runs);
                        (kind_of_element(&part.root.node_type), runs)
                    })
                    .collect();
                note.doc.replace_with(start, end, parts)
            }
        }
        _ => false,
    };
    if applied {
        info.mark_document_edit_applied(changeset.id);
    } else {
        eprintln!("[aznotes] a structural edit the model cannot mirror was dropped");
    }
    if applied || synced {
        state.editor.typing = None;
        state.edited();
    }
    // A new DOM either way: the applied edit, or (dropped) the engine's
    // preview of an edit the model does not take.
    Update::RefreshDom
}

/// The caret's block and byte, else the last block the caret was seen in.
fn target(state: &crate::AppState, info: &CallbackInfo) -> Option<(DomNodeId, usize, usize)> {
    let host = host_node(info, root_dom())?;
    let (block, byte) = caret(info, host).unwrap_or((state.editor.caret_block, 0));
    Some((host, block, byte))
}

/// Toggles `format`: over the selection in the model (a new DOM), or at a
/// caret as the typing style of what is typed next (`engine`: also ask the
/// engine to paint it, when no key default does).
pub fn toggle_format(state: &mut crate::AppState, info: &mut CallbackInfo, format: Format, engine: bool) -> Update {
    let Some((host, block, byte)) = target(state, info) else {
        return Update::DoNothing;
    };
    let spans = selection(info, host);
    let open = state.open.clone();
    let Some(note) = open.as_deref().and_then(|id| state.library.get_mut(id)) else {
        return Update::DoNothing;
    };
    if !spans.is_empty() {
        // Set everywhere when one span lacks it, else clear everywhere.
        let on = !spans.iter().all(|(b, s, e)| note.doc.has_format(*b, *s, *e, format));
        let mut changed = false;
        for (b, s, e) in spans {
            if note.doc.has_format(b, s, e, format) != on {
                changed |= note.doc.toggle_format(b, s, e, format);
            }
        }
        if changed {
            state.edited();
            return Update::RefreshDom;
        }
        return Update::DoNothing;
    }
    let mut formats = FormatSet::default();
    for f in Format::ALL {
        formats.set(f, note.doc.has_format(block, byte, byte, f));
    }
    formats.set(format, !formats.has(format));
    state.editor.typing = Some(Typing {
        block,
        at: byte,
        formats,
    });
    if engine {
        let engine_format = match format {
            Format::Bold => Some(TextFormat::Bold),
            Format::Italic => Some(TextFormat::Italic),
            Format::Underline => Some(TextFormat::Underline),
            Format::Strike => Some(TextFormat::Strikethrough),
            Format::Code => None,
        };
        if let Some(f) = engine_format {
            info.toggle_text_format(host, f);
        }
    }
    Update::DoNothing
}

/// The blocks a block command acts on: those of the selection, else the
/// caret's.
fn command_blocks(state: &crate::AppState, info: &CallbackInfo) -> Vec<usize> {
    let Some((host, block, _)) = target(state, info) else {
        return Vec::new();
    };
    let mut blocks: Vec<usize> = selection(info, host).into_iter().map(|(b, _, _)| b).collect();
    blocks.dedup();
    if blocks.is_empty() {
        blocks.push(block);
    }
    blocks
}

/// The toolbar's block buttons: the target blocks become `kind`, or
/// paragraphs again when the first already is one of that family.
pub fn toggle_kind(state: &mut crate::AppState, info: &mut CallbackInfo, kind: BlockKind) -> Update {
    let blocks = command_blocks(state, info);
    let Some(note) = state.open_note_mut() else {
        return Update::DoNothing;
    };
    let Some(&first) = blocks.first() else {
        return Update::DoNothing;
    };
    let undo = note
        .doc
        .blocks
        .get(first)
        .is_some_and(|b| b.kind.same_family(&kind));
    let mut changed = false;
    for b in blocks {
        let Some(current) = note.doc.blocks.get(b).map(|blk| blk.kind.clone()) else {
            continue;
        };
        let next = if undo {
            BlockKind::Paragraph
        } else if current.is_list() && kind.is_list() {
            kind.with_indent(current.indent())
        } else {
            kind.clone()
        };
        changed |= note.doc.set_kind(b, next);
    }
    if changed {
        note.doc.normalize();
        state.edited();
        Update::RefreshDom
    } else {
        Update::DoNothing
    }
}

/// Indents (`delta > 0`) or outdents the target list items.
pub fn indent(state: &mut crate::AppState, info: &mut CallbackInfo, delta: i8) -> Update {
    let blocks = command_blocks(state, info);
    let Some(note) = state.open_note_mut() else {
        return Update::DoNothing;
    };
    let mut changed = false;
    for b in blocks {
        changed |= note.doc.indent(b, delta);
    }
    if changed {
        state.edited();
        Update::RefreshDom
    } else {
        Update::DoNothing
    }
}

/// Inserts a horizontal rule after the caret's block, and an empty
/// paragraph after it for the caret.
pub fn insert_rule(state: &mut crate::AppState, info: &mut CallbackInfo) -> Update {
    let Some((_, block, _)) = target(state, info) else {
        return Update::DoNothing;
    };
    let Some(note) = state.open_note_mut() else {
        return Update::DoNothing;
    };
    let at = note.doc.insert_after(block, Block::new(BlockKind::Rule, Vec::new()));
    note.doc.insert_after(at, Block::paragraph(""));
    state.edited();
    Update::RefreshDom
}

/// Links (or unlinks, `None`) the selection.
pub fn set_link(state: &mut crate::AppState, info: &mut CallbackInfo, url: Option<String>) -> Update {
    let Some(host) = host_node(info, root_dom()) else {
        return Update::DoNothing;
    };
    let spans = selection(info, host);
    let Some(note) = state.open_note_mut() else {
        return Update::DoNothing;
    };
    let mut changed = false;
    for (b, s, e) in spans {
        changed |= note.doc.set_link(b, s, e, url.clone());
    }
    if changed {
        state.edited();
        Update::RefreshDom
    } else {
        Update::DoNothing
    }
}

/// A click on a check item's box: tick or untick it.
pub extern "C" fn on_check_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let (mut app, block) = match data.downcast_ref::<BlockRef>() {
        Some(r) => (r.app.clone(), r.block),
        None => return Update::DoNothing,
    };
    let Some(mut guard) = app.downcast_mut::<crate::AppState>() else {
        return Update::DoNothing;
    };
    let state = &mut *guard;
    // Typing the app has not folded in yet goes in first.
    let _ = sync_text(state, &mut info, false);
    let Some(note) = state.open_note_mut() else {
        return Update::DoNothing;
    };
    if note.doc.toggle_check(block) {
        state.edited();
        Update::RefreshDom
    } else {
        Update::DoNothing
    }
}

/// The keys the editor handles itself (the engine's defaults do the
/// rest): Enter in a code block (a line break), Enter on an empty list
/// item (out of the list), Backspace at the start of a list item, heading,
/// quote or code block (back to a paragraph), Tab / Shift+Tab in a list,
/// and the format and block shortcuts.
#[allow(clippy::too_many_lines)]
pub extern "C" fn on_editor_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let keyboard = info.get_current_keyboard_state();
    let Some(key) = keyboard.current_virtual_keycode.into_option() else {
        return Update::DoNothing;
    };
    let modifiers = info.get_key_modifiers();
    let primary = modifiers.primary_down();
    let shift = modifiers.shift;
    let Some(mut guard) = data.downcast_mut::<crate::AppState>() else {
        return Update::DoNothing;
    };
    let state = &mut *guard;
    let Some(host) = host_node(&info, root_dom()) else {
        return Update::DoNothing;
    };
    let caret = caret(&info, host);
    if let Some((block, _)) = caret {
        state.editor.caret_block = block;
    }
    let collapsed = selection(&info, host).is_empty();

    if primary {
        let command = match (key, shift) {
            (VirtualKeyCode::B, false) => Some(Command::Format(Format::Bold, false)),
            (VirtualKeyCode::I, false) => Some(Command::Format(Format::Italic, false)),
            (VirtualKeyCode::U, false) => Some(Command::Format(Format::Underline, false)),
            (VirtualKeyCode::X, true) => Some(Command::Format(Format::Strike, true)),
            (VirtualKeyCode::E, false) => Some(Command::Format(Format::Code, true)),
            (VirtualKeyCode::Key0, false) => Some(Command::Kind(BlockKind::Paragraph)),
            (VirtualKeyCode::Key1, false) => Some(Command::Kind(BlockKind::Heading(1))),
            (VirtualKeyCode::Key2, false) => Some(Command::Kind(BlockKind::Heading(2))),
            (VirtualKeyCode::Key3, false) => Some(Command::Kind(BlockKind::Heading(3))),
            (VirtualKeyCode::Key7, true) => Some(Command::Kind(BlockKind::Numbered(0))),
            (VirtualKeyCode::Key8, true) => Some(Command::Kind(BlockKind::Bullet(0))),
            (VirtualKeyCode::Key9, true) => Some(Command::Kind(BlockKind::Check {
                indent: 0,
                checked: false,
            })),
            (VirtualKeyCode::Return, false) => Some(Command::Check),
            _ => None,
        };
        let Some(command) = command else {
            return Update::DoNothing;
        };
        let _ = sync_text(state, &mut info, false);
        return match command {
            Command::Format(format, engine) => {
                // B / I / U at a caret: the engine's default action paints
                // the typing style, the model records it. Over a selection
                // the model formats and the engine's default is vetoed.
                if !collapsed || engine {
                    info.prevent_default();
                }
                toggle_format(state, &mut info, format, engine)
            }
            Command::Kind(kind) => {
                info.prevent_default();
                toggle_kind(state, &mut info, kind)
            }
            Command::Check => {
                info.prevent_default();
                let block = caret.map_or(state.editor.caret_block, |(b, _)| b);
                let toggled = state
                    .open_note_mut()
                    .is_some_and(|note| note.doc.toggle_check(block));
                if toggled {
                    state.edited();
                    Update::RefreshDom
                } else {
                    Update::DoNothing
                }
            }
        };
    }

    let Some((block, byte)) = caret else {
        return Update::DoNothing;
    };
    let kind = match state.open_note_mut().and_then(|n| n.doc.blocks.get(block)) {
        Some(b) => (b.kind.clone(), b.is_empty()),
        None => return Update::DoNothing,
    };
    let (kind, empty) = kind;
    match key {
        VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter if !shift && collapsed => {
            if let BlockKind::Code { .. } = kind {
                // A code block takes line breaks; Enter on its empty last
                // line (the text ends in "\n") leaves it.
                info.prevent_default();
                let _ = sync_text(state, &mut info, false);
                let text = state
                    .open_note_mut()
                    .and_then(|n| n.doc.blocks.get(block).map(Block::flat))
                    .unwrap_or_default();
                if byte >= text.len() && text.ends_with('\n') {
                    if let Some(note) = state.open_note_mut() {
                        let trimmed = text.trim_end_matches('\n').to_string();
                        note.doc.sync_block_text(block, &trimmed, None);
                        note.doc.insert_after(block, Block::paragraph(""));
                    }
                    state.edited();
                    return Update::RefreshDom;
                }
                if let Some(node) = info
                    .get_document_caret()
                    .into_option()
                    .and_then(|p| node_id(p.node))
                {
                    info.insert_text(root_dom(), node, "\n");
                }
                return Update::DoNothing;
            }
            if kind.is_list() && empty {
                // Enter on an empty item leaves the list (a level at a time).
                info.prevent_default();
                let _ = sync_text(state, &mut info, false);
                if let Some(note) = state.open_note_mut() {
                    note.doc.indent(block, -1);
                }
                state.edited();
                return Update::RefreshDom;
            }
            Update::DoNothing
        }
        VirtualKeyCode::Back if collapsed && byte == 0 && kind != BlockKind::Paragraph && kind.has_text() => {
            // Backspace at the start of a list item, heading, quote or code
            // block turns it back into a paragraph (a list item outdents
            // first) instead of merging it into the block above.
            info.prevent_default();
            let _ = sync_text(state, &mut info, false);
            if let Some(note) = state.open_note_mut() {
                if kind.is_list() {
                    note.doc.indent(block, -1);
                } else {
                    note.doc.set_kind(block, BlockKind::Paragraph);
                }
            }
            state.edited();
            Update::RefreshDom
        }
        VirtualKeyCode::Tab if kind.is_list() => {
            info.prevent_default();
            let _ = sync_text(state, &mut info, false);
            indent(state, &mut info, if shift { -1 } else { 1 })
        }
        _ => Update::DoNothing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bold() -> FormatSet {
        FormatSet {
            bold: true,
            ..FormatSet::default()
        }
    }

    #[test]
    fn the_typing_style_follows_insertions_at_its_caret_and_ends_with_anything_else() {
        let mut editor = EditorState {
            typing: Some(Typing {
                block: 2,
                at: 3,
                formats: bold(),
            }),
            caret_block: 2,
        };
        assert_eq!(editor.typing_for(2, "abc", "abcX"), Some(bold()), "typed at the caret");
        assert_eq!(editor.typing.map(|t| t.at), Some(4), "the style moves past the typed text");
        assert_eq!(editor.typing_for(2, "abcX", "abcXY"), Some(bold()));
        assert_eq!(editor.typing_for(2, "abcXY", "aXbcXY"), None, "typed elsewhere");
        assert_eq!(editor.typing, None, "and dropped");
        editor.typing = Some(Typing {
            block: 2,
            at: 3,
            formats: bold(),
        });
        assert_eq!(editor.typing_for(1, "abc", "abcX"), None, "another block");
        editor.typing = Some(Typing {
            block: 2,
            at: 3,
            formats: bold(),
        });
        assert_eq!(editor.typing_for(2, "abc", "ab"), None, "a delete");
    }
}

/// A shortcut the editor runs.
enum Command {
    /// A format; `true`: no key default of the engine paints it.
    Format(Format, bool),
    Kind(BlockKind),
    Check,
}

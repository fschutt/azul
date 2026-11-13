//! The editing suite: WPT `editing/data` cases against azul's editing engine.
//!
//! A case (tests/wpt/editing/<command>.json, vendored by
//! `scripts/refci/vendor_wpt.py`) is: the editing host's initial markup with
//! the selection marked by `[` `]` (in text) and `{` `}` (between nodes), the
//! commands `document.execCommand` would run, and the markup expected after
//! them. The runner plays the part of a user AND of an app without its own
//! document model (Path 2 of `azul_layout::document_edit`):
//!
//! 1. the markup goes into `<div contenteditable="true">` and through azul's
//!    XML loader; the markers are taken out and become the selection
//!    (`caret_at_node_byte`, set on the editing session a click opened in the
//!    block, or `set_cross_block_selection` across blocks);
//! 2. each command is what a user would do, run through the engine's own
//!    entry points: `insertparagraph` = Enter, `insertlinebreak` = Shift+Enter,
//!    `delete` = Backspace, `forwarddelete` = Delete (all decided by
//!    `LayoutWindow::keyboard_default_action`, then recorded as a structural
//!    edit or applied as a selection op, as the shells do), `inserttext` =
//!    typed text (`record_text_input` + `apply_text_changeset`);
//! 3. a recorded structural edit is applied to the case's `Dom` with
//!    `apply_document_operation` and acknowledged; text edits are read back
//!    with `styled_dom_with_edits`;
//! 4. the host's content is serialized as tags and text only (void elements as
//!    `<br/>`, `&amp;` `&lt;` `&gt;` escaped) - the form
//!    `expected_canonical` is written in - and compared with the markers
//!    stripped. Attributes are not compared (v1).
//!
//! `bold`, `italic`, `underline`, `createlink`, the list commands, `indent`
//! and `outdent` have no engine operation: azul records no inline wrap, list
//! or quote change for a selection (the app toggles its own model, AzWriter's
//! `toggle_format_range`). Those cases report UNSUPPORTED until the engine
//! gains one; wire it into `run_command`.

use std::{
    panic::{catch_unwind, AssertUnwindSafe},
    path::Path,
    time::Instant,
};

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId, NodeType},
    events::{DefaultAction, SelectionDirection, SelectionMode, SelectionOp, SelectionStep},
    geom::{LogicalPosition, LogicalSize},
    resources::RendererResources,
    selection::{CursorAffinity, MultiCursorState, SelectionRange, TextCursor},
    styled_dom::{NodeHierarchyItemId, StyledDom},
    window::{KeyboardState, VirtualKeyCode},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, managers::changeset::DocumentOperation,
    window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

use crate::{
    expect::{self, CaseResult, Expectations, Outcome},
    reftest::panic_message,
};

/// `html(0) > body(1) > div[contenteditable](2)`.
const HOST: usize = 2;
/// Void elements, serialized `<br/>` (scripts/refci/htmlnorm.py `VOID`).
const VOID: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr", "keygen",
];

struct Case {
    id: String,
    initial_xhtml: String,
    commands: Vec<(String, String)>,
    expected: Vec<String>,
}

pub fn run(root: &Path, out: &Path, filter: Option<&str>) -> bool {
    let started = Instant::now();
    let dir = root.join("editing");
    let mut files: Vec<_> = match std::fs::read_dir(&dir) {
        Ok(rd) => rd
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .collect(),
        Err(e) => {
            println!("wpt editing: cannot read {}: {e}", dir.display());
            return false;
        }
    };
    files.sort();
    let mut cases = Vec::new();
    for f in &files {
        match load(f) {
            Ok(mut c) => cases.append(&mut c),
            Err(e) => {
                println!("wpt editing: {}: {e}", f.display());
                return false;
            }
        }
    }
    let cases: Vec<Case> = cases
        .into_iter()
        .filter(|c| filter.map_or(true, |f| c.id.contains(f)))
        .collect();
    if cases.is_empty() {
        println!("wpt editing: no case selected (filter {filter:?})");
        return filter.is_some();
    }
    let exp = Expectations::load(&root.join("editing_expectations.txt"));
    let fonts = FcFontCache::build();

    let mut results = Vec::with_capacity(cases.len());
    for case in &cases {
        let command = case.commands.last().map_or("", |c| c.0.as_str());
        let gap = gap_of(command).to_string();
        let fonts = fonts.clone();
        let (outcome, detail) = match catch_unwind(AssertUnwindSafe(|| run_case(fonts, case))) {
            Ok(Ok(actual)) => {
                let want: Vec<String> = case.expected.iter().map(|e| strip_markers(e)).collect();
                if want.iter().any(|w| *w == actual) {
                    (Outcome::Pass, actual)
                } else {
                    (
                        Outcome::Fail,
                        format!(
                            "got {actual:?}, want {:?}",
                            want.first().map_or("", String::as_str)
                        ),
                    )
                }
            }
            Ok(Err((outcome, why))) => (outcome, why),
            Err(payload) => (
                Outcome::Error,
                format!("panic: {}", panic_message(&payload)),
            ),
        };
        results.push(CaseResult {
            id: case.id.clone(),
            outcome,
            gap,
            detail,
        });
    }
    expect::gate("editing", &results, &exp, out, started)
}

/// The engine gap a command's failures belong to (scripts/REFCI_2026_09_30.md).
fn gap_of(command: &str) -> &'static str {
    match command {
        "bold" | "italic" | "underline" | "createlink" => "E-EDIT-FORMAT",
        "insertunorderedlist" | "insertorderedlist" => "E-EDIT-LIST",
        "indent" | "outdent" => "E-EDIT-QUOTE",
        "insertparagraph" => "E-EDIT-SPLIT",
        "insertlinebreak" => "E-EDIT-BR",
        "delete" | "forwarddelete" => "E-EDIT-DELETE",
        "inserttext" => "E-EDIT-TYPE",
        _ => "-",
    }
}

fn load(path: &Path) -> Result<Vec<Case>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let str_of = |v: &serde_json::Value| -> String {
        v.as_str().map_or_else(|| v.to_string(), str::to_string)
    };
    let mut out = Vec::new();
    for c in v
        .get("cases")
        .and_then(serde_json::Value::as_array)
        .ok_or("no cases array")?
    {
        let commands = c
            .get("commands")
            .and_then(serde_json::Value::as_array)
            .ok_or("case without commands")?
            .iter()
            .filter_map(|pair| {
                let pair = pair.as_array()?;
                Some((
                    str_of(pair.first()?),
                    pair.get(1).map(&str_of).unwrap_or_default(),
                ))
            })
            .collect();
        out.push(Case {
            id: c.get("id").map(&str_of).ok_or("case without id")?,
            initial_xhtml: c
                .get("initial_xhtml")
                .map(&str_of)
                .ok_or("case without initial_xhtml")?,
            commands,
            expected: c
                .get("expected_canonical")
                .and_then(serde_json::Value::as_array)
                .ok_or("case without expected_canonical")?
                .iter()
                .map(&str_of)
                .collect(),
        });
    }
    Ok(out)
}

fn strip_markers(s: &str) -> String {
    s.replace(['[', ']', '{', '}'], "")
}

type CaseError = (Outcome, String);

fn harness(why: impl Into<String>) -> CaseError {
    (Outcome::Error, format!("harness: {}", why.into()))
}

/// Run one case; `Ok` is the host's content afterwards.
fn run_case(fonts: FcFontCache, case: &Case) -> Result<String, CaseError> {
    // A command with no engine operation fails before any setup.
    for (name, _) in &case.commands {
        if !matches!(
            name.as_str(),
            "stylewithcss"
                | "defaultparagraphseparator"
                | "inserttext"
                | "insertparagraph"
                | "insertlinebreak"
                | "delete"
                | "forwarddelete"
        ) {
            return Err((
                Outcome::Unsupported,
                format!(
                    "no engine operation for {name}: azul records no inline wrap, list or quote \
                     edit for a selection"
                ),
            ));
        }
    }

    let xml = format!(
        "<html><body><div contenteditable=\"true\">{}</div></body></html>",
        case.initial_xhtml
    );
    let parsed =
        azul_layout::xml::parse_xml(&xml).map_err(|e| (Outcome::Error, format!("parse: {e}")))?;
    let mut model = azul_layout::xml::dom_from_parsed_xml(parsed);
    let markers = take_markers(&mut model);
    let _ = model.fixup_children_estimated();
    let (start, end) = selection_of(&model, &markers).map_err(|e| harness(e))?;

    let styled = StyledDom::create_from_dom(model.clone());
    let mut lw = LayoutWindow::new(fonts).map_err(|e| harness(format!("window: {e:?}")))?;
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = ws.clone();
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    let mut dbg = None;
    lw.layout_and_generate_display_list(styled, &ws, &rr, &sc, &mut dbg)
        .map_err(|e| (Outcome::Error, format!("layout: {e:?}")))?;

    let host = dnid(HOST);
    place_selection(&mut lw, host, &model, start, end)?;

    for (name, value) in &case.commands {
        run_command(&mut lw, host, name, value)?;
    }

    let result = match lw.pending_document_edit.clone() {
        Some(changeset) => {
            let parent = edited_parent(&lw, &changeset.operation)
                .ok_or_else(|| harness("the structural edit names no parent"))?;
            let path = lw
                .node_child_index_path(dnid(0), parent)
                .ok_or_else(|| harness("no path to the edited node"))?;
            let applied =
                azul_layout::document_edit::apply_document_operation(&mut model, &path, &changeset)
                    .map_err(|e| {
                        (
                            Outcome::Fail,
                            format!("the recorded edit does not apply: {e:?}"),
                        )
                    })?;
            let _ = lw.mark_document_edit_applied_with_inverse(changeset.id, applied.inverse);
            model
        }
        None => lw
            .styled_dom_with_edits(DomId::ROOT_ID)
            .ok_or_else(|| harness("no laid-out DOM"))?
            .reconstruct_dom_subtree(None),
    };
    let host_dom = result
        .children
        .as_ref()
        .first()
        .and_then(|body| body.children.as_ref().first())
        .ok_or_else(|| harness("the result has no editing host"))?;
    let mut out = String::new();
    for child in host_dom.children.as_ref() {
        serialize(child, &mut out);
    }
    Ok(out)
}

/// What a user does for `name` (see the module doc).
fn run_command(
    lw: &mut LayoutWindow,
    host: DomNodeId,
    name: &str,
    value: &str,
) -> Result<(), CaseError> {
    match name {
        // execCommand state with no engine counterpart: blocks keep the shape
        // they are split from, and formatting is never CSS-in-style.
        "stylewithcss" | "defaultparagraphseparator" => Ok(()),
        "inserttext" => {
            if lw.text_edit_manager.get_cross_block_selection().is_some() {
                lw.replace_cross_block_selection(value)
                    .map(|_| ())
                    .ok_or_else(|| {
                        (
                            Outcome::Fail,
                            "typing over a cross-block selection recorded nothing".into(),
                        )
                    })
            } else if value.is_empty() {
                let _ = lw.apply_selection_op(
                    host,
                    &SelectionOp::new(
                        SelectionDirection::Backward,
                        SelectionStep::Character,
                        SelectionMode::Delete,
                    ),
                );
                Ok(())
            } else {
                if lw.record_text_input(value).is_empty() {
                    return Err((Outcome::Fail, "the typed text reached no editable".into()));
                }
                let _ = lw.apply_text_changeset();
                Ok(())
            }
        }
        "insertparagraph" => key(lw, host, VirtualKeyCode::Return, false),
        "insertlinebreak" => key(lw, host, VirtualKeyCode::Return, true),
        "delete" => key(lw, host, VirtualKeyCode::Back, false),
        "forwarddelete" => key(lw, host, VirtualKeyCode::Delete, false),
        other => Err((
            Outcome::Unsupported,
            format!("no engine operation for {other}"),
        )),
    }
}

/// A key press, as the shells handle it after dispatch: the default action
/// `keyboard_default_action` decides, then a structural edit is recorded, a
/// soft break is typed, or Backspace / Delete run as a selection op.
fn key(
    lw: &mut LayoutWindow,
    host: DomNodeId,
    vk: VirtualKeyCode,
    shift: bool,
) -> Result<(), CaseError> {
    let mut ks = KeyboardState::default();
    ks.current_virtual_keycode = Some(vk).into();
    let mut pressed = vec![vk];
    if shift {
        pressed.push(VirtualKeyCode::LShift);
    }
    ks.pressed_virtual_keycodes = pressed.into();
    let editing = lw.build_editing_query_state(Some(host));
    let action = lw.keyboard_default_action(&ks, Some(host), false, editing.as_ref());
    match action.action {
        DefaultAction::SplitBlockAtCursor { .. }
        | DefaultAction::MergeWithPrevious { .. }
        | DefaultAction::MergeWithNext { .. } => {
            if lw
                .record_structural_default_action(&action.action)
                .is_none()
            {
                return Err((Outcome::Fail, "the structural edit was not recorded".into()));
            }
            Ok(())
        }
        DefaultAction::InsertLineBreakAtCursor { .. } => {
            if lw.record_text_input("\n").is_empty() {
                return Err((Outcome::Fail, "the line break reached no editable".into()));
            }
            let _ = lw.apply_text_changeset();
            Ok(())
        }
        DefaultAction::None if matches!(vk, VirtualKeyCode::Back | VirtualKeyCode::Delete) => {
            let direction = if matches!(vk, VirtualKeyCode::Back) {
                SelectionDirection::Backward
            } else {
                SelectionDirection::Forward
            };
            // "Nothing to delete" (Backspace at the very start) is a result too.
            let _ = lw.apply_selection_op(
                host,
                &SelectionOp::new(direction, SelectionStep::Character, SelectionMode::Delete),
            );
            Ok(())
        }
        _ => Err((
            Outcome::Fail,
            format!("{vk:?} in the editing host has no editing default action"),
        )),
    }
}

/// The node whose child list a structural edit changes (the `host_path`
/// target of `apply_document_operation`).
fn edited_parent(lw: &LayoutWindow, op: &DocumentOperation) -> Option<DomNodeId> {
    match op {
        DocumentOperation::SplitNode(s) => parent_of(lw, s.node),
        DocumentOperation::MergeNodes(m) => parent_of(lw, m.first),
        DocumentOperation::InsertChildren(i) => Some(i.parent),
        DocumentOperation::RemoveChildren(r) => Some(r.parent),
        DocumentOperation::ReplaceChildren(r) => Some(r.parent),
        DocumentOperation::WrapRange(w) => Some(w.node),
        DocumentOperation::UnwrapRange(u) => Some(u.node),
    }
}

fn parent_of(lw: &LayoutWindow, node: DomNodeId) -> Option<DomNodeId> {
    let lr = lw.get_layout_result(&node.dom)?;
    let id = node.node.into_crate_internal()?;
    let parent = lr
        .styled_dom
        .node_hierarchy
        .as_container()
        .get(id)?
        .parent_id()?;
    Some(DomNodeId {
        dom: node.dom,
        node: NodeHierarchyItemId::from_crate_internal(Some(parent)),
    })
}

fn dnid(n: usize) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(n))),
    }
}

// ---------------------------------------------------------------------------
// Selection markers
// ---------------------------------------------------------------------------

/// Where a marker was, in the cleaned tree: inside a text node (child-index
/// path from the root, byte offset), or between the children of an element
/// (the marker was a text node of its own: `<b>x</b>{}<b>y</b>`).
#[derive(Clone)]
enum Anchor {
    Text(Vec<usize>, usize),
    Between(Vec<usize>, usize),
}

/// Take `[` `]` `{` `}` out of every text node (dropping text nodes that
/// held only markers). Returns `(is_start, anchor)` in document order.
fn take_markers(root: &mut Dom) -> Vec<(bool, Anchor)> {
    let mut found = Vec::new();
    let mut path = Vec::new();
    strip(root, &mut path, &mut found);
    found
}

fn strip(node: &mut Dom, path: &mut Vec<usize>, found: &mut Vec<(bool, Anchor)>) {
    let children = std::mem::take(&mut node.children).into_library_owned_vec();
    let mut kept: Vec<Dom> = Vec::with_capacity(children.len());
    for mut child in children {
        let text = match child.root.get_node_type() {
            NodeType::Text(t) => Some(t.as_str().to_string()),
            _ => None,
        };
        match text {
            Some(text) if text.contains(['[', ']', '{', '}']) => {
                let mut clean = String::with_capacity(text.len());
                let mut marks = Vec::new();
                for c in text.chars() {
                    match c {
                        '[' | '{' => marks.push((true, clean.len())),
                        ']' | '}' => marks.push((false, clean.len())),
                        _ => clean.push(c),
                    }
                }
                let index = kept.len();
                if clean.is_empty() {
                    for (is_start, _) in marks {
                        found.push((is_start, Anchor::Between(path.clone(), index)));
                    }
                    continue;
                }
                let mut at = path.clone();
                at.push(index);
                for (is_start, byte) in marks {
                    found.push((is_start, Anchor::Text(at.clone(), byte)));
                }
                kept.push(Dom::create_text_do_not_use_without_block_level_wrapper(
                    clean,
                ));
            }
            Some(_) => kept.push(child),
            None => {
                path.push(kept.len());
                strip(&mut child, path, found);
                path.pop();
                kept.push(child);
            }
        }
    }
    node.children = kept.into();
}

/// A text node in document order: pre-order node id, byte length, text.
struct TextAt {
    id: usize,
    path: Vec<usize>,
    text: String,
}

fn subtree_len(d: &Dom) -> usize {
    1 + d.children.as_ref().iter().map(subtree_len).sum::<usize>()
}

/// Pre-order id of the node at `path` (the `StyledDom` arena order).
fn id_at(root: &Dom, path: &[usize]) -> Option<usize> {
    let mut id = 0;
    let mut node = root;
    for &i in path {
        let kids = node.children.as_ref();
        id += 1 + kids.get(..i)?.iter().map(subtree_len).sum::<usize>();
        node = kids.get(i)?;
    }
    Some(id)
}

fn texts(root: &Dom) -> Vec<TextAt> {
    fn walk(d: &Dom, path: &mut Vec<usize>, next_id: &mut usize, out: &mut Vec<TextAt>) {
        let id = *next_id;
        *next_id += 1;
        if let NodeType::Text(t) = d.root.get_node_type() {
            out.push(TextAt {
                id,
                path: path.clone(),
                text: t.as_str().to_string(),
            });
        }
        for (i, c) in d.children.as_ref().iter().enumerate() {
            path.push(i);
            walk(c, path, next_id, out);
            path.pop();
        }
    }
    let mut out = Vec::new();
    walk(root, &mut Vec::new(), &mut 0, &mut out);
    out
}

/// A selection end: text node id, byte, and the node's text.
type End = (usize, usize, String);

/// `(start, end)` from the markers; without markers the caret is at the end.
fn selection_of(model: &Dom, markers: &[(bool, Anchor)]) -> Result<(End, End), String> {
    let all = texts(model);
    let resolve = |a: &Anchor| -> Option<End> {
        match a {
            Anchor::Text(path, byte) => {
                let t = all.iter().find(|t| &t.path == path)?;
                Some((t.id, *byte, t.text.clone()))
            }
            Anchor::Between(parent, index) => {
                // The position's place in document order: before the child
                // now at `index`, else after the parent's whole subtree.
                let parent_id = id_at(model, parent)?;
                let mut node = model;
                for &i in parent {
                    node = node.children.as_ref().get(i)?;
                }
                let pos = if *index < node.children.as_ref().len() {
                    let mut p = parent.clone();
                    p.push(*index);
                    id_at(model, &p)?
                } else {
                    parent_id + subtree_len(node)
                };
                // The end of the text before it, else the start of the text after.
                all.iter()
                    .filter(|t| t.id < pos)
                    .last()
                    .map(|t| (t.id, t.text.len(), t.text.clone()))
                    .or_else(|| {
                        all.iter()
                            .find(|t| t.id >= pos)
                            .map(|t| (t.id, 0, t.text.clone()))
                    })
            }
        }
    };
    let start = markers.iter().find(|m| m.0).map(|m| &m.1);
    let end = markers.iter().find(|m| !m.0).map(|m| &m.1);
    match (start, end) {
        (Some(s), Some(e)) => Ok((
            resolve(s).ok_or("a selection start with no text to put it in")?,
            resolve(e).ok_or("a selection end with no text to put it in")?,
        )),
        (Some(s), None) | (None, Some(s)) => {
            let at = resolve(s).ok_or("a caret with no text to put it in")?;
            Ok((at.clone(), at))
        }
        (None, None) => {
            let last = all.last().ok_or("no text in the editing host")?;
            let at = (last.id, last.text.len(), last.text.clone());
            Ok((at.clone(), at))
        }
    }
}

/// A caret at `byte` of text node `node`: Leading on the character there, or
/// Trailing on the last one at the end of the text (the engine's own end caret).
fn caret(
    lw: &LayoutWindow,
    at: &End,
) -> Result<(azul_core::selection::TextBlock, TextCursor), CaseError> {
    let (node, byte, text) = at;
    let block = lw
        .text_block_of(dnid(*node))
        .ok_or_else(|| harness("the selection's text is in no text block"))?;
    let (byte, trailing) = if *byte >= text.len() && !text.is_empty() {
        (text.char_indices().last().map_or(0, |(i, _)| i), true)
    } else {
        (*byte, false)
    };
    let mut cursor = lw
        .caret_at_node_byte(
            block,
            NodeId::new(*node),
            u32::try_from(byte).unwrap_or(u32::MAX),
        )
        .ok_or_else(|| harness("no caret position in the selection's text"))?;
    if trailing {
        cursor.affinity = CursorAffinity::Trailing;
    }
    Ok((block, cursor))
}

/// Focus the host and put the selection where the markers were: a click in
/// the start's block opens the editing session, then the session's selection
/// is set exactly (a click cannot aim at a byte).
fn place_selection(
    lw: &mut LayoutWindow,
    host: DomNodeId,
    _model: &Dom,
    start: End,
    end: End,
) -> Result<(), CaseError> {
    let (block_s, cur_s) = caret(lw, &start)?;
    let (block_e, cur_e) = caret(lw, &end)?;

    let element = block_s.element().unwrap_or_else(|| block_s.container());
    if let Some(rect) = lw.get_node_layout_rect(dnid(element.index())) {
        let point = LogicalPosition::new(
            rect.origin.x + 1.0,
            rect.origin.y + (rect.size.height * 0.5).min(8.0),
        );
        let _ = lw.process_mouse_click_for_selection(point, 0);
    }
    lw.focus_manager.set_focused_node(Some(host));

    if block_s != block_e {
        if !lw.set_cross_block_selection(block_s, cur_s, block_e, cur_e) {
            return Err(harness("set_cross_block_selection refused the selection"));
        }
        return Ok(());
    }
    let collapsed = start.0 == end.0 && start.1 == end.1;
    let existing = lw
        .text_edit_manager
        .multi_cursor
        .as_ref()
        .map(|mc| (mc.block, mc.contenteditable_key));
    match existing {
        Some((b, _)) if b == block_s => {
            if let Some(mc) = lw.text_edit_manager.multi_cursor.as_mut() {
                if collapsed {
                    mc.set_single_cursor(cur_s);
                } else {
                    mc.set_single_range(SelectionRange {
                        start: cur_s,
                        end: cur_e,
                    });
                }
            }
        }
        other => {
            let key = other.map_or(0, |(_, k)| k);
            let mut mc = MultiCursorState::new_with_cursor(cur_s, block_s, key);
            if !collapsed {
                mc.set_single_range(SelectionRange {
                    start: cur_s,
                    end: cur_e,
                });
            }
            lw.text_edit_manager.multi_cursor = Some(mc);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Serialization
// ---------------------------------------------------------------------------

/// Tags and text only - `scripts/refci/htmlnorm.py` `serialize(with_attrs=False)`.
fn serialize(d: &Dom, out: &mut String) {
    if let NodeType::Text(t) = d.root.get_node_type() {
        for c in t.as_str().chars() {
            match c {
                '&' => out.push_str("&amp;"),
                '<' => out.push_str("&lt;"),
                '>' => out.push_str("&gt;"),
                c => out.push(c),
            }
        }
        return;
    }
    let tag = d.root.get_node_type().get_path().to_string();
    if VOID.contains(&tag.as_str()) {
        out.push('<');
        out.push_str(&tag);
        out.push_str("/>");
        return;
    }
    out.push('<');
    out.push_str(&tag);
    out.push('>');
    for c in d.children.as_ref() {
        serialize(c, out);
    }
    out.push_str("</");
    out.push_str(&tag);
    out.push('>');
}

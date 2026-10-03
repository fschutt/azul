//! The note editor: azul's shared rich-text editor (`RichTextEditor`, the
//! one AzMail's compose window and AzWriter use too) over the open note's
//! document.
//!
//! The editor folds the engine's edits into its state - typing (the runs
//! keep their formats), Enter / Backspace across blocks, pastes, Markdown
//! shortcuts, formats, the check boxes - and hands every new state to
//! [`on_editor_change`]: the app keeps it (`AppState::editor`) and copies
//! the document into the open note (autosave, search, the list's preview).
//! The formatting toolbar (`ui.rs`) runs the editor's commands on that
//! state ([`run`]); Undo / Redo are the editor's ONE history.

use azul::{
    callbacks::{
        CallbackInfo, RefAny, RichTextEditorOnChangeCallbackType,
        RichTextEditorOnLinkCallbackType, Update,
    },
    dom::{Dom, DomId, DomNodeId},
    str::String as AzString,
    svg::{CssPath, CssPathSelector},
    vec::RichTextSpanVec,
    widgets::{RichTextCommand, RichTextDoc, RichTextEditor, RichTextEditorState, RichTextSpan},
};

use crate::{ids, model::Note, AppState};

/// The editor state for `doc` (a note just opened): the host's id set, an
/// empty history.
#[must_use]
pub fn state_for(doc: &RichTextDoc) -> RichTextEditorState {
    let mut state = RichTextEditorState::create(doc.clone());
    state.host_id = ids::NOTE_BODY;
    state
}

/// `editor` with the pictures of `note`'s image blocks the app has loaded.
fn with_images(mut editor: RichTextEditor, s: &AppState, note: &Note, doc: &RichTextDoc) -> RichTextEditor {
    for src in doc.image_srcs().iter() {
        let image = crate::store::image_key(&note.notebook, src.as_str()).and_then(|key| s.images.get(&key));
        if let Some(image) = image {
            editor = editor.with_image(src.clone(), image.clone());
        }
    }
    editor
}

/// The editing host of the open note (the note pane draws its own paper
/// around it).
#[must_use]
pub fn editor_dom(s: &AppState, app: &RefAny, note: &Note) -> Dom {
    let editor = RichTextEditor::create(s.editor.clone())
        .with_id(ids::NOTE_BODY)
        .with_accessibility_name("Note text")
        .with_font_size(s.settings.text_size.px())
        .with_on_change(app.clone(), on_editor_change as RichTextEditorOnChangeCallbackType)
        .with_on_link(app.clone(), on_editor_link as RichTextEditorOnLinkCallbackType);
    with_images(editor, s, note, &note.doc).content_dom()
}

/// `doc` read-only, with `title` over it (a version in the history, the PDF
/// export).
#[must_use]
pub fn print_dom(s: &AppState, note: &Note, doc: &RichTextDoc, title: &str, font_px: f32) -> Dom {
    let mut view = state_for(doc);
    view.host_id = ids::NOTE_PRINT;
    let editor = RichTextEditor::create(view)
        .with_read_only(true)
        .with_accessibility_name(title)
        .with_font_size(font_px);
    Dom::create_div()
        .with_css(format!("display: block; font-size: {font_px}px; font-family: sans-serif;"))
        .with_child(
            Dom::create_h1_with_text(title)
                .with_css(format!("margin: 0px; margin-bottom: 14px; font-size: {}px;", (font_px * 2.0).round())),
        )
        .with_child(with_images(editor, s, note, doc).content_dom())
}

/// `on_change`: the editor's new state is kept; its document is the open
/// note's (edited now: dated, searchable, due a save).
extern "C" fn on_editor_change(mut data: RefAny, _info: CallbackInfo, state: RichTextEditorState) -> Update {
    let Some(mut guard) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    let changed = s.open_note().is_some_and(|note| note.doc != state.doc);
    s.editor = state;
    if changed {
        adopt_doc(s);
    }
    Update::DoNothing
}

/// Ctrl/Cmd + click on a link in the text: open it.
extern "C" fn on_editor_link(_data: RefAny, _info: CallbackInfo, url: AzString) -> Update {
    if open_url(url.as_str()) {
        println!("AZNOTES_OPENED_LINK {}", url.as_str());
    }
    Update::DoNothing
}

/// The editor's document becomes the open note's.
fn adopt_doc(s: &mut AppState) {
    let doc = s.editor.doc.clone();
    if let Some(note) = s.open_note_mut() {
        note.doc = doc;
    }
    s.edited();
}

/// Folds what was typed and not reported yet into the editor and the open
/// note (before the app reads or changes the note). Returns whether the
/// note changed.
pub fn sync(s: &mut AppState, info: &mut CallbackInfo) -> bool {
    if !s.editor.sync(*info) {
        return false;
    }
    adopt_doc(s);
    true
}

/// Runs an editor command (the toolbar, a shortcut of the window) on the
/// open note.
pub fn run(s: &mut AppState, info: &mut CallbackInfo, command: RichTextCommand) -> Update {
    let before = s.editor.revision;
    let update = s.editor.apply_command(*info, command);
    if s.editor.revision != before {
        adopt_doc(s);
    }
    update
}

/// The editor's selection now, as `(block, start, end)` (the link sheet
/// takes the focus, so it keeps them).
#[must_use]
pub fn selection(s: &AppState, info: &CallbackInfo) -> Vec<(usize, usize, usize)> {
    s.editor
        .get_selection(*info)
        .iter()
        .map(|span| (span.block, span.start, span.end))
        .collect()
}

/// Links `spans` to `url` (`None`: unlinks them); with nothing selected the
/// address goes in at the caret as its own linked text.
pub fn link(s: &mut AppState, info: &mut CallbackInfo, spans: &[(usize, usize, usize)], url: Option<String>) -> Update {
    let spans: Vec<RichTextSpan> = spans
        .iter()
        .map(|&(block, start, end)| RichTextSpan { block, start, end })
        .collect();
    let before = s.editor.revision;
    let update = s
        .editor
        .set_link_on(*info, RichTextSpanVec::from_vec(spans), url.unwrap_or_default());
    if s.editor.revision != before {
        adopt_doc(s);
    }
    update
}

/// Opens `url` with the system's handler (the browser, for a web address).
pub fn open_url(url: &str) -> bool {
    match azul::url::Url::parse(url).into_result() {
        Ok(url) => url.open(),
        Err(_) => false,
    }
}

/// Puts the keyboard focus back into the editor (after a toolbar button
/// took it).
pub fn focus_editor(info: &mut CallbackInfo) {
    info.set_focus_to_path(
        root_dom(),
        CssPath {
            selectors: vec![CssPathSelector::Id(ids::NOTE_BODY)].into(),
        },
    );
}

/// The editing host of the window, if a note is open.
#[must_use]
pub fn host_node(info: &CallbackInfo, dom: DomId) -> Option<DomNodeId> {
    let node = info.get_node_id_by_id_attribute(dom, ids::NOTE_BODY);
    (node.into_raw() != 0).then_some(DomNodeId { dom, node })
}

/// The DOM of the root window (the host lives there).
#[must_use]
pub const fn root_dom() -> DomId {
    DomId { inner: 0 }
}

//! The FLOW editor the writing apps share (feature `azul`): AzWriter's text and AzNotes' page.
//!
//! A writing app has no Preview and no list inside its main view: the text flows like a mobile
//! Markdown editor, and the sidebar's collapsed rail carries the formatting a person needs while
//! writing (bold, the size, undo ...) at the lower-left corner, where the thumb of a
//! right-handed user holding the phone upright reaches. This module is the one place that
//! builds both, so the two apps do not:
//!
//! - [`canvas`]: the scroller the page flows in. The app builds the editing host itself (its id,
//!   its change callback, its links and images: `RichTextEditor::page_doms` or `dom`) and hands
//!   it in; the canvas gives it the column ([`FlowOptions`]: a measure, margins, the room the
//!   keyboard leaves), the blank page's placeholder and the keyboard on mount. A pan scrolls the
//!   page, a tap places the caret and raises the soft keyboard (the engine's contenteditable
//!   focus does that, as the code view's tap does); this module adds nothing to the gesture.
//! - [`quick_access`]: the rail's formatting buttons ([`Quick`]) with the caret's state (a bold
//!   run lights Bold, a list item lights Bullets), the swatch-less toggles of a writing app, in
//!   the order the app names (the LAST is the nearest to the thumb; [`WRITER_QUICK`] is the
//!   default list). Their tags are below [`TAG_LIMIT`]: an app appends buttons of its own from
//!   there on.
//! - [`quick_commands`] and [`run`]: what a tap on such a button does to the document, as the
//!   editor's own commands, applied to the state the app keeps, the focus handed back to the text.
//!
//! The app owns every state: the editor state, which quick buttons it shows, where its
//! document is. Nothing here keeps one.

use azul::{
    callbacks::CallbackInfo,
    dom::{AttributeType, Dom},
    widgets::{
        RichBlockKind, RichCheck, RichFormat, RichTextCommand, RichTextEditorState,
        SidebarQuickAction, SidebarQuickState,
    },
};

use crate::l10n::label;
pub use crate::flow_rules::{
    heading_target, step_size, FlowOptions, FlowPaper, Quick, BODY_HALF_POINTS, DEFAULT_MEASURE,
    QUICK_ALL, SIZES_HALF_POINTS, TAG_LIMIT, WRITER_QUICK,
};

/// The blank page's placeholder: a caret where the text begins and the app's words after it.
fn placeholder(o: &FlowOptions) -> Dom {
    Dom::create_div()
        .with_css(o.placeholder_css())
        .with_child(
            Dom::create_div().with_css("width: 2px; height: 20px; margin-right: 3px; background: system:accent;"),
        )
        .with_child(Dom::create_span_with_text(o.placeholder.clone()))
}

/// The page the text flows on: `page` (the editing host the app built) inside the page's box
/// inside the scroller. A blank document shows the placeholder UNDER the page (the editor is
/// transparent and takes the touch: the keyboard comes with the first tap, not before), and the
/// text takes the keyboard on mount only where [`FlowOptions::autofocus`] says.
#[must_use]
pub fn canvas(page: Dom, o: &FlowOptions) -> Dom {
    canvas_pages(vec![page], o)
}

/// [`canvas`] for a document of several pages (the print layout's sheets): one box per page in
/// the one scroller. The first page takes the keyboard when `autofocus` says so, and carries the
/// placeholder of a blank document.
#[must_use]
pub fn canvas_pages(pages: Vec<Dom>, o: &FlowOptions) -> Dom {
    let mut scroller = Dom::create_div().with_css(o.scroller_css());
    for (index, page) in pages.into_iter().enumerate() {
        let page = if o.autofocus && index == 0 {
            page.with_attribute(AttributeType::Autofocus)
        } else {
            page
        };
        let mut paper = Dom::create_div().with_css(o.paper_css());
        if !o.paper_class.is_empty() {
            paper = paper.with_class(o.paper_class.clone());
        }
        if index == 0 && o.blank && !o.placeholder.is_empty() {
            paper.add_child(placeholder(o));
        }
        paper.add_child(page);
        scroller.add_child(paper);
    }
    if o.dom_id.is_empty() {
        scroller
    } else {
        scroller.with_id(o.dom_id.clone())
    }
}

fn on_off(on: bool) -> SidebarQuickState {
    if on {
        SidebarQuickState::On
    } else {
        SidebarQuickState::Off
    }
}

fn plain_if(enabled: bool) -> SidebarQuickState {
    if enabled {
        SidebarQuickState::Plain
    } else {
        SidebarQuickState::Disabled
    }
}

/// The state of a button for the caret in `editor`: a toggle shows what the text under the caret
/// is, undo and redo wait for something to undo, and with no document open everything is
/// disabled.
#[must_use]
pub fn quick_state(quick: Quick, editor: Option<&RichTextEditorState>) -> SidebarQuickState {
    let Some(editor) = editor else {
        return SidebarQuickState::Disabled;
    };
    match quick {
        Quick::Undo => plain_if(editor.can_undo()),
        Quick::Redo => plain_if(editor.can_redo()),
        Quick::Bold => on_off(editor.is_current_format(RichFormat::Bold)),
        Quick::Italic => on_off(editor.is_current_format(RichFormat::Italic)),
        Quick::Underline => on_off(editor.is_current_format(RichFormat::Underline)),
        Quick::Strike => on_off(editor.is_current_format(RichFormat::Strike)),
        Quick::Code => on_off(editor.is_current_format(RichFormat::Code)),
        Quick::Bullets => on_off(editor.is_current_kind(RichBlockKind::Bullet(0))),
        Quick::Numbers => on_off(editor.is_current_kind(RichBlockKind::Numbered(0))),
        Quick::Checklist => on_off(editor.is_current_kind(RichBlockKind::Check(RichCheck {
            indent: 0,
            checked: false,
        }))),
        Quick::Heading => on_off(matches!(editor.current_kind(), RichBlockKind::Heading(_))),
        Quick::Quote => on_off(editor.is_current_quoted()),
        Quick::SizeDown | Quick::SizeUp | Quick::Outdent | Quick::Indent => SidebarQuickState::Plain,
    }
}

/// The rail's quick-access buttons for `set`, in that order (the app puts the most used LAST),
/// each with the state the caret in `editor` gives it. `None`: no document is open.
#[must_use]
pub fn quick_access(editor: Option<&RichTextEditorState>, set: &[Quick]) -> Vec<SidebarQuickAction> {
    set.iter()
        .map(|quick| {
            SidebarQuickAction::create(label(quick.key()), quick.icon())
                .with_tag(quick.tag())
                .with_state(quick_state(*quick, editor))
        })
        .collect()
}

/// The editor commands a tap on `quick` runs on the document in `editor`. The size buttons step
/// through Word's sizes from the size under the caret; Heading cycles paragraph, 1, 2, 3 and back.
#[must_use]
pub fn quick_commands(quick: Quick, editor: &RichTextEditorState) -> Vec<RichTextCommand> {
    let toggle = |format| vec![RichTextCommand::ToggleFormat(format)];
    match quick {
        Quick::Undo => vec![RichTextCommand::Undo],
        Quick::Redo => vec![RichTextCommand::Redo],
        Quick::Bold => toggle(RichFormat::Bold),
        Quick::Italic => toggle(RichFormat::Italic),
        Quick::Underline => toggle(RichFormat::Underline),
        Quick::Strike => toggle(RichFormat::Strike),
        Quick::Code => toggle(RichFormat::Code),
        Quick::SizeDown => vec![RichTextCommand::SetFontSize(step_size(editor.current_font().half_points, false))],
        Quick::SizeUp => vec![RichTextCommand::SetFontSize(step_size(editor.current_font().half_points, true))],
        Quick::Bullets => vec![RichTextCommand::ToggleKind(RichBlockKind::Bullet(0))],
        Quick::Numbers => vec![RichTextCommand::ToggleKind(RichBlockKind::Numbered(0))],
        Quick::Checklist => vec![RichTextCommand::ToggleKind(RichBlockKind::Check(RichCheck {
            indent: 0,
            checked: false,
        }))],
        Quick::Heading => {
            let level = match editor.current_kind() {
                RichBlockKind::Heading(level) => Some(level),
                _ => None,
            };
            vec![RichTextCommand::ToggleKind(RichBlockKind::Heading(heading_target(level)))]
        }
        Quick::Quote => vec![RichTextCommand::ToggleQuote],
        Quick::Outdent => vec![RichTextCommand::Outdent],
        Quick::Indent => vec![RichTextCommand::Indent],
    }
}

/// Applies `commands` to the document in `editor` and gives the focus back to the text (a button
/// of the rail or the ribbon took it). `true` when the document changed: the app then asks for a
/// new page. The caller must have folded what was typed and not reported yet into `editor`
/// first (`RichTextEditorState::sync`).
pub fn run(editor: &mut RichTextEditorState, info: &mut CallbackInfo, commands: Vec<RichTextCommand>) -> bool {
    let before = editor.revision;
    for command in commands {
        let _ = editor.apply_command(*info, command);
    }
    editor.focus(*info);
    editor.revision != before
}

/// The caret's place, `(block, byte)`, as the editor last saw it.
#[must_use]
pub fn caret(editor: &RichTextEditorState) -> (usize, usize) {
    (editor.caret_block, editor.caret_byte)
}

//! The shared rich-text editor (`widgets::rich_text_editor`), driven the way
//! the engine drives it: laid out in a window, typed into, its callbacks
//! fired with a `CallbackInfo` over that window. The editor's model is what
//! the app gets in `on_change`.
//!
//! The bugs the three editors it replaces had (scripts/DEDUP_EDITORS A3):
//! - AzMail flattened a paragraph to one text node on the first keystroke,
//!   so its bold and its link were gone on the next rebuild or Send (A3.1);
//! - Ctrl/Cmd+B over a selection did nothing in AzMail and AzWriter (A3.3);
//! - AzMail's Bold pressed twice wrapped another `<b>` instead of toggling
//!   the format off (A3.4);
//! - AzWriter kept two undo histories, its own and the engine's (A3.6).

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use azul_core::{
    callbacks::Update,
    dom::{Dom, DomId, DomNodeId, EventFilter, NodeId, NodeType},
    events::{DefaultAction, FocusEventFilter},
    geom::OptionLogicalPosition,
    gl::OptionGlContextPtr,
    hit_test::ScrollPosition,
    refany::{OptionRefAny, RefAny},
    resources::RendererResources,
    selection::SelectionRange,
    styled_dom::{NodeHierarchyItemId, StyledDom},
    window::{KeyboardState, MonitorVec, RawWindowHandle, VirtualKeyCode},
};
use azul_css::{system::SystemStyle, AzString};
use azul_layout::{
    callbacks::{
        Callback, CallbackChange, CallbackInfo, CallbackInfoRefData, ExternalSystemCallbacks,
    },
    widgets::{
        rich_text::doc::{
            RichBlock, RichBlockKind, RichFormat, RichRun, RichTextDoc,
        },
        rich_text_editor::{
            RichTextCommand, RichTextEditor, RichTextEditorOnChangeCallbackType,
            RichTextEditorState, DEFAULT_HOST_ID,
        },
    },
    window::LayoutWindow,
    window_state::FullWindowState,
};

use crate::editing_harness::{dnid, lay_out};

// ---- fixtures ----

/// Every state the editor reported through `on_change`, in order.
type Log = Arc<Mutex<Vec<RichTextEditorState>>>;

extern "C" fn record(mut data: RefAny, _info: CallbackInfo, state: RichTextEditorState) -> Update {
    if let Some(log) = data.downcast_ref::<Log>() {
        log.lock().expect("log").push(state);
    }
    Update::DoNothing
}

fn plain(text: &str) -> RichRun {
    RichRun::plain(text)
}

fn bold(text: &str) -> RichRun {
    RichRun::plain(text).with_format(RichFormat::Bold)
}

fn linked(text: &str) -> RichRun {
    RichRun::plain(text).with_link(AzString::from("https://example.org/plan"))
}

/// `<p>ab<b>c</b><a>d</a></p>`, then `<p>second</p>`.
fn mail_doc() -> RichTextDoc {
    RichTextDoc::from_blocks(vec![
        RichBlock::new(
            RichBlockKind::Paragraph,
            vec![plain("ab"), bold("c"), linked("d")],
        ),
        RichBlock::paragraph("second"),
    ])
}

/// The editor for `doc` laid out in a window, its host focused; the log
/// its `on_change` writes into.
fn editor(doc: RichTextDoc) -> (LayoutWindow, Log) {
    let log: Log = Arc::new(Mutex::new(Vec::new()));
    let widget = RichTextEditor::create(RichTextEditorState::create(doc))
        .with_on_change(
            RefAny::new(log.clone()),
            record as RichTextEditorOnChangeCallbackType,
        )
        .dom();
    let mut lw = lay_out(Dom::create_body().with_child(widget));
    let host = host(&lw);
    lw.focus_manager.set_focused_node(Some(dnid(host)));
    (lw, log)
}

fn styled(lw: &LayoutWindow) -> &StyledDom {
    &lw.layout_results
        .get(&DomId::ROOT_ID)
        .expect("the window is laid out")
        .styled_dom
}

/// The editing host's node index.
fn host(lw: &LayoutWindow) -> usize {
    styled(lw)
        .node_data
        .as_ref()
        .iter()
        .position(|n| n.has_id(DEFAULT_HOST_ID))
        .expect("the editor's host carries its id")
}

/// The node index of the text node holding exactly `text`.
fn text_node(lw: &LayoutWindow, text: &str) -> usize {
    styled(lw)
        .node_data
        .as_ref()
        .iter()
        .position(|n| matches!(n.get_node_type(), NodeType::Text(t) if t.as_ref().as_str() == text))
        .unwrap_or_else(|| panic!("a text node {text:?}"))
}

/// Opens the editing session from byte `start` of text node `start_node`
/// to byte `end` of text node `end_node` (a caret when they are equal).
fn select(lw: &mut LayoutWindow, start_node: usize, start: u32, end_node: usize, end: u32) {
    let block = lw.text_block_of(dnid(start_node)).expect("a text block");
    let anchor = lw
        .caret_at_node_byte(block, NodeId::new(start_node), start)
        .expect("the anchor is laid out");
    let focus = lw
        .caret_at_node_byte(block, NodeId::new(end_node), end)
        .expect("the focus is laid out");
    lw.open_session(
        block,
        SelectionRange {
            start: anchor,
            end: focus,
        },
    );
}

/// Types `text` at the session's caret, as a keystroke lands.
fn type_text(lw: &mut LayoutWindow, text: &str) {
    let _ = lw.record_text_input(text);
    let _ = lw.apply_text_changeset();
}

/// Runs `f` against a `CallbackInfo` over `lw`, hit on `hit`, the way a
/// user callback sees it.
fn with_info<R>(
    lw: &LayoutWindow,
    hit: DomNodeId,
    f: impl FnOnce(CallbackInfo) -> R,
) -> (R, Vec<CallbackChange>) {
    let renderer_resources = RendererResources::default();
    let previous_window_state: Option<FullWindowState> = None;
    let current_window_state = lw.current_window_state.clone();
    let gl_context = OptionGlContextPtr::None;
    let scroll_states: BTreeMap<DomId, BTreeMap<NodeHierarchyItemId, ScrollPosition>> =
        BTreeMap::new();
    let window_handle = RawWindowHandle::Unsupported;
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let ref_data = CallbackInfoRefData {
        layout_window: lw,
        renderer_resources: &renderer_resources,
        previous_window_state: &previous_window_state,
        current_window_state: &current_window_state,
        gl_context: &gl_context,
        current_scroll_manager: &scroll_states,
        current_window_handle: &window_handle,
        system_callbacks: &system_callbacks,
        system_style: Arc::new(SystemStyle::default()),
        monitors: Arc::new(Mutex::new(MonitorVec::from_const_slice(&[]))),
        #[cfg(feature = "icu")]
        icu_localizer: azul_layout::icu::IcuLocalizerHandle::default(),
        ctx: core::cell::RefCell::new(OptionRefAny::None),
    };
    let changes: Arc<Mutex<Vec<CallbackChange>>> = Arc::new(Mutex::new(Vec::new()));
    let info = CallbackInfo::new(
        &ref_data,
        &changes,
        hit,
        OptionLogicalPosition::None,
        OptionLogicalPosition::None,
    );
    let out = f(info);
    let queued = info.take_changes();
    (out, queued)
}

/// Fires the host's handler for `event`, as the engine dispatches it to the
/// focused editing host.
fn fire(lw: &LayoutWindow, event: EventFilter) -> (Update, Vec<CallbackChange>) {
    let host = host(lw);
    let (callback, data) = styled(lw).node_data.as_ref()[host]
        .get_callbacks()
        .as_ref()
        .iter()
        .find(|cb| cb.event == event)
        .map(|cb| (cb.callback.clone(), cb.refany.clone()))
        .expect("the host handles the event");
    with_info(lw, dnid(host), |info| {
        Callback::from_core(callback).invoke(data, info)
    })
}

/// The last state the editor reported.
fn last(log: &Log) -> RichTextEditorState {
    log.lock()
        .expect("log")
        .last()
        .cloned()
        .expect("the editor reported a change")
}

/// The key `key` pressed with the platform's primary modifier (both Ctrl
/// and Cmd held, so the test reads the same on every platform).
fn press_primary(lw: &mut LayoutWindow, key: VirtualKeyCode) {
    lw.current_window_state.keyboard_state = KeyboardState {
        current_virtual_keycode: Some(key).into(),
        pressed_virtual_keycodes: vec![VirtualKeyCode::LControl, VirtualKeyCode::LWin, key].into(),
        ..Default::default()
    };
}

fn prevented(changes: &[CallbackChange]) -> bool {
    changes
        .iter()
        .any(|c| matches!(c, CallbackChange::PreventDefault))
}

// ---- the tests ----

#[test]
fn typing_into_a_paragraph_with_bold_and_a_link_keeps_both_in_the_editors_model() {
    let (mut lw, log) = editor(mail_doc());
    let ab = text_node(&lw, "ab");
    select(&mut lw, ab, 2, ab, 2);
    type_text(&mut lw, "X");

    let _ = fire(&lw, EventFilter::Focus(FocusEventFilter::TextChanged));

    let state = last(&log);
    assert_eq!(
        state.doc.blocks()[0].runs_vec(),
        vec![plain("abX"), bold("c"), linked("d")],
        "the typed X joins the plain run; the bold run and the link stay"
    );
    assert_eq!(state.doc.blocks()[1].flat(), "second");
}

#[test]
fn ctrl_or_cmd_b_over_a_selection_makes_it_bold_in_the_model_and_cancels_the_engines_default() {
    let (mut lw, log) = editor(RichTextDoc::from_blocks(vec![RichBlock::paragraph(
        "hello world",
    )]));
    let text = text_node(&lw, "hello world");
    select(&mut lw, text, 6, text, 11);
    press_primary(&mut lw, VirtualKeyCode::B);

    let (_, changes) = fire(&lw, EventFilter::Focus(FocusEventFilter::VirtualKeyDown));

    assert!(
        prevented(&changes),
        "the editor formats the selection; the engine's default (a typing style) is cancelled"
    );
    assert_eq!(
        last(&log).doc.blocks()[0].runs_vec(),
        vec![plain("hello "), bold("world")]
    );
}

#[test]
fn bold_pressed_twice_over_a_selection_leaves_it_plain() {
    let (mut lw, log) = editor(RichTextDoc::from_blocks(vec![RichBlock::paragraph(
        "hello world",
    )]));
    let text = text_node(&lw, "hello world");
    select(&mut lw, text, 6, text, 11);
    press_primary(&mut lw, VirtualKeyCode::B);

    let _ = fire(&lw, EventFilter::Focus(FocusEventFilter::VirtualKeyDown));
    let _ = fire(&lw, EventFilter::Focus(FocusEventFilter::VirtualKeyDown));

    assert_eq!(
        last(&log).doc.blocks()[0].runs_vec(),
        vec![plain("hello world")],
        "the second press takes the bold off again, it does not nest another"
    );
}

#[test]
fn enter_is_acked_without_an_engine_side_inverse_and_the_editors_undo_takes_it_back() {
    let (mut lw, log) = editor(RichTextDoc::from_blocks(vec![RichBlock::paragraph(
        "hello world",
    )]));
    let text = text_node(&lw, "hello world");
    select(&mut lw, text, 5, text, 5);
    let focused = Some(dnid(host(&lw)));
    let editing = lw
        .build_editing_query_state(focused)
        .expect("the focus is in a contenteditable host");
    let keys = KeyboardState {
        current_virtual_keycode: Some(VirtualKeyCode::Return).into(),
        pressed_virtual_keycodes: vec![VirtualKeyCode::Return].into(),
        ..Default::default()
    };
    let action = azul_layout::default_actions::determine_keyboard_default_action_with_editing(
        &keys,
        focused,
        &lw.layout_results,
        false,
        Some(&editing),
    )
    .action;
    assert!(
        matches!(action, DefaultAction::SplitBlockAtCursor { .. }),
        "premise: Enter splits the paragraph"
    );
    assert!(
        lw.record_structural_default_action(&action).is_some(),
        "premise: the split is recorded"
    );

    let (update, changes) = fire(&lw, EventFilter::Focus(FocusEventFilter::DocumentEdit));

    assert_eq!(update, Update::RefreshDom);
    assert!(
        changes
            .iter()
            .any(|c| matches!(c, CallbackChange::MarkDocumentEditApplied { .. })),
        "the split is acknowledged: {changes:?}"
    );
    assert!(
        !changes
            .iter()
            .any(|c| matches!(c, CallbackChange::MarkDocumentEditAppliedWithInverse { .. })),
        "without an inverse: the engine keeps no second history of it"
    );
    let state = last(&log);
    let texts: Vec<String> = state.doc.blocks().iter().map(RichBlock::flat).collect();
    assert_eq!(texts, vec!["hello", " world"]);
    assert!(state.history.can_undo());

    // The app's Undo (a ribbon, a QuickAccess button) runs on the state it
    // keeps: the ONE history.
    let mut kept = state;
    let host = dnid(host(&lw));
    let (update, changes) = with_info(&lw, host, |info| {
        kept.apply_command(info, RichTextCommand::Undo)
    });
    assert_eq!(update, Update::RefreshDom);
    assert_eq!(
        kept.doc,
        RichTextDoc::from_blocks(vec![RichBlock::paragraph("hello world")])
    );
    assert!(
        changes
            .iter()
            .any(|c| matches!(c, CallbackChange::ResetEditorContent { .. })),
        "the engine's editing state for the host goes with the undone content: {changes:?}"
    );
    assert!(kept.history.can_redo());
}

#[test]
fn an_undo_of_a_format_puts_the_typing_and_the_format_back_one_step_at_a_time() {
    let (mut lw, log) = editor(RichTextDoc::from_blocks(vec![RichBlock::paragraph(
        "hello world",
    )]));
    let text = text_node(&lw, "hello world");
    select(&mut lw, text, 11, text, 11);
    type_text(&mut lw, "!");
    let _ = fire(&lw, EventFilter::Focus(FocusEventFilter::TextChanged));
    select(&mut lw, text, 0, text, 5);
    press_primary(&mut lw, VirtualKeyCode::B);
    let _ = fire(&lw, EventFilter::Focus(FocusEventFilter::VirtualKeyDown));

    let mut kept = last(&log);
    assert_eq!(
        kept.doc.blocks()[0].runs_vec(),
        vec![bold("hello"), plain(" world!")]
    );
    let host = dnid(host(&lw));
    let _ = with_info(&lw, host, |info| {
        kept.apply_command(info, RichTextCommand::Undo)
    });
    assert_eq!(
        kept.doc.blocks()[0].runs_vec(),
        vec![plain("hello world!")],
        "the format goes first"
    );
    let _ = with_info(&lw, host, |info| {
        kept.apply_command(info, RichTextCommand::Undo)
    });
    assert_eq!(
        kept.doc.blocks()[0].runs_vec(),
        vec![plain("hello world")],
        "then the typing"
    );
    let _ = with_info(&lw, host, |info| {
        kept.apply_command(info, RichTextCommand::Redo)
    });
    assert_eq!(kept.doc.blocks()[0].flat(), "hello world!");
}

// ---- WRITER6: the formats and the undo keys come from the engine ----

/// The key `key` pressed with the primary modifier and Shift.
fn press_primary_shift(lw: &mut LayoutWindow, key: VirtualKeyCode) {
    lw.current_window_state.keyboard_state = KeyboardState {
        current_virtual_keycode: Some(key).into(),
        pressed_virtual_keycodes: vec![
            VirtualKeyCode::LControl,
            VirtualKeyCode::LWin,
            VirtualKeyCode::LShift,
            key,
        ]
        .into(),
        ..Default::default()
    };
}

/// What the shell does with a callback's acknowledgements: the synced
/// revision and a reset of the host's editing state.
fn apply_acks(lw: &mut LayoutWindow, changes: &[CallbackChange]) {
    for change in changes {
        match change {
            CallbackChange::MarkTextRevisionSynced { revision } => {
                lw.mark_text_revision_synced(*revision);
            }
            CallbackChange::ResetEditorContent { host, caret_at_end } => {
                let _ = lw.reset_editor_content(*host, *caret_at_end);
            }
            _ => {}
        }
    }
}

/// The engine's typing style (its Ctrl/Cmd+B default action, or a toolbar
/// button's `toggle_text_format`) set where the editor's key handler never
/// saw it: the typed text is bold on screen, and the editor takes the bold
/// from the engine's edit report (`DocumentTextEdit::runs`), not from a
/// mirror of its own.
#[test]
fn text_typed_after_the_engines_bold_toggle_at_a_caret_is_bold_in_the_model() {
    let (mut lw, log) = editor(RichTextDoc::from_blocks(vec![RichBlock::paragraph(
        "hello world",
    )]));
    let text = text_node(&lw, "hello world");
    select(&mut lw, text, 5, text, 5);
    let host_node = dnid(host(&lw));
    let _ = lw.toggle_text_format(host_node, azul_core::events::TextFormat::Bold);
    type_text(&mut lw, "X");

    let _ = fire(&lw, EventFilter::Focus(FocusEventFilter::TextChanged));

    assert_eq!(
        last(&log).doc.blocks()[0].runs_vec(),
        vec![plain("hello"), bold("X"), plain(" world")]
    );
}

/// An inline formatted paste (`a <b>b</b> c`) lands in the engine's
/// overlay with its bold; the model keeps the bold (DEDUP_EDITORS D1).
#[test]
fn a_pasted_bold_word_is_bold_in_the_model() {
    let (mut lw, log) = editor(RichTextDoc::from_blocks(vec![RichBlock::paragraph(
        "hello world",
    )]));
    let text = text_node(&lw, "hello world");
    select(&mut lw, text, 5, text, 5);
    let _ = lw.paste_clipboard_content(
        &crate::a_rich_paste_inserts_formatting_and_blocks::clipboard("a <b>b</b> c", "a b c"),
    );
    let _ = lw.apply_text_changeset();

    let _ = fire(&lw, EventFilter::Focus(FocusEventFilter::TextChanged));

    assert_eq!(
        last(&log).doc.blocks()[0].runs_vec(),
        vec![plain("helloa "), bold("b"), plain(" c world")]
    );
}

/// Ctrl/Cmd+Z, Ctrl/Cmd+Shift+Z and Ctrl/Cmd+Y reach the editor (the
/// engine's text undo is only the key's default action now): the editor
/// runs its ONE history and cancels the engine's undo.
#[test]
fn ctrl_or_cmd_z_undoes_the_editors_own_history_and_cancels_the_engines_text_undo() {
    let (mut lw, log) = editor(RichTextDoc::from_blocks(vec![RichBlock::paragraph(
        "hello world",
    )]));
    let text = text_node(&lw, "hello world");
    select(&mut lw, text, 11, text, 11);
    type_text(&mut lw, "!");
    let (_, changes) = fire(&lw, EventFilter::Focus(FocusEventFilter::TextChanged));
    apply_acks(&mut lw, &changes);
    assert_eq!(last(&log).doc.blocks()[0].flat(), "hello world!");

    press_primary(&mut lw, VirtualKeyCode::Z);
    let (update, changes) = fire(&lw, EventFilter::Focus(FocusEventFilter::VirtualKeyDown));
    assert!(prevented(&changes), "the engine's text undo is cancelled");
    assert_eq!(update, Update::RefreshDom);
    assert_eq!(last(&log).doc.blocks()[0].flat(), "hello world");
    assert!(
        changes
            .iter()
            .any(|c| matches!(c, CallbackChange::ResetEditorContent { .. })),
        "the engine drops its editing state of the undone text: {changes:?}"
    );
    apply_acks(&mut lw, &changes);

    press_primary_shift(&mut lw, VirtualKeyCode::Z);
    let (_, changes) = fire(&lw, EventFilter::Focus(FocusEventFilter::VirtualKeyDown));
    assert!(prevented(&changes));
    assert_eq!(
        last(&log).doc.blocks()[0].flat(),
        "hello world!",
        "Shift+Z redoes"
    );
    apply_acks(&mut lw, &changes);

    press_primary(&mut lw, VirtualKeyCode::Z);
    let (_, changes) = fire(&lw, EventFilter::Focus(FocusEventFilter::VirtualKeyDown));
    apply_acks(&mut lw, &changes);
    press_primary(&mut lw, VirtualKeyCode::Y);
    let (_, changes) = fire(&lw, EventFilter::Focus(FocusEventFilter::VirtualKeyDown));
    assert!(prevented(&changes));
    assert_eq!(
        last(&log).doc.blocks()[0].flat(),
        "hello world!",
        "Y redoes too"
    );
}

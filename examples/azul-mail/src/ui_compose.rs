//! The compose window: New, Reply, Reply All, Forward and a draft opened again.
//!
//! Outlook 2010's message window, as a window of its own (`CallbackInfo::create_window`): the
//! title row, a ribbon (Basic Text: Bold, Italic, Underline, Bullets, Numbering; Include: Attach
//! File, Link; Save: Save, Discard), the header block (Send beside From, To, Cc, Bcc, Subject),
//! the attachments, and the rich editor (`editor.rs`) on paper. Every compose window shares the
//! app's state; its layout callback finds ITS compose by the key it carries in the callback's
//! context (`LayoutCallback::ctx`, read with `LayoutCallbackInfo::get_ctx`).
//!
//! Save writes the draft into the account's Drafts folder and Send hands the mail to SEND's
//! `send::send_mail`, both on an azul `Thread` of this window; once the mail is sent the window
//! closes and the mail is in Sent Items. Queued (it waits in the Outbox for the next Send /
//! Receive) and Failed keep the window open and say why.
//!
//! Field ids for scripts: `#compose-to`, `#compose-cc`, `#compose-bcc`, `#compose-subject`,
//! `#compose-body` (the editor), `#compose-link`; the window id is `azmail-compose-<n>`.

use std::path::PathBuf;

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, ResumeCallbackType, TextInputOnTextInputCallbackType,
        TimerCallbackInfo, TimerCallbackReturn,
    },
    dialog::{FileDialog, FileOpenMultiResult},
    dom::{DomId, FocusTarget, TextFormat, VirtualKeyCode},
    option::{OptionFileTypeList, OptionString},
    prelude::*,
    shells::{DocumentShell, ShellThemeAccent, ShellThemeScope},
    str::String as AzString,
    task::{TimerId, Timer},
    time::{Duration, SystemTimeDiff},
    widgets::{
        ButtonType, OnTextInputReturn, Ribbon, RibbonButton, RibbonGroup, RibbonItem, RibbonTab,
        StatusBar, StatusBarSegment, TextInputState, TextInputValid, Titlebar,
    },
    window::WindowDecorations,
};

use crate::{
    compose::{self, ComposeFields, ComposeKind, MailDoc, StartFields},
    editor, send,
    store::LocalFolder,
    with_app, MailApp,
};

/// A compose window's state.
pub(crate) struct Compose {
    pub(crate) id: u64,
    /// `azmail-compose-<id>`: the window's id (the debug server addresses windows by it).
    pub(crate) window_id: String,
    pub(crate) kind: ComposeKind,
    /// The account it is sent from.
    pub(crate) account_id: String,
    /// The From line (`Name <address>`).
    pub(crate) from: String,
    pub(crate) to: String,
    pub(crate) cc: String,
    pub(crate) bcc: String,
    pub(crate) subject: String,
    /// The editor's model: the contenteditable host with its blocks (`editor.rs`).
    pub(crate) body: Dom,
    pub(crate) in_reply_to: Option<String>,
    pub(crate) references: Vec<String>,
    pub(crate) attachments: Vec<AttachedFile>,
    /// The draft this mail was last saved as (its local UID in the Drafts folder).
    pub(crate) draft_uid: Option<u32>,
    pub(crate) status: ComposeStatus,
    /// The Insert Link field is shown, and its text.
    pub(crate) show_link: bool,
    pub(crate) link: String,
}

/// A file to attach, read when the mail is saved or sent (on the thread).
#[derive(Debug, Clone)]
pub(crate) struct AttachedFile {
    pub(crate) path: PathBuf,
    pub(crate) name: String,
    pub(crate) size: u64,
}

/// What the window says under the ribbon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ComposeStatus {
    Editing,
    Saving,
    Saved(String),
    Sending,
    /// In the Outbox: sent with the next Send / Receive.
    Queued(String),
    Failed(String),
    Problem(String),
}

impl Compose {
    /// What Send and Save take from the window.
    fn fields(&self) -> ComposeFields {
        ComposeFields {
            from: self.from.clone(),
            to: self.to.clone(),
            cc: self.cc.clone(),
            bcc: self.bcc.clone(),
            subject: self.subject.clone(),
            body: editor::host_to_doc(&self.body),
            in_reply_to: self.in_reply_to.clone(),
            references: self.references.clone(),
        }
    }

    /// Sending, or sent to the Outbox: Send would send it twice.
    fn busy(&self) -> bool {
        matches!(
            self.status,
            ComposeStatus::Sending | ComposeStatus::Saving | ComposeStatus::Queued(_)
        )
    }
}

/// The key a compose window's layout callback carries in its context.
struct ComposeKey(u64);

/// A compose window's callbacks' data: the app and which compose.
struct ComposeRef {
    app: RefAny,
    id: u64,
}

fn compose_ref(app: &RefAny, id: u64) -> RefAny {
    RefAny::new(ComposeRef {
        app: app.clone(),
        id,
    })
}

/// `(app, id)` of a callback's data.
fn target_of(data: &mut RefAny) -> Option<(RefAny, u64)> {
    data.downcast_ref::<ComposeRef>()
        .map(|r| (r.app.clone(), r.id))
}

// ==== Opening ====

/// Opens a compose window for `kind` from the current account (Reply / Reply All / Forward /
/// Draft: of the message open in the reading pane).
pub(crate) fn open_compose(s: &mut MailApp, info: &mut CallbackInfo, app: RefAny, kind: ComposeKind) {
    let Some(account) = s.current_account().cloned() else {
        s.notice = String::from("Add an account first (File > Add Account).");
        return;
    };
    let original = s.open.as_ref().and_then(|o| o.view.clone());
    let draft_uid = s
        .open
        .as_ref()
        .filter(|_| kind == ComposeKind::Draft)
        .map(|o| o.entry.uid);
    let header_date = |date: &str| compose::header_date_in(date, &chrono::Local);
    let (start, body) = match (kind, original.as_ref()) {
        (ComposeKind::Reply | ComposeKind::ReplyAll, Some(view)) => {
            let fields = compose::reply_fields(view, &account.email, kind == ComposeKind::ReplyAll);
            let header = compose::quote_header(&header_date(&view.date), &view.from);
            (fields, MailDoc::reply_quote(view, &header))
        }
        (ComposeKind::Forward, Some(view)) => (
            compose::forward_fields(view),
            MailDoc::forward_quote(view, &header_date(&view.date)),
        ),
        (ComposeKind::Draft, Some(view)) => {
            (compose::draft_fields(view), MailDoc::from_plain(&view.text, 0))
        }
        _ => (StartFields::default(), MailDoc::empty()),
    };
    let id = s.next_compose;
    s.next_compose += 1;
    let window_id = format!("azmail-compose-{id}");
    let title = compose::window_title(&start.subject);
    s.composes.push(Compose {
        id,
        window_id: window_id.clone(),
        kind,
        account_id: account.id.clone(),
        from: account.sender(),
        to: start.to,
        cc: start.cc,
        bcc: start.bcc,
        subject: start.subject,
        body: editor::doc_to_host(&body),
        in_reply_to: start.in_reply_to,
        references: start.references,
        attachments: Vec::new(),
        draft_uid,
        status: ComposeStatus::Editing,
        show_link: false,
        link: String::new(),
    });
    let mut window = WindowCreateOptions::create(layout_compose);
    window.window_state.layout_callback.ctx = OptionRefAny::Some(RefAny::new(ComposeKey(id)));
    window.window_state.window_id = AzString::from(window_id.as_str());
    window.window_state.title = AzString::from(title);
    window.window_state.size.dimensions = LogicalSize::create(880.0, 700.0);
    window.window_state.flags.decorations = WindowDecorations::NoTitle;
    window.create_callback = Some(Callback::create(on_compose_created)).into();
    info.create_window(window);
    println!("AZMAIL_COMPOSE_OPEN {window_id} {}", kind.name());
    let _ = app;
}

/// The compose window is up: once its editor is laid out, the caret goes to the top (above a
/// reply's quote) and the editor takes the focus.
extern "C" fn on_compose_created(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let window_id = info.get_current_window_state().window_id.as_str().to_string();
    let app = data.clone();
    let Some(id) = data
        .downcast_ref::<MailApp>()
        .and_then(|s| s.composes.iter().find(|c| c.window_id == window_id).map(|c| c.id))
    else {
        return Update::DoNothing;
    };
    let get_time = info.get_system_time_fn();
    info.add_timer(
        TimerId::unique(),
        Timer::create(compose_ref(&app, id), on_caret_timer, get_time)
            .with_interval(Duration::System(SystemTimeDiff::from_millis(40))),
    );
    Update::DoNothing
}

/// Puts the caret at the top of the editor once the host is there (a few frames at most).
extern "C" fn on_caret_timer(_data: RefAny, info: TimerCallbackInfo) -> TimerCallbackReturn {
    let mut callback_info = info.callback_info;
    let dom = DomId { inner: 0 };
    let node = callback_info.get_node_id_by_id_attribute(dom, editor::HOST_ID);
    if node.into_raw() == 0 {
        return if info.call_count > 50 {
            TimerCallbackReturn::terminate_unchanged()
        } else {
            TimerCallbackReturn::continue_unchanged()
        };
    }
    let host = azul::dom::DomNodeId { dom, node };
    callback_info.set_focus(FocusTarget::Id(host));
    callback_info.reset_editor_content(host, false);
    TimerCallbackReturn::terminate_unchanged()
}

// ==== The window ====

/// The compose this window shows (its layout callback's context).
fn compose_id_of(info: &LayoutCallbackInfo) -> Option<u64> {
    let mut ctx = info.get_ctx().into_option()?;
    let id = ctx.downcast_ref::<ComposeKey>().map(|k| k.0);
    id
}

/// A compose window's layout.
pub(crate) extern "C" fn layout_compose(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
    let _mode = info.get_mode();
    let app = data.clone();
    let Some(id) = compose_id_of(&info) else {
        return Dom::create_body();
    };
    let Some(guard) = data.downcast_ref::<MailApp>() else {
        return Dom::create_body();
    };
    let s = &*guard;
    let Some(c) = s.composes.iter().find(|c| c.id == id) else {
        return Dom::create_body()
            .with_child(Dom::create_span_with_text("This message was closed."));
    };
    let mut document = Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
        .with_child(header_block(c, &app));
    if c.show_link {
        document.add_child(link_bar(c, &app));
    }
    if !c.attachments.is_empty() {
        document.add_child(attachments_row(c, &app));
    }
    document.add_child(editor_dom(c, &app));
    let shell = DocumentShell::create(document)
        .with_ribbon(compose_ribbon(c, &app))
        .with_status_bar(status_bar(c));
    let column = Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
        .with_child(
            Titlebar::create(compose::window_title(&c.subject))
                .without_border_bottom()
                .dom(),
        )
        .with_child(shell.dom());
    Dom::create_body()
        .with_css("display: flex; flex-direction: column; margin: 0px;")
        .with_child(
            ShellThemeScope::create(column)
                .with_accent(ShellThemeAccent::Blue)
                .dom(),
        )
        .with_callback(
            EventFilter::Window(WindowEventFilter::VirtualKeyDown),
            compose_ref(&app, id),
            on_compose_key,
        )
        .with_callback(
            EventFilter::Window(WindowEventFilter::CloseRequested),
            compose_ref(&app, id),
            on_compose_close_requested,
        )
}

/// What a ribbon button or a key does in the compose window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ComposeAction {
    Send,
    Save,
    Discard,
    Bold,
    Italic,
    Underline,
    Bullets,
    Numbering,
    AttachFile,
    ToggleLink,
    InsertLink,
    RemoveAttachment(usize),
}

struct ComposeActionRef {
    app: RefAny,
    id: u64,
    action: ComposeAction,
}

fn action_ref(app: &RefAny, id: u64, action: ComposeAction) -> RefAny {
    RefAny::new(ComposeActionRef {
        app: app.clone(),
        id,
        action,
    })
}

fn compose_ribbon(c: &Compose, app: &RefAny) -> Dom {
    let button = |icon: &str, label: &str, action: ComposeAction| {
        RibbonButton::create(icon, label).with_on_click(
            action_ref(app, c.id, action),
            on_compose_action as ButtonOnClickCallbackType,
        )
    };
    let small = |icon: &str, label: &str, action: ComposeAction| {
        RibbonItem::SmallButton(button(icon, label, action))
    };
    let message = RibbonTab::create("Message")
        .with_group(
            RibbonGroup::create("Basic Text")
                .with_item(small("format_bold", "Bold", ComposeAction::Bold))
                .with_item(small("format_italic", "Italic", ComposeAction::Italic))
                .with_item(small("format_underlined", "Underline", ComposeAction::Underline))
                .with_item(small("format_list_bulleted", "Bullets", ComposeAction::Bullets))
                .with_item(small("format_list_numbered", "Numbering", ComposeAction::Numbering)),
        )
        .with_group(
            RibbonGroup::create("Include")
                .with_item(RibbonItem::LargeButton(button(
                    "attach_file",
                    "Attach File",
                    ComposeAction::AttachFile,
                )))
                .with_item(RibbonItem::SmallButton(
                    button("link", "Link", ComposeAction::ToggleLink).with_toggled(c.show_link),
                )),
        )
        .with_group(
            RibbonGroup::create("Save")
                .with_item(small("save", "Save Draft", ComposeAction::Save))
                .with_item(small("delete", "Discard", ComposeAction::Discard)),
        );
    Ribbon::create(vec![message]).dom_desktop()
}

/// A compose field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ComposeField {
    To,
    Cc,
    Bcc,
    Subject,
    Link,
}

struct ComposeFieldRef {
    app: RefAny,
    id: u64,
    field: ComposeField,
}

fn field_input(app: &RefAny, id: u64, field: ComposeField, value: &str, dom_id: &str) -> Dom {
    TextInput::create()
        .with_text(value)
        .with_on_text_input(
            RefAny::new(ComposeFieldRef {
                app: app.clone(),
                id,
                field,
            }),
            on_compose_field as TextInputOnTextInputCallbackType,
        )
        .dom()
        .with_id(dom_id)
        .with_css("flex-grow: 1;")
}

/// Send beside the From / To / Cc / Bcc / Subject rows.
fn header_block(c: &Compose, app: &RefAny) -> Dom {
    let row = |label: &str, field: Dom| {
        Dom::create_div()
            .with_css("display: flex; flex-direction: row; align-items: center; margin-bottom: 4px;")
            .with_child(
                Dom::create_span_with_text(label)
                    .with_css("width: 64px; flex-shrink: 0; font-size: 13px;"),
            )
            .with_child(field)
    };
    let send_label = match &c.status {
        ComposeStatus::Sending => "Sending...",
        ComposeStatus::Queued(_) => "Queued",
        _ => "Send",
    };
    let send = Button::with_type(send_label, ButtonType::Primary)
        .with_icon("send")
        .with_on_click(
            action_ref(app, c.id, ComposeAction::Send),
            on_compose_action as ButtonOnClickCallbackType,
        )
        .dom()
        .with_id("compose-send")
        .with_css("width: 72px; min-height: 64px; margin-right: 10px;");
    let fields = Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1;")
        .with_child(row(
            "From",
            Dom::create_span_with_text(c.from.as_str()).with_css("font-size: 13px;"),
        ))
        .with_child(row(
            "To...",
            field_input(app, c.id, ComposeField::To, &c.to, "compose-to"),
        ))
        .with_child(row(
            "Cc...",
            field_input(app, c.id, ComposeField::Cc, &c.cc, "compose-cc"),
        ))
        .with_child(row(
            "Bcc...",
            field_input(app, c.id, ComposeField::Bcc, &c.bcc, "compose-bcc"),
        ))
        .with_child(row(
            "Subject:",
            field_input(app, c.id, ComposeField::Subject, &c.subject, "compose-subject"),
        ));
    Dom::create_div()
        .with_css("display: flex; flex-direction: row; padding: 10px 14px 6px 14px; flex-shrink: 0;")
        .with_child(send)
        .with_child(fields)
}

/// Insert Link: the address, and the button that makes the selection (or the address) a link.
fn link_bar(c: &Compose, app: &RefAny) -> Dom {
    Dom::create_div()
        .with_css(
            "display: flex; flex-direction: row; align-items: center; padding: 0px 14px 6px \
             96px; flex-shrink: 0;",
        )
        .with_child(Dom::create_span_with_text("Address:").with_css("font-size: 13px; margin-right: 8px;"))
        .with_child(field_input(app, c.id, ComposeField::Link, &c.link, "compose-link"))
        .with_child(
            Button::create("Insert Link")
                .with_on_click(
                    action_ref(app, c.id, ComposeAction::InsertLink),
                    on_compose_action as ButtonOnClickCallbackType,
                )
                .dom()
                .with_css("margin-left: 8px;"),
        )
}

/// The attached files, each with Remove.
fn attachments_row(c: &Compose, app: &RefAny) -> Dom {
    let mut row = Dom::create_div().with_css(
        "display: flex; flex-direction: row; flex-wrap: wrap; align-items: center; padding: 0px \
         14px 6px 96px; flex-shrink: 0;",
    );
    row.add_child(Dom::create_span_with_text("Attached:").with_css("font-size: 13px; margin-right: 8px;"));
    for (i, file) in c.attachments.iter().enumerate() {
        row.add_child(
            Dom::create_span_with_text(format!("{} ({} KB)", file.name, (file.size + 1023) / 1024))
                .with_css("font-size: 13px; margin-right: 4px;"),
        );
        row.add_child(
            Button::create("Remove")
                .with_on_click(
                    action_ref(app, c.id, ComposeAction::RemoveAttachment(i)),
                    on_compose_action as ButtonOnClickCallbackType,
                )
                .dom()
                .with_css("margin-right: 12px;"),
        );
    }
    row
}

/// The editor: the model's host, with the engine's edit events.
fn editor_dom(c: &Compose, app: &RefAny) -> Dom {
    let host = c
        .body
        .clone()
        .with_callback(
            EventFilter::Focus(FocusEventFilter::TextChanged),
            compose_ref(app, c.id),
            on_compose_text_changed,
        )
        .with_callback(
            EventFilter::Focus(FocusEventFilter::DocumentEdit),
            compose_ref(app, c.id),
            on_compose_document_edit,
        );
    Dom::create_div()
        .with_css(
            "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; padding: 0px \
             14px 14px 14px;",
        )
        .with_child(host)
}

/// The status line: what Save / Send did.
fn status_bar(c: &Compose) -> Dom {
    let text = match &c.status {
        ComposeStatus::Editing => match c.kind {
            ComposeKind::Reply | ComposeKind::ReplyAll => String::from("Reply"),
            ComposeKind::Forward => String::from("Forward"),
            ComposeKind::Draft => String::from("Draft"),
            ComposeKind::New => String::from("New message"),
        },
        ComposeStatus::Saving => String::from("Saving the draft..."),
        ComposeStatus::Saved(at) => format!("Draft saved at {at}."),
        ComposeStatus::Sending => String::from("Sending..."),
        ComposeStatus::Queued(reason) => {
            format!("In the Outbox, sent with the next Send/Receive: {reason}")
        }
        ComposeStatus::Failed(reason) => format!("Not sent: {reason}"),
        ComposeStatus::Problem(text) => text.clone(),
    };
    let mut segments = vec![StatusBarSegment::create(text)];
    if !c.attachments.is_empty() {
        segments.push(StatusBarSegment::create(format!(
            "{} attachment(s)",
            c.attachments.len()
        )));
    }
    StatusBar::create(segments).dom()
}

// ==== Callbacks ====

/// Runs `f` on the compose `id` (and the app), when it is still open.
fn with_compose<R>(
    app: &mut RefAny,
    id: u64,
    f: impl FnOnce(&mut MailApp, usize, RefAny) -> R,
) -> Option<R> {
    with_app(app, |s, app| {
        let at = s.composes.iter().position(|c| c.id == id)?;
        Some(f(s, at, app))
    })
    .flatten()
}

extern "C" fn on_compose_field(mut data: RefAny, _info: CallbackInfo, state: TextInputState) -> OnTextInputReturn {
    let target = data
        .downcast_ref::<ComposeFieldRef>()
        .map(|r| (r.app.clone(), r.id, r.field));
    if let Some((mut app, id, field)) = target {
        let text = state.get_text().as_str().to_string();
        let _ = with_compose(&mut app, id, |s, at, _| {
            let c = &mut s.composes[at];
            match field {
                ComposeField::To => c.to = text,
                ComposeField::Cc => c.cc = text,
                ComposeField::Bcc => c.bcc = text,
                ComposeField::Subject => c.subject = text,
                ComposeField::Link => c.link = text,
            }
        });
    }
    OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    }
}

/// Typing in the editor: the text goes into the model (no rebuild).
extern "C" fn on_compose_text_changed(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, id)) = target_of(&mut data) else {
        return Update::DoNothing;
    };
    let Some(host) = editor::host_node(&info) else {
        return Update::DoNothing;
    };
    let _ = with_compose(&mut app, id, |s, at, _| {
        editor::sync_text(&mut s.composes[at].body, &mut info, host);
    });
    Update::DoNothing
}

/// Enter, Backspace / Delete across blocks, a paste: the engine's structural edit goes into the
/// model, and the window rebuilds from it.
extern "C" fn on_compose_document_edit(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, id)) = target_of(&mut data) else {
        return Update::DoNothing;
    };
    let Some(host) = editor::host_node(&info) else {
        return Update::DoNothing;
    };
    with_compose(&mut app, id, |s, at, _| {
        let body = &mut s.composes[at].body;
        editor::sync_text(body, &mut info, host);
        if editor::apply_structural_edit(body, &mut info, host) {
            Update::RefreshDom
        } else {
            Update::DoNothing
        }
    })
    .unwrap_or(Update::DoNothing)
}

/// Ctrl/Cmd+Enter sends, Ctrl/Cmd+S saves the draft (Ctrl/Cmd+B / I / U are the editor's own).
extern "C" fn on_compose_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, id)) = target_of(&mut data) else {
        return Update::DoNothing;
    };
    let keyboard = info.get_current_keyboard_state();
    let modifiers = info.get_key_modifiers();
    if !(modifiers.ctrl || modifiers.meta) {
        return Update::DoNothing;
    }
    let action = match keyboard.current_virtual_keycode.into_option() {
        Some(VirtualKeyCode::Return) => ComposeAction::Send,
        Some(VirtualKeyCode::S) => ComposeAction::Save,
        _ => return Update::DoNothing,
    };
    info.prevent_default();
    run_compose_action(&mut app, &mut info, id, action)
}

/// The window is being closed (its close button): its compose goes.
extern "C" fn on_compose_close_requested(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some((mut app, id)) = target_of(&mut data) else {
        return Update::DoNothing;
    };
    let _ = with_compose(&mut app, id, |s, at, _| {
        let c = s.composes.remove(at);
        println!("AZMAIL_COMPOSE_CLOSED {}", c.window_id);
    });
    Update::DoNothing
}

extern "C" fn on_compose_action(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, id, action)) = data
        .downcast_ref::<ComposeActionRef>()
        .map(|r| (r.app.clone(), r.id, r.action))
    else {
        return Update::DoNothing;
    };
    run_compose_action(&mut app, &mut info, id, action)
}

/// The editor's model brought up to date with what was typed, then `f` on it; when `f`
/// changed the model, the editor takes it (`reset_editor_content`: the engine's typing gives
/// way to the new content, the caret goes to the end) and the window rebuilds.
fn edit_model(
    s: &mut MailApp,
    at: usize,
    info: &mut CallbackInfo,
    f: impl FnOnce(&mut Dom, &CallbackInfo, azul::dom::DomNodeId) -> bool,
) -> Update {
    let Some(host) = editor::host_node(info) else {
        return Update::DoNothing;
    };
    let body = &mut s.composes[at].body;
    editor::sync_text(body, info, host);
    if f(body, info, host) {
        info.reset_editor_content(host, true);
        Update::RefreshDom
    } else {
        Update::DoNothing
    }
}

fn run_compose_action(app: &mut RefAny, info: &mut CallbackInfo, id: u64, action: ComposeAction) -> Update {
    with_compose(app, id, |s, at, app| match action {
        ComposeAction::Bold | ComposeAction::Italic | ComposeAction::Underline => {
            let (wrapper, format) = match action {
                ComposeAction::Bold => (Dom::create_b(), TextFormat::Bold),
                ComposeAction::Italic => (Dom::create_i(), TextFormat::Italic),
                _ => (Dom::create_u(), TextFormat::Underline),
            };
            let wrapped = edit_model(s, at, info, |body, info, host| {
                editor::wrap_selection(body, info, host, &wrapper)
            });
            if wrapped == Update::DoNothing {
                // No selection: the format applies to what is typed next.
                if let Some(host) = editor::host_node(info) {
                    info.toggle_text_format(host, format);
                }
            }
            wrapped
        }
        ComposeAction::Bullets | ComposeAction::Numbering => {
            let ordered = action == ComposeAction::Numbering;
            edit_model(s, at, info, |body, info, host| {
                editor::toggle_list(body, info, host, ordered)
            })
        }
        ComposeAction::ToggleLink => {
            let c = &mut s.composes[at];
            c.show_link = !c.show_link;
            Update::RefreshDom
        }
        ComposeAction::InsertLink => {
            let href = s.composes[at].link.trim().to_string();
            if href.is_empty() {
                s.composes[at].status =
                    ComposeStatus::Problem(String::from("Type the link's address first."));
                return Update::RefreshDom;
            }
            let href = if href.contains("://") || href.starts_with("mailto:") {
                href
            } else {
                format!("https://{href}")
            };
            let update = edit_model(s, at, info, |body, info, host| {
                editor::insert_link(body, info, host, &href, "")
            });
            let c = &mut s.composes[at];
            c.show_link = false;
            c.link.clear();
            if update == Update::DoNothing {
                Update::RefreshDom
            } else {
                update
            }
        }
        ComposeAction::AttachFile => {
            let _request = FileDialog::open_multiple_files(
                "Attach File",
                OptionString::None,
                OptionFileTypeList::None,
                compose_ref(&app, id),
                on_files_picked as ResumeCallbackType,
            );
            Update::DoNothing
        }
        ComposeAction::RemoveAttachment(i) => {
            let c = &mut s.composes[at];
            if i < c.attachments.len() {
                c.attachments.remove(i);
            }
            Update::RefreshDom
        }
        ComposeAction::Save | ComposeAction::Send => {
            if s.composes[at].busy() {
                return Update::DoNothing;
            }
            if let Some(host) = editor::host_node(info) {
                editor::sync_text(&mut s.composes[at].body, info, host);
            }
            let send = action == ComposeAction::Send;
            let fields = s.composes[at].fields();
            if send {
                // Refused before anything is written: no recipient, a bad address.
                if let Err(e) = compose::outgoing(&fields, Vec::new()) {
                    s.composes[at].status = ComposeStatus::Problem(e.to_string());
                    return Update::RefreshDom;
                }
            }
            let Some(account) = s.accounts.iter().find(|a| a.id == s.composes[at].account_id).cloned()
            else {
                s.composes[at].status =
                    ComposeStatus::Problem(String::from("The account is gone."));
                return Update::RefreshDom;
            };
            let c = &mut s.composes[at];
            c.status = if send {
                ComposeStatus::Sending
            } else {
                ComposeStatus::Saving
            };
            if send {
                println!("AZMAIL_SEND_START {}", c.window_id);
            }
            let job = OutgoingJob {
                compose_id: c.id,
                window_id: c.window_id.clone(),
                root: s.root.clone(),
                account_id: account.id.clone(),
                store_root: crate::account::mail_root(&s.root, &account),
                fields,
                attachments: c.attachments.clone(),
                draft_uid: c.draft_uid,
                send,
            };
            info.add_thread(
                ThreadId::unique(),
                Thread::create(RefAny::new(job), app, outgoing_thread),
            );
            Update::RefreshDom
        }
        ComposeAction::Discard => {
            let c = s.composes.remove(at);
            println!("AZMAIL_COMPOSE_CLOSED {}", c.window_id);
            info.close_window();
            Update::DoNothing
        }
    })
    .unwrap_or(Update::DoNothing)
}

/// Files picked to attach.
extern "C" fn on_files_picked(mut data: RefAny, _info: CallbackInfo, result: RefAny) -> Update {
    let Some((mut app, id)) = target_of(&mut data) else {
        return Update::DoNothing;
    };
    let Some(picked) = FileOpenMultiResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    let files: Vec<AttachedFile> = picked
        .paths
        .as_ref()
        .iter()
        .map(|path| PathBuf::from(path.inner.as_str()))
        .filter_map(|path| {
            let size = std::fs::metadata(&path).ok()?.len();
            let name = path.file_name()?.to_string_lossy().into_owned();
            Some(AttachedFile { path, name, size })
        })
        .collect();
    if files.is_empty() {
        return Update::DoNothing;
    }
    with_compose(&mut app, id, |s, at, _| {
        s.composes[at].attachments.extend(files);
        Update::RefreshDom
    })
    .unwrap_or(Update::DoNothing)
}

// ==== Save and Send: on a Thread of the compose window ====

/// What the thread is given.
#[derive(Clone)]
struct OutgoingJob {
    compose_id: u64,
    window_id: String,
    /// The AzMail folder.
    root: PathBuf,
    account_id: String,
    /// The account's mail folder (`mail/<folder>/...` under it).
    store_root: PathBuf,
    fields: ComposeFields,
    attachments: Vec<AttachedFile>,
    /// The draft saved before (replaced by a save, removed once sent).
    draft_uid: Option<u32>,
    /// Send (else save a draft).
    send: bool,
}

/// What the thread did.
#[derive(Clone)]
enum OutgoingDone {
    Sent(send::SendStatus),
    DraftSaved(u32),
    Problem(String),
}

struct OutgoingMessage {
    compose_id: u64,
    window_id: String,
    done: OutgoingDone,
}

extern "C" fn outgoing_thread(mut init: RefAny, mut sender: ThreadSender, _receiver: ThreadReceiver) {
    let Some(job) = init
        .downcast_ref::<OutgoingJob>()
        .map(|job| OutgoingJob::clone(&job))
    else {
        return;
    };
    let done = run_outgoing(&job);
    sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg {
        refany: RefAny::new(OutgoingMessage {
            compose_id: job.compose_id,
            window_id: job.window_id.clone(),
            done,
        }),
        callback: WriteBackCallback {
            cb: on_outgoing_done,
            ctx: OptionRefAny::None,
        },
    }));
}

/// Reads the attachments, then saves the draft or sends (blocking: DNS, SMTP, files).
fn run_outgoing(job: &OutgoingJob) -> OutgoingDone {
    let mut attachments = Vec::with_capacity(job.attachments.len());
    for file in &job.attachments {
        match std::fs::read(&file.path) {
            Ok(bytes) => attachments.push(send::Attachment {
                file_name: file.name.clone(),
                mime_type: compose::mime_type_for(&file.name).to_string(),
                bytes,
            }),
            Err(e) => {
                return OutgoingDone::Problem(format!("Could not read {}: {e}", file.name));
            }
        }
    }
    let now = crate::now_unix();
    if !job.send {
        let mail = compose::draft_mail(&job.fields, attachments);
        let bytes = compose::draft_bytes(&mail, now);
        return match compose::save_draft(&job.store_root, job.draft_uid, &bytes, now) {
            Ok(entry) => OutgoingDone::DraftSaved(entry.uid),
            Err(e) => OutgoingDone::Problem(format!("The draft could not be saved: {e}")),
        };
    }
    let mail = match compose::outgoing(&job.fields, attachments) {
        Ok(mail) => mail,
        Err(e) => return OutgoingDone::Problem(e.to_string()),
    };
    let settings = send::SendSettings::load(&job.root, &job.account_id);
    let status = send::send_mail(&job.root, &job.account_id, &settings, &mail);
    if let (send::SendStatus::Sent { .. }, Some(uid)) = (&status, job.draft_uid) {
        // Sent: the draft it was is not a draft any more.
        let _ = compose::delete_draft(&LocalFolder::new(job.store_root.clone()), uid);
    }
    OutgoingDone::Sent(status)
}

/// Save or Send is done, on the compose window.
extern "C" fn on_outgoing_done(mut app: RefAny, mut payload: RefAny, mut info: CallbackInfo) -> Update {
    let Some((id, window_id, done)) = payload
        .downcast_ref::<OutgoingMessage>()
        .map(|m| (m.compose_id, m.window_id.clone(), m.done.clone()))
    else {
        return Update::DoNothing;
    };
    with_app(&mut app, |s, _| {
        let at = s.composes.iter().position(|c| c.id == id);
        let update = match done {
            OutgoingDone::Sent(send::SendStatus::Sent { message_id }) => {
                println!("AZMAIL_SEND_DONE {window_id} sent {message_id}");
                if let Some(at) = at {
                    let c = s.composes.remove(at);
                    s.notice = format!("Sent: {}", c.subject);
                    println!("AZMAIL_COMPOSE_CLOSED {}", c.window_id);
                }
                info.close_window();
                Update::RefreshDomAllWindows
            }
            OutgoingDone::Sent(send::SendStatus::Queued { reason }) => {
                println!("AZMAIL_SEND_DONE {window_id} queued {reason}");
                if let Some(at) = at {
                    s.composes[at].status = ComposeStatus::Queued(reason);
                }
                Update::RefreshDomAllWindows
            }
            OutgoingDone::Sent(send::SendStatus::Failed { reason }) => {
                println!("AZMAIL_SEND_DONE {window_id} failed {reason}");
                if let Some(at) = at {
                    s.composes[at].status = ComposeStatus::Failed(reason);
                }
                Update::RefreshDomAllWindows
            }
            OutgoingDone::DraftSaved(uid) => {
                println!("AZMAIL_DRAFT_SAVED {window_id} {uid}");
                if let Some(at) = at {
                    let c = &mut s.composes[at];
                    c.draft_uid = Some(uid);
                    c.status = ComposeStatus::Saved(chrono::Local::now().format("%H:%M").to_string());
                }
                Update::RefreshDomAllWindows
            }
            OutgoingDone::Problem(text) => {
                if let Some(at) = at {
                    s.composes[at].status = ComposeStatus::Problem(text);
                }
                Update::RefreshDom
            }
        };
        // Sent Items, Drafts and the unread counts as the files are now.
        s.reload_folders();
        s.reload_messages();
        update
    })
    .unwrap_or(Update::DoNothing)
}

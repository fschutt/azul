//! The compose window: New, Reply, Reply All, Forward and a draft opened again.
//!
//! Outlook 2010's message window, as a window of its own (`CallbackInfo::create_window`): the
//! title row, a ribbon (Basic Text: Bold, Italic, Underline, Bullets, Numbering; Include: Attach
//! File, Link; Save: Save, Discard), the header block (Send beside From, To, Cc, Bcc, Subject),
//! the attachments, and azul's shared rich-text editor (`RichTextEditor`, the one AzNotes and
//! AzWriter use) on paper. Every compose window shares the
//! app's state; its layout callback finds ITS compose by the key it carries in the callback's
//! context (`LayoutCallback::ctx`, read with `LayoutCallbackInfo::get_ctx`).
//!
//! Save writes the draft into the account's Drafts folder and Send hands the mail to SEND's
//! `send::send_mail`, both on an azul `Thread` of this window; once the mail is sent the window
//! closes and the mail is in Sent Items. Queued (it waits in the Outbox for the next Send /
//! Receive) and Failed keep the window open and say why.
//!
//! Field ids for scripts (`ids.rs`): `#__azmail_compose_to`, `_cc`, `_bcc`, `_subject`, `_send`,
//! `_body` (the editor), `_link`; the window id is `azmail-compose-<n>`.

use std::path::PathBuf;

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, ModalOnCloseCallbackType, ResumeCallbackType,
        RichTextEditorOnChangeCallbackType, StandardDialogOnEventCallbackType,
        TextInputOnTextInputCallbackType, TimerCallbackInfo, TimerCallbackReturn,
    },
    dialog::{FileDialog, FileOpenMultiResult},
    dom::{DomId, FocusTarget, VirtualKeyCode},
    option::{OptionFileTypeList, OptionString},
    prelude::*,
    shells::{DocumentShell, ShellThemeAccent, ShellThemeScope},
    str::String as AzString,
    task::{TimerId, Timer},
    time::{Duration, SystemTimeDiff},
    vec::RichTextSpanVec,
    widgets::{
        ButtonType, MessageBox, MessageBoxKind, Modal, ModalState, OnTextInputReturn, Ribbon,
        RibbonButton, RibbonGroup, RibbonItem, RibbonTab, RichBlockKind, RichFormat,
        RichTextCommand, RichTextDoc, RichTextEditor, RichTextEditorState, StandardDialogEvent,
        StandardDialogEventKind, StatusBar, StatusBarSegment, TextInputState, TextInputValid,
        Titlebar,
    },
    window::WindowDecorations,
};

use crate::{
    compose::{self, ComposeFields, ComposeKind, StartFields},
    ids, message, send,
    store::{DriveFolder, MailStore},
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
    /// The body: the shared rich-text editor's state (its document, its ONE undo history, the
    /// caret), as its `on_change` last handed it over.
    pub(crate) body: RichTextEditorState,
    pub(crate) in_reply_to: Option<String>,
    pub(crate) references: Vec<String>,
    pub(crate) attachments: Vec<AttachedFile>,
    /// The draft this mail was last saved as (its local UID in the Drafts folder).
    pub(crate) draft_uid: Option<u32>,
    pub(crate) status: ComposeStatus,
    /// The Insert Link field is shown, and its text.
    pub(crate) show_link: bool,
    pub(crate) link: String,
    /// The body's selection when the Insert Link field opened (the field takes the focus).
    pub(crate) link_spans: RichTextSpanVec,
    /// Changed since the window opened or the draft was last saved.
    pub(crate) edited: bool,
    /// The window's close was held back: the "Save changes?" bar is shown.
    pub(crate) asking_close: bool,
    /// "Save" on that bar: the window closes once the draft is saved.
    pub(crate) close_after_save: bool,
}

/// A file to attach, read when the mail is saved or sent (on the thread).
#[derive(Debug, Clone)]
pub(crate) struct AttachedFile {
    pub(crate) source: AttachSource,
    pub(crate) name: String,
    pub(crate) size: u64,
}

/// Where an attachment's bytes come from.
#[derive(Debug, Clone)]
pub(crate) enum AttachSource {
    /// A file on this computer, read when the mail is saved or sent.
    File(PathBuf),
    /// A forwarded mail's or a reopened draft's attachment, read when the window opened (a
    /// re-saved draft replaces the file it came from).
    Carried { mime_type: String, bytes: Vec<u8> },
}

/// The attachments a forward or a reopened draft carries on: the open message's, with their
/// bytes (the same file the reading pane shows, read once more).
fn carried_attachments(s: &MailApp, kind: ComposeKind) -> Vec<AttachedFile> {
    if !matches!(kind, ComposeKind::Forward | ComposeKind::Draft) {
        return Vec::new();
    }
    let (Some(open), Some(store)) = (s.open.as_ref(), s.store()) else {
        return Vec::new();
    };
    let Ok(bytes) = store.get(&open.entry.path) else {
        return Vec::new();
    };
    message::attachment_parts(&bytes)
        .into_iter()
        .map(|part| AttachedFile {
            name: part.name,
            size: part.bytes.len() as u64,
            source: AttachSource::Carried {
                mime_type: part.mime_type,
                bytes: part.bytes,
            },
        })
        .collect()
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
            body: self.body.doc.clone(),
            in_reply_to: self.in_reply_to.clone(),
            references: self.references.clone(),
        }
    }

    /// Closing the window asks "Save changes?": the mail was edited since it opened or was last
    /// saved, and it is not on its way out (being sent).
    fn close_asks(&self) -> bool {
        self.edited && !matches!(self.status, ComposeStatus::Sending | ComposeStatus::Queued(_))
    }

    /// Sending, or sent to the Outbox: Send would send it twice.
    fn busy(&self) -> bool {
        matches!(
            self.status,
            ComposeStatus::Sending | ComposeStatus::Saving | ComposeStatus::Queued(_)
        )
    }
}

/// The editor state of a body that is starting: its host is `ids::COMPOSE_BODY` (scripts
/// focus it as `#__azmail_compose_body`; its blocks are `#__azmail_compose_body-<index>`).
fn body_state(doc: RichTextDoc) -> RichTextEditorState {
    let mut state = RichTextEditorState::create(doc);
    state.host_id = ids::COMPOSE_BODY;
    state
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
    let carried = carried_attachments(s, kind);
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
            (fields, compose::reply_quote(view, &header))
        }
        (ComposeKind::Forward, Some(view)) => (
            compose::forward_fields(view),
            compose::forward_quote(view, &header_date(&view.date)),
        ),
        // A draft opens with its HTML part: its formats and links come back with it.
        (ComposeKind::Draft, Some(view)) => (compose::draft_fields(view), compose::draft_body(view)),
        _ => (StartFields::default(), RichTextDoc::create()),
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
        body: body_state(body),
        in_reply_to: start.in_reply_to,
        references: start.references,
        attachments: carried,
        draft_uid,
        status: ComposeStatus::Editing,
        show_link: false,
        link: String::new(),
        link_spans: RichTextSpanVec::from_vec(Vec::new()),
        edited: false,
        asking_close: false,
        close_after_save: false,
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
    let node = callback_info.get_node_id_by_id_attribute(dom, ids::COMPOSE_BODY);
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
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;");
    document.add_child(header_block(c, &app));
    if c.show_link {
        document.add_child(link_bar(c, &app));
    }
    if !c.attachments.is_empty() {
        document.add_child(attachments_row(c, &app));
    }
    document.add_child(editor_dom(c, &app));
    let shell = DocumentShell::create(document)
        .office_shell()
        .with_ribbon(compose_ribbon(c, &app))
        .with_status_bar(status_bar(c));
    let mut column = Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
        .with_child(
            Titlebar::create(compose::window_title(&c.subject))
                .without_border_bottom()
                .dom(),
        )
        .with_child(shell.dom());
    if c.asking_close {
        column.add_child(save_changes_question(c, &app));
    }
    Dom::create_body()
        .with_css(crate::WINDOW_BODY_CSS)
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
    /// The "Save changes?" bar: save the draft, then close.
    CloseSave,
    /// The "Save changes?" bar: close without saving.
    CloseDiscard,
    /// The "Save changes?" bar: keep the window open.
    CloseCancel,
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
    // Pressed: what the caret's text and block are.
    let toggle = |icon: &str, label: &str, action: ComposeAction, on: bool| {
        RibbonItem::SmallButton(button(icon, label, action).with_toggled(on))
    };
    let body = &c.body;
    let message = RibbonTab::create("Message")
        .with_group(
            RibbonGroup::create("Basic Text")
                .with_item(toggle(
                    "format_bold",
                    "Bold",
                    ComposeAction::Bold,
                    body.is_current_format(RichFormat::Bold),
                ))
                .with_item(toggle(
                    "format_italic",
                    "Italic",
                    ComposeAction::Italic,
                    body.is_current_format(RichFormat::Italic),
                ))
                .with_item(toggle(
                    "format_underlined",
                    "Underline",
                    ComposeAction::Underline,
                    body.is_current_format(RichFormat::Underline),
                ))
                .with_item(toggle(
                    "format_list_bulleted",
                    "Bullets",
                    ComposeAction::Bullets,
                    body.is_current_kind(RichBlockKind::Bullet(0)),
                ))
                .with_item(toggle(
                    "format_list_numbered",
                    "Numbering",
                    ComposeAction::Numbering,
                    body.is_current_kind(RichBlockKind::Numbered(0)),
                )),
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

fn field_input(app: &RefAny, id: u64, field: ComposeField, value: &str, dom_id: AzString) -> Dom {
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
        .with_id(ids::COMPOSE_SEND)
        .with_css("width: 72px; min-height: 64px; margin-right: 10px;");
    let fields = Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1;")
        .with_child(row(
            "From",
            Dom::create_span_with_text(c.from.as_str()).with_css("font-size: 13px;"),
        ))
        .with_child(row(
            "To...",
            field_input(app, c.id, ComposeField::To, &c.to, ids::COMPOSE_TO),
        ))
        .with_child(row(
            "Cc...",
            field_input(app, c.id, ComposeField::Cc, &c.cc, ids::COMPOSE_CC),
        ))
        .with_child(row(
            "Bcc...",
            field_input(app, c.id, ComposeField::Bcc, &c.bcc, ids::COMPOSE_BCC),
        ))
        .with_child(row(
            "Subject:",
            field_input(app, c.id, ComposeField::Subject, &c.subject, ids::COMPOSE_SUBJECT),
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
        .with_child(field_input(app, c.id, ComposeField::Link, &c.link, ids::COMPOSE_LINK))
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

/// "Do you want to save changes to this message?" when an edited message's window is closed:
/// the standard question (a `MessageBox` in a `Modal`, as Outlook asks in a dialog).
fn save_changes_question(c: &Compose, app: &RefAny) -> Dom {
    let question = MessageBox::create(
        MessageBoxKind::Question,
        "Do you want to save changes to this message?",
        "A saved message is kept in Drafts.",
    )
    .with_buttons(
        vec![
            AzString::from("Save"),
            AzString::from("Don't Save"),
            AzString::from("Cancel"),
        ],
        0,
    )
    .with_on_event(
        compose_ref(app, c.id),
        on_save_changes_answer as StandardDialogOnEventCallbackType,
    );
    Modal::create(question.dom())
        .with_title("AzMail")
        .with_open(true)
        .with_on_close(
            compose_ref(app, c.id),
            on_save_changes_dismissed as ModalOnCloseCallbackType,
        )
        .dom()
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

/// The editor: the shared rich-text editor on white paper (a mail body, whatever the app's
/// mode); every change comes back through `on_compose_body_change`.
fn editor_dom(c: &Compose, app: &RefAny) -> Dom {
    let editor = RichTextEditor::create(c.body.clone())
        .with_id(ids::COMPOSE_BODY)
        .with_accessibility_name("Message body")
        .with_paragraph_spacing(0.0)
        .with_on_change(
            compose_ref(app, c.id),
            on_compose_body_change as RichTextEditorOnChangeCallbackType,
        );
    let paper = Dom::create_div()
        .with_css(
            "display: flex; flex-direction: column; flex-grow: 1; min-height: 160px; \
             background: #ffffff; color: #1a1a1a; font-family: sans-serif; overflow-y: auto;",
        )
        .with_child(editor.content_dom());
    Dom::create_div()
        .with_css(
            "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; padding: 0px \
             14px 14px 14px;",
        )
        .with_child(paper)
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
            c.edited |= field != ComposeField::Link;
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

/// The editor's new state (typing, Enter, a paste, a format, an undo): the compose keeps it.
extern "C" fn on_compose_body_change(
    mut data: RefAny,
    _info: CallbackInfo,
    state: RichTextEditorState,
) -> Update {
    let Some((mut app, id)) = target_of(&mut data) else {
        return Update::DoNothing;
    };
    let _ = with_compose(&mut app, id, |s, at, _| {
        let c = &mut s.composes[at];
        c.edited |= c.body.doc != state.doc;
        c.body = state;
    });
    Update::DoNothing
}

/// Ctrl/Cmd+Enter sends, Ctrl/Cmd+S saves the draft (Ctrl/Cmd+B / I / U - at a caret and over
/// a selection - are the editor's own).
extern "C" fn on_compose_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, id)) = target_of(&mut data) else {
        return Update::DoNothing;
    };
    let keyboard = info.get_current_keyboard_state();
    let modifiers = info.get_key_modifiers();
    if !modifiers.primary_down() {
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

/// The window is being closed (its close button, Alt+F4, `close_window`): an edited message
/// holds the close back (`prevent_window_close`) and asks "Save changes?"; otherwise its
/// compose goes. The decision reads the compose as it is NOW, so a close the app asks for
/// itself after dropping or sending the mail (Discard, "Don't Save", sent) always passes.
extern "C" fn on_compose_close_requested(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, id)) = target_of(&mut data) else {
        return Update::DoNothing;
    };
    let held = with_compose(&mut app, id, |s, at, _| {
        if s.composes[at].close_asks() {
            s.composes[at].asking_close = true;
            println!("AZMAIL_COMPOSE_ASK_SAVE {}", s.composes[at].window_id);
            return true;
        }
        let c = s.composes.remove(at);
        println!("AZMAIL_COMPOSE_CLOSED {}", c.window_id);
        false
    })
    .unwrap_or(false);
    if !held {
        return Update::DoNothing;
    }
    // Last: the veto rides on whatever window state this callback queued.
    info.prevent_window_close();
    Update::RefreshDom
}

/// The answer to "Save changes?": Save (button 0) saves the draft and then closes, Don't Save
/// (1) closes, Cancel (2, Escape) keeps the window.
extern "C" fn on_save_changes_answer(
    mut data: RefAny,
    mut info: CallbackInfo,
    event: StandardDialogEvent,
) -> Update {
    let Some((mut app, id)) = target_of(&mut data) else {
        return Update::DoNothing;
    };
    let action = match (event.kind, event.index) {
        (StandardDialogEventKind::Button, 0) => ComposeAction::CloseSave,
        (StandardDialogEventKind::Button, 1) => ComposeAction::CloseDiscard,
        (StandardDialogEventKind::Button | StandardDialogEventKind::Cancel, _) => {
            ComposeAction::CloseCancel
        }
        _ => return Update::DoNothing,
    };
    run_compose_action(&mut app, &mut info, id, action)
}

/// The question's modal was closed (its close button, Escape): Cancel.
extern "C" fn on_save_changes_dismissed(mut data: RefAny, mut info: CallbackInfo, _state: ModalState) -> Update {
    let Some((mut app, id)) = target_of(&mut data) else {
        return Update::DoNothing;
    };
    run_compose_action(&mut app, &mut info, id, ComposeAction::CloseCancel)
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

/// Runs `command` on the body (after what was typed is folded in); a change marks the message
/// edited.
fn edit_body(s: &mut MailApp, at: usize, info: &mut CallbackInfo, command: RichTextCommand) -> Update {
    let c = &mut s.composes[at];
    let before = c.body.revision;
    let update = c.body.apply_command(*info, command);
    c.edited |= c.body.revision != before;
    c.body.focus(*info);
    update
}

fn run_compose_action(app: &mut RefAny, info: &mut CallbackInfo, id: u64, action: ComposeAction) -> Update {
    with_compose(app, id, |s, at, app| {
        if matches!(action, ComposeAction::RemoveAttachment(_)) {
            s.composes[at].edited = true;
        }
        match action {
            ComposeAction::Bold | ComposeAction::Italic | ComposeAction::Underline => {
                // Over a selection: set, or off again when it is all set (Bold twice is plain
                // again); at a caret: what is typed next.
                let format = match action {
                    ComposeAction::Bold => RichFormat::Bold,
                    ComposeAction::Italic => RichFormat::Italic,
                    _ => RichFormat::Underline,
                };
                edit_body(s, at, info, RichTextCommand::ToggleFormat(format))
            }
            ComposeAction::Bullets | ComposeAction::Numbering => {
                let kind = if action == ComposeAction::Numbering {
                    RichBlockKind::Numbered(0)
                } else {
                    RichBlockKind::Bullet(0)
                };
                edit_body(s, at, info, RichTextCommand::ToggleKind(kind))
            }
            ComposeAction::ToggleLink => {
                let c = &mut s.composes[at];
                c.show_link = !c.show_link;
                // The address field takes the focus: the selection to link is kept now.
                if c.show_link {
                    let _ = c.body.sync(*info);
                    c.link_spans = c.body.get_selection(*info);
                }
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
                let c = &mut s.composes[at];
                let spans = core::mem::replace(&mut c.link_spans, RichTextSpanVec::from_vec(Vec::new()));
                let before = c.body.revision;
                let _ = c.body.set_link_on(*info, spans, href);
                c.edited |= c.body.revision != before;
                c.show_link = false;
                c.link.clear();
                c.body.focus(*info);
                Update::RefreshDom
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
            ComposeAction::CloseCancel => {
                s.composes[at].asking_close = false;
                Update::RefreshDom
            }
            ComposeAction::Save | ComposeAction::Send | ComposeAction::CloseSave => {
                if s.composes[at].busy() {
                    return Update::DoNothing;
                }
                if action == ComposeAction::CloseSave {
                    let c = &mut s.composes[at];
                    c.asking_close = false;
                    c.close_after_save = true;
                }
                if s.composes[at].body.sync(*info) {
                    s.composes[at].edited = true;
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
                // The DKIM key, when Send / Receive read it from the keyring (or it was made in
                // this run); without it a signing account's mail waits in the Outbox.
                let dkim_key = s.dkim_keys.get(&account.id).cloned().flatten();
                // The account's password or token, for an account that sends through its
                // provider's server (submission); without it such mail waits in the Outbox.
                let sign_in = s.secrets.get(&account.id).cloned().or_else(crate::test_secret);
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
                    dkim_key,
                    sign_in,
                };
                info.add_thread(
                    ThreadId::unique(),
                    Thread::create(RefAny::new(job), app, outgoing_thread),
                );
                Update::RefreshDom
            }
            ComposeAction::Discard | ComposeAction::CloseDiscard => {
                let c = s.composes.remove(at);
                println!("AZMAIL_COMPOSE_CLOSED {}", c.window_id);
                info.close_window();
                Update::DoNothing
            }
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
            Some(AttachedFile {
                source: AttachSource::File(path),
                name,
                size,
            })
        })
        .collect();
    if files.is_empty() {
        return Update::DoNothing;
    }
    with_compose(&mut app, id, |s, at, _| {
        s.composes[at].edited = true;
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
    root: DriveFolder,
    account_id: String,
    /// The account's mail folder (`mail/<folder>/...` under it).
    store_root: DriveFolder,
    fields: ComposeFields,
    attachments: Vec<AttachedFile>,
    /// The draft saved before (replaced by a save, removed once sent).
    draft_uid: Option<u32>,
    /// Send (else save a draft).
    send: bool,
    /// The DKIM private key for an account that signs (from `MailApp::dkim_keys`).
    dkim_key: Option<crate::account::Secret>,
    /// The sign-in secret for submission (from `MailApp::secrets`).
    sign_in: Option<crate::account::Secret>,
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
        match &file.source {
            // A file the user picked outside the data tree: the kit's one way to read one
            // (a drive at its folder that keeps no manifest).
            AttachSource::File(path) => match azul_appkit::files::read_outside(path) {
                Ok(bytes) => attachments.push(send::Attachment {
                    file_name: file.name.clone(),
                    mime_type: compose::mime_type_for(&file.name).to_string(),
                    bytes,
                }),
                Err(e) => {
                    return OutgoingDone::Problem(format!("Could not read {}: {e}", file.name));
                }
            },
            AttachSource::Carried { mime_type, bytes } => attachments.push(send::Attachment {
                file_name: file.name.clone(),
                mime_type: mime_type.clone(),
                bytes: bytes.clone(),
            }),
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
    let mut settings = send::SendSettings::load(&job.root, &job.account_id);
    settings.dkim_key = job.dkim_key.clone();
    settings.sign_in = job.sign_in.clone();
    let status = send::send_mail(&job.root, &job.account_id, &settings, &mail);
    if let (send::SendStatus::Sent { .. }, Some(uid)) = (&status, job.draft_uid) {
        // Sent: the draft it was is not a draft any more.
        let _ = compose::delete_draft(&MailStore::new(job.store_root.clone()), uid);
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
                    c.edited = false;
                    c.status = ComposeStatus::Saved(chrono::Local::now().format("%H:%M").to_string());
                    if c.close_after_save {
                        // "Save" on the "Save changes?" bar: saved, so the window goes.
                        let c = s.composes.remove(at);
                        println!("AZMAIL_COMPOSE_CLOSED {}", c.window_id);
                        info.close_window();
                    }
                }
                Update::RefreshDomAllWindows
            }
            OutgoingDone::Problem(text) => {
                if let Some(at) = at {
                    let c = &mut s.composes[at];
                    c.status = ComposeStatus::Problem(text);
                    c.close_after_save = false;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn compose() -> Compose {
        Compose {
            id: 1,
            window_id: String::from("azmail-compose-1"),
            kind: ComposeKind::Reply,
            account_id: String::from("ada"),
            from: String::from("Ada <ada@example.org>"),
            to: String::from("ben@example.org"),
            cc: String::new(),
            bcc: String::new(),
            subject: String::from("Re: Garden"),
            body: body_state(RichTextDoc::create()),
            in_reply_to: None,
            references: Vec::new(),
            attachments: Vec::new(),
            draft_uid: None,
            status: ComposeStatus::Editing,
            show_link: false,
            link: String::new(),
            link_spans: RichTextSpanVec::from_vec(Vec::new()),
            edited: false,
            asking_close: false,
            close_after_save: false,
        }
    }

    #[test]
    fn an_edited_message_asks_before_its_window_closes_a_saved_or_sending_one_does_not() {
        let mut c = compose();
        assert!(!c.close_asks(), "nothing changed since it opened");
        c.edited = true;
        assert!(c.close_asks(), "a typed line would be lost");
        c.status = ComposeStatus::Saved(String::from("10:42"));
        assert!(c.close_asks(), "edited again after the save");
        c.edited = false;
        assert!(!c.close_asks(), "saved as it is");
        c.edited = true;
        c.status = ComposeStatus::Sending;
        assert!(!c.close_asks(), "on its way out");
        c.status = ComposeStatus::Queued(String::from("no route"));
        assert!(!c.close_asks(), "in the Outbox");
        c.status = ComposeStatus::Failed(String::from("refused"));
        assert!(c.close_asks(), "not sent: still the only copy");
    }
}

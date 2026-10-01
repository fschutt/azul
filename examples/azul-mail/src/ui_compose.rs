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
        (ComposeKind::Draft, Some(view)) => (
            StartFields {
                to: view.to.clone(),
                cc: view.cc.clone(),
                subject: view.subject.clone(),
                in_reply_to: None,
                references: view.references.clone(),
            },
            MailDoc::from_plain(&view.text, 0),
        ),
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
        bcc: String::new(),
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

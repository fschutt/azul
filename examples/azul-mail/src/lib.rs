//! AzMail: sign in to an IMAP account and sync its mail to files.
//!
//! The first run shows "Add your mail account": the address fills in the servers of known
//! providers (Gmail, Outlook / Office 365, iCloud, Fastmail; any field can be overridden), and
//! the password - an app password for Gmail and iCloud - or an OAuth access token goes to the OS
//! keyring, never to a file. "Save and sync" writes `<AzMail folder>/<account id>/account.json`
//! and syncs every folder on an azul `Thread` (`sync.rs`, `imap_client.rs`):
//!
//! ```text
//! <mail folder>/mail/<folder>/<yyyy>/<mm>/<uid>.eml    the exact bytes
//! <mail folder>/mail/<folder>/index.jsonl              one line per message
//! <mail folder>/mail/<folder>/state.json               UIDVALIDITY and the last UID
//! ```
//!
//! Spam (`\Junk`) is `mail/spam`. The window lists the folders and their messages from those
//! files, and shows a message as plain text (quoted lines indented and coloured by level) or as
//! its HTML part, sanitized and without remote images (`html.rs`).
//!
//! Environment:
//! - `AZMAIL_DATA`: the AzMail folder (default: `AzMail` in the user's data folder).
//! - `AZMAIL_TEST_PASSWORD`, `AZMAIL_TEST_CA`: with `AZ_BACKEND=headless` only, the password to
//!   sign in with and a PEM certificate to trust (a test server's). A headless run never
//!   touches the real keyring either: azul serves it from memory.
//!
//! On stdout, for scripts: `AZMAIL_ACCOUNT_SAVED <file>`, `AZMAIL_SYNC_START <mail folder>`,
//! `AZMAIL_SYNC_DONE fetched=<n> reused=<n> folders=<n>`, `AZMAIL_SYNC_FAILED <why>` and
//! `AZMAIL_KEYRING <outcome>`. The secret is never printed.

pub mod account;
pub mod args;
pub mod auth;
pub mod compose;
pub mod editor;
pub mod folders;
pub mod html;
pub mod imap_client;
pub mod listing;
pub mod message;
pub mod mutf7;
pub mod send;
pub mod sending;
pub mod store;
pub mod sync;

#[cfg(test)]
mod testutil;

use std::{collections::HashMap, path::PathBuf};

use account::{Account, AccountForm, Secret};
use azul::{
    window::WindowDecorations,
    error::KeyringResult,
    file::FilePath,
    option::{OptionKeyringResult, OptionThreadSendMsg},
    prelude::*,
    str::String as AzString,
    widgets::{
        ButtonType, CheckBoxState, OnTextInputReturn, TextInputState, TextInputValid, Titlebar,
    },
};
use folders::Role;
use message::MessageView;
use store::{FolderState, IndexEntry, LocalFolder};
use sync::{Progress, SyncError, SyncOptions, SyncReport};

/// Message rows shown at first, and added by "Show more".
const PAGE: usize = 300;
/// Plain-text lines shown at most.
const MAX_LINES: usize = 3000;

const BODY: &str = "display: flex; flex-direction: column; height: 100%; margin: 0; font-family: \
                    sans-serif; font-size: 14px; color: #1d2330; background: #f4f5f8;";
const TOOLBAR: &str = "display: flex; flex-direction: row; align-items: center; padding: 8px \
                       16px; background: #ffffff; border-bottom: 1px solid #d9dce3;";
const NOTICE: &str = "padding: 6px 16px; font-size: 13px; color: #2c4a7a; background: #e6eefc;";
const FOOTER: &str = "padding: 4px 16px; font-size: 12px; color: #6b7385; background: #ffffff; \
                      border-top: 1px solid #d9dce3;";
const NOTE: &str = "font-size: 12px; color: #6b7385; margin-top: 4px;";
const LABEL: &str = "font-size: 12px; color: #4a5468; margin-top: 12px; margin-bottom: 4px;";
const ERROR: &str = "font-size: 13px; color: #b3261e; margin-top: 12px;";
/// Quote bar colours by level (1, 2, 3, then again).
const QUOTE_COLOURS: [&str; 4] = ["#2f6db0", "#2e7d32", "#8e24aa", "#b36b00"];

// ==== State ====

/// The app.
struct MailApp {
    /// The AzMail folder (`AZMAIL_DATA`).
    root: PathBuf,
    accounts: Vec<Account>,
    /// The account shown, an index into `accounts`.
    current: Option<usize>,
    /// Secrets in memory for this run, by account id: typed in the form, read from the
    /// keyring, or the headless test password.
    secrets: HashMap<String, Secret>,
    form: Option<SetupForm>,
    folders: Vec<FolderRow>,
    /// The folder shown (its key).
    folder: Option<String>,
    /// The folder's index, newest first.
    messages: Vec<IndexEntry>,
    /// Rows shown.
    shown: usize,
    open: Option<OpenMessage>,
    sync: SyncState,
    /// The keyring operation whose answer is awaited (one at a time: azul keeps only the last
    /// answer).
    keyring: Option<KeyringOp>,
    notice: String,
}

/// The "Add your mail account" / "Account settings" form.
struct SetupForm {
    form: AccountForm,
    secret: Secret,
    error: String,
    /// Settings of an existing account: an empty secret keeps the saved one.
    editing: bool,
    /// The defaults the form was drawn with (so a new address redraws the placeholders).
    drawn: account::FormDefaults,
}

impl SetupForm {
    fn create(form: AccountForm, editing: bool) -> SetupForm {
        let drawn = form.defaults();
        SetupForm {
            form,
            secret: Secret::new(String::new()),
            error: String::new(),
            editing,
            drawn,
        }
    }
}

#[derive(Clone)]
struct FolderRow {
    key: String,
    display: String,
    role: Role,
    messages: u64,
}

/// The message shown.
struct OpenMessage {
    entry: IndexEntry,
    view: Option<MessageView>,
    error: String,
    /// "HTML" is on; the sanitized part, made when it was first asked for.
    html: bool,
    sanitized: Option<html::Sanitized>,
}

enum SyncState {
    Idle,
    Running {
        thread: ThreadId,
        account: String,
        status: String,
        percent: f32,
    },
    Done(String),
    Failed(String),
}

enum KeyringOp {
    Store,
    /// A secret read to sync this account.
    Get {
        account: String,
    },
}

impl MailApp {
    fn current_account(&self) -> Option<&Account> {
        self.current.and_then(|i| self.accounts.get(i))
    }

    /// The current account's synced files.
    fn store(&self) -> Option<LocalFolder> {
        self.current_account()
            .map(|a| LocalFolder::new(account::mail_root(&self.root, a)))
    }

    /// Reads the synced folders of the current account (from their state files).
    fn reload_folders(&mut self) {
        let Some(store) = self.store() else {
            self.folders.clear();
            self.folder = None;
            return;
        };
        let mut rows: Vec<FolderRow> = store
            .folders()
            .into_iter()
            .map(|key| {
                let state = store
                    .get(&store::state_key(&key))
                    .ok()
                    .and_then(|bytes| String::from_utf8(bytes).ok())
                    .and_then(|text| FolderState::from_json(&text));
                FolderRow {
                    display: state
                        .as_ref()
                        .map(|s| s.display.clone())
                        .filter(|d| !d.is_empty())
                        .unwrap_or_else(|| key.clone()),
                    messages: state.map_or(0, |s| s.messages),
                    role: Role::of_key(&key),
                    key,
                }
            })
            .collect();
        rows.sort_by(|a, b| (a.role, &a.display).cmp(&(b.role, &b.display)));
        self.folders = rows;
        let still_there = self
            .folder
            .as_ref()
            .is_some_and(|f| self.folders.iter().any(|row| &row.key == f));
        if !still_there {
            self.folder = self.folders.first().map(|row| row.key.clone());
            self.shown = PAGE;
        }
    }

    /// Reads the shown folder's index.
    fn reload_messages(&mut self) {
        self.messages = match (self.store(), self.folder.as_ref()) {
            (Some(store), Some(folder)) => {
                let mut list = store
                    .get(&store::index_key(folder))
                    .map(|bytes| store::index_from_jsonl(&String::from_utf8_lossy(&bytes)))
                    .unwrap_or_default();
                list.sort_by(|a, b| (&b.date, b.uid).cmp(&(&a.date, a.uid)));
                list
            }
            _ => Vec::new(),
        };
        let open_uid = self.open.as_ref().map(|o| o.entry.uid);
        if open_uid.is_some_and(|uid| !self.messages.iter().any(|e| e.uid == uid)) {
            self.open = None;
        }
    }

    /// Shows account `index`.
    fn show_account(&mut self, index: usize) {
        self.current = Some(index);
        self.folder = None;
        self.open = None;
        self.reload_folders();
        self.reload_messages();
    }

    /// Opens message `uid` of the shown folder.
    fn open_message(&mut self, uid: u32) {
        let Some(entry) = self.messages.iter().find(|e| e.uid == uid).cloned() else {
            return;
        };
        let bytes = match self.store() {
            Some(store) => store.get(&entry.path).map_err(|e| e.to_string()),
            None => Err(String::from("no account")),
        };
        let (view, error) = match bytes {
            Ok(bytes) => match message::parse_view(&bytes) {
                Some(view) => (Some(view), String::new()),
                None => (None, String::from("This file is not a mail message.")),
            },
            Err(e) => (None, format!("Could not read {}: {e}", entry.path)),
        };
        self.open = Some(OpenMessage {
            entry,
            view,
            error,
            html: false,
            sanitized: None,
        });
    }
}

/// The headless test password, if this is a headless run that has one.
fn test_secret() -> Option<Secret> {
    account::test_secret(
        std::env::var("AZ_BACKEND").ok().as_deref(),
        std::env::var(account::TEST_PASSWORD_VAR).ok().as_deref(),
    )
}

/// The headless test server's certificate, if this is a headless run that has one.
fn test_ca() -> Option<PathBuf> {
    (std::env::var("AZ_BACKEND").as_deref() == Ok("headless"))
        .then(|| std::env::var(account::TEST_CA_VAR).ok())
        .flatten()
        .filter(|p| !p.trim().is_empty())
        .map(PathBuf::from)
}

// ==== Layout ====

extern "C" fn layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let Some(view) = data.downcast_ref::<MailApp>().map(|s| View::of(&s)) else {
        return Dom::create_body();
    };
    let mut body = Dom::create_body()
        .with_css(BODY)
        .with_callback(
            EventFilter::Window(WindowEventFilter::KeyringResult),
            data.clone(),
            on_keyring_result,
        )
        .with_child(title_row())
        .with_child(toolbar(&view, &data));
    if !view.notice.is_empty() {
        body.add_child(Dom::create_span_with_text(view.notice.as_str()).with_css(NOTICE));
    }
    let main = match &view.form {
        Some(form) => form_panel(form, view.can_cancel, &data),
        None => Dom::create_div()
            .with_css("display: flex; flex-direction: row; flex-grow: 1; min-height: 0px;")
            .with_child(sidebar(&view, &data))
            .with_child(message_list(&view, &data))
            .with_child(message_pane(&view, &data)),
    };
    body.with_child(main)
        .with_child(Dom::create_span_with_text(view.footer.as_str()).with_css(FOOTER))
}

/// The window's title row, drawn by azul (the window is `NoTitle`, so macOS draws only the
/// traffic lights): white like the toolbar below it and with no line of its own, so the two
/// read as one bar.
fn title_row() -> Dom {
    Titlebar::create("AzMail")
        .with_background(ColorU::rgb(0xff, 0xff, 0xff))
        .without_border_bottom()
        .dom()
}

/// What the window shows, read from `MailApp` before the DOM is built.
struct View {
    form: Option<FormView>,
    can_cancel: bool,
    has_account: bool,
    accounts: Vec<(usize, String, bool)>,
    folders: Vec<FolderRow>,
    folder: Option<String>,
    rows: Vec<RowView>,
    more: usize,
    open: Option<OpenView>,
    running: bool,
    status: String,
    percent: Option<f32>,
    notice: String,
    footer: String,
}

struct FormView {
    form: AccountForm,
    secret: String,
    defaults: account::FormDefaults,
    default_folder: String,
    error: String,
    editing: bool,
}

struct RowView {
    uid: u32,
    from: String,
    subject: String,
    date: String,
    unread: bool,
    selected: bool,
}

struct OpenView {
    subject: String,
    lines: Vec<(String, String)>,
    attachments: Vec<String>,
    text: String,
    has_html: bool,
    html: Option<html::Sanitized>,
    error: String,
}

impl View {
    fn of(s: &MailApp) -> View {
        let open_uid = s.open.as_ref().map(|o| o.entry.uid);
        let rows = s
            .messages
            .iter()
            .take(s.shown)
            .map(|e| RowView {
                uid: e.uid,
                from: if e.from.is_empty() {
                    String::from("(no sender)")
                } else {
                    e.from.clone()
                },
                subject: if e.subject.is_empty() {
                    String::from("(no subject)")
                } else {
                    e.subject.clone()
                },
                date: message::short_date_in(&e.date, &chrono::Local),
                unread: !e.flags.iter().any(|f| f.eq_ignore_ascii_case("\\Seen")),
                selected: Some(e.uid) == open_uid,
            })
            .collect();
        let (running, status, percent) = match &s.sync {
            SyncState::Idle => (false, String::new(), None),
            SyncState::Running {
                status, percent, ..
            } => (true, status.clone(), Some(*percent)),
            SyncState::Done(text) | SyncState::Failed(text) => (false, text.clone(), None),
        };
        let mail_root = s
            .current_account()
            .map(|a| account::mail_root(&s.root, a).display().to_string());
        View {
            form: s.form.as_ref().map(|f| {
                let defaults = f.form.defaults();
                FormView {
                    default_folder: account::account_id(&f.form.email)
                        .map(|id| account::account_dir(&s.root, &id).display().to_string())
                        .unwrap_or_default(),
                    form: f.form.clone(),
                    secret: f.secret.expose().to_string(),
                    defaults,
                    error: f.error.clone(),
                    editing: f.editing,
                }
            }),
            can_cancel: !s.accounts.is_empty(),
            has_account: s.current_account().is_some(),
            accounts: s
                .accounts
                .iter()
                .enumerate()
                .map(|(i, a)| (i, a.email.clone(), Some(i) == s.current))
                .collect(),
            folders: s.folders.clone(),
            folder: s.folder.clone(),
            rows,
            more: s.messages.len().saturating_sub(s.shown),
            open: s.open.as_ref().map(|o| {
                let v = o.view.clone().unwrap_or_default();
                let mut lines = vec![(String::from("From"), v.from.clone())];
                if !v.to.is_empty() {
                    lines.push((String::from("To"), v.to.clone()));
                }
                if !v.cc.is_empty() {
                    lines.push((String::from("Cc"), v.cc.clone()));
                }
                let date = if v.date.is_empty() {
                    &o.entry.date
                } else {
                    &v.date
                };
                lines.push((
                    String::from("Date"),
                    message::short_date_in(date, &chrono::Local),
                ));
                OpenView {
                    subject: if v.subject.is_empty() {
                        String::from("(no subject)")
                    } else {
                        v.subject.clone()
                    },
                    lines,
                    attachments: v
                        .attachments
                        .iter()
                        .map(|a| format!("{} ({} bytes)", a.name, a.size))
                        .collect(),
                    text: v.text.clone(),
                    has_html: v.html.is_some(),
                    html: if o.html { o.sanitized.clone() } else { None },
                    error: o.error.clone(),
                }
            }),
            running,
            status,
            percent,
            notice: s.notice.clone(),
            footer: match mail_root {
                Some(root) => format!(
                    "Mail folder: {root} (mail/<folder>/<yyyy>/<mm>/<uid>.eml, index.jsonl, \
                     state.json)"
                ),
                None => format!("AzMail folder: {}", s.root.display()),
            },
        }
    }
}

fn toolbar(view: &View, data: &RefAny) -> Dom {
    let mut bar = Dom::create_div().with_css(TOOLBAR).with_child(
        Dom::create_span_with_text("AzMail")
            .with_css("font-size: 20px; font-weight: bold; margin-right: 18px;"),
    );
    if let Some((_, email, _)) = view.accounts.iter().find(|(_, _, current)| *current) {
        bar.add_child(
            Dom::create_span_with_text(email.as_str())
                .with_css("font-size: 14px; color: #4a5468; margin-right: 12px;"),
        );
    }
    bar.add_child(Dom::create_div().with_css("flex-grow: 1;"));
    if !view.status.is_empty() {
        bar.add_child(
            Dom::create_span_with_text(view.status.as_str())
                .with_css("font-size: 13px; color: #4a5468; margin-right: 10px;"),
        );
    }
    if let Some(percent) = view.percent {
        bar.add_child(
            ProgressBar::create(percent)
                .dom()
                .with_css("width: 140px; margin-right: 10px;"),
        );
    }
    let button = |label: &str, on_click: extern "C" fn(RefAny, CallbackInfo) -> Update| {
        Button::create(label)
            .with_on_click(data.clone(), on_click)
            .dom()
            .with_css("margin-left: 6px;")
    };
    if view.has_account && view.form.is_none() {
        if view.running {
            bar.add_child(button("Stop", on_stop_sync));
        } else {
            bar.add_child(
                Button::with_type("Sync now", ButtonType::Primary)
                    .with_on_click(data.clone(), on_sync_now)
                    .dom()
                    .with_css("margin-left: 6px;"),
            );
        }
        bar.add_child(button("Account settings", on_edit_account));
    }
    if view.form.is_none() {
        bar.add_child(button("Add account", on_add_account));
    }
    bar
}

fn sidebar(view: &View, data: &RefAny) -> Dom {
    let mut side = Dom::create_div().with_css(
        "display: flex; flex-direction: column; width: 220px; flex-shrink: 0; overflow: auto; \
         padding: 8px 0; background: #eef0f4; border-right: 1px solid #d9dce3;",
    );
    for (index, email, current) in &view.accounts {
        let weight = if *current { "bold" } else { "normal" };
        side.add_child(
            Dom::create_span_with_text(email.as_str())
                .with_css(format!(
                    "padding: 6px 12px; font-size: 12px; color: #4a5468; font-weight: {weight}; \
                     cursor: pointer;"
                ))
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::Click),
                    RefAny::new(AccountRef {
                        app: data.clone(),
                        index: *index,
                    }),
                    on_select_account,
                ),
        );
        if !*current {
            continue;
        }
        if view.folders.is_empty() {
            side.add_child(
                Dom::create_span_with_text("Not synced yet: press \"Sync now\".")
                    .with_css("padding: 4px 20px; font-size: 12px; color: #6b7385;"),
            );
        }
        for folder in &view.folders {
            let chosen = view.folder.as_deref() == Some(folder.key.as_str());
            let background = if chosen { "#dbe7ff" } else { "transparent" };
            side.add_child(
                Dom::create_div()
                    .with_css(format!(
                        "display: flex; flex-direction: row; padding: 5px 12px 5px 20px; \
                         background: {background}; cursor: pointer;"
                    ))
                    .with_child(
                        Dom::create_span_with_text(folder.display.as_str())
                            .with_css("flex-grow: 1;"),
                    )
                    .with_child(
                        Dom::create_span_with_text(folder.messages.to_string())
                            .with_css("font-size: 12px; color: #6b7385;"),
                    )
                    .with_callback(
                        EventFilter::Hover(HoverEventFilter::Click),
                        RefAny::new(FolderRef {
                            app: data.clone(),
                            key: folder.key.clone(),
                        }),
                        on_select_folder,
                    ),
            );
        }
    }
    side
}

fn message_list(view: &View, data: &RefAny) -> Dom {
    let mut list = Dom::create_div().with_css(
        "display: flex; flex-direction: column; width: 380px; flex-shrink: 0; overflow: auto; \
         background: #ffffff; border-right: 1px solid #d9dce3;",
    );
    if view.rows.is_empty() {
        list.add_child(
            Dom::create_span_with_text("No messages here.")
                .with_css("padding: 16px; color: #6b7385;"),
        );
    }
    for row in &view.rows {
        let background = if row.selected { "#dbe7ff" } else { "#ffffff" };
        let weight = if row.unread { "bold" } else { "normal" };
        list.add_child(
            Dom::create_div()
                .with_css(format!(
                    "display: flex; flex-direction: column; padding: 7px 12px; background: \
                     {background}; border-bottom: 1px solid #eceef2; cursor: pointer;"
                ))
                .with_child(
                    Dom::create_div()
                        .with_css("display: flex; flex-direction: row;")
                        .with_child(Dom::create_span_with_text(row.from.as_str()).with_css(
                            format!("flex-grow: 1; font-weight: {weight}; overflow: hidden;"),
                        ))
                        .with_child(
                            Dom::create_span_with_text(row.date.as_str())
                                .with_css("font-size: 12px; color: #6b7385; margin-left: 8px;"),
                        ),
                )
                .with_child(
                    Dom::create_span_with_text(row.subject.as_str())
                        .with_css("font-size: 13px; color: #4a5468; overflow: hidden;"),
                )
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::Click),
                    RefAny::new(MessageRef {
                        app: data.clone(),
                        uid: row.uid,
                    }),
                    on_select_message,
                ),
        );
    }
    if view.more > 0 {
        list.add_child(
            Button::create(format!("Show more ({} not shown)", view.more))
                .with_on_click(data.clone(), on_show_more)
                .dom()
                .with_css("margin: 10px;"),
        );
    }
    list
}

fn message_pane(view: &View, data: &RefAny) -> Dom {
    let mut pane = Dom::create_div().with_css(
        "display: flex; flex-direction: column; flex-grow: 1; overflow: auto; background: \
         #ffffff;",
    );
    let Some(open) = &view.open else {
        pane.add_child(
            Dom::create_span_with_text("Select a message.")
                .with_css("padding: 16px; color: #6b7385;"),
        );
        return pane;
    };
    let mut header = Dom::create_div()
        .with_css(
            "display: flex; flex-direction: column; padding: 12px 16px; border-bottom: 1px solid \
             #eceef2;",
        )
        .with_child(
            Dom::create_span_with_text(open.subject.as_str())
                .with_css("font-size: 18px; font-weight: bold; margin-bottom: 6px;"),
        );
    for (label, value) in &open.lines {
        header.add_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; font-size: 13px;")
                .with_child(
                    Dom::create_span_with_text(label.as_str())
                        .with_css("width: 48px; color: #6b7385;"),
                )
                .with_child(Dom::create_span_with_text(value.as_str())),
        );
    }
    if !open.attachments.is_empty() {
        header.add_child(
            Dom::create_span_with_text(format!("Attachments: {}", open.attachments.join(", ")))
                .with_css(NOTE),
        );
    }
    let mut modes = Dom::create_div()
        .with_css("display: flex; flex-direction: row; margin-top: 8px;")
        .with_child(
            Button::with_type(
                "Plain text",
                if open.html.is_none() {
                    ButtonType::Primary
                } else {
                    ButtonType::Default
                },
            )
            .with_on_click(data.clone(), on_plain_mode)
            .dom(),
        );
    if open.has_html {
        modes.add_child(
            Button::with_type(
                "HTML",
                if open.html.is_some() {
                    ButtonType::Primary
                } else {
                    ButtonType::Default
                },
            )
            .with_on_click(data.clone(), on_html_mode)
            .dom()
            .with_css("margin-left: 6px;"),
        );
    }
    header.add_child(modes);
    pane.add_child(header);
    if !open.error.is_empty() {
        pane.add_child(
            Dom::create_span_with_text(open.error.as_str())
                .with_css("padding: 16px; color: #b3261e;"),
        );
        return pane;
    }
    match &open.html {
        Some(sanitized) => pane.add_child(html_body(sanitized)),
        None => pane.add_child(plain_body(&open.text)),
    }
    pane
}

/// Plain text: every line a row, quoted lines indented behind a bar in their level's colour.
fn plain_body(text: &str) -> Dom {
    let mut body =
        Dom::create_div().with_css("display: flex; flex-direction: column; padding: 12px 16px;");
    let lines = message::quote_lines(text);
    for line in lines.iter().take(MAX_LINES) {
        let css = if line.level == 0 {
            String::from("white-space: pre-wrap; min-height: 18px;")
        } else {
            let colour = QUOTE_COLOURS[(line.level - 1) % QUOTE_COLOURS.len()];
            format!(
                "white-space: pre-wrap; min-height: 18px; margin-left: {}px; padding-left: 8px; \
                 border-left: 3px solid {colour}; color: {colour};",
                (line.level - 1) * 12
            )
        };
        body.add_child(
            Dom::create_div()
                .with_css(css)
                .with_child(Dom::create_span_with_text(line.text.as_str())),
        );
    }
    if lines.len() > MAX_LINES {
        body.add_child(
            Dom::create_span_with_text(format!("({} more lines)", lines.len() - MAX_LINES))
                .with_css(NOTE),
        );
    }
    body
}

/// The sanitized HTML part through azul's own XML parser.
fn html_body(sanitized: &html::Sanitized) -> Dom {
    let mut body =
        Dom::create_div().with_css("display: flex; flex-direction: column; padding: 12px 16px;");
    body.add_child(
        Dom::create_span_with_text(format!(
            "Remote images are off ({} not loaded).",
            sanitized.blocked_images
        ))
        .with_css("font-size: 12px; color: #6b7385; margin-bottom: 8px;"),
    );
    match Xml::from_str(sanitized.xhtml.as_str()) {
        ResultXmlXmlError::Ok(xml) => body.add_child(Dom::create_from_parsed_xml(xml)),
        ResultXmlXmlError::Err(e) => body.add_child(
            Dom::create_span_with_text(format!("The HTML part could not be shown: {e:?}"))
                .with_css(ERROR),
        ),
    }
    body
}

/// A form field.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Field {
    Email,
    Secret,
    Username,
    ImapHost,
    ImapPort,
    SmtpHost,
    SmtpPort,
    Folder,
}

/// A form check box.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Flag {
    Plain,
    Xoauth2,
}

fn form_panel(form: &FormView, can_cancel: bool, data: &RefAny) -> Dom {
    let label = |text: &str| Dom::create_span_with_text(text).with_css(LABEL);
    let input = |kind: TextInput, field: Field, value: &str, placeholder: &str, id: &str| {
        let mut input = kind
            .with_text(value)
            .with_placeholder(placeholder)
            .with_on_text_input(
                RefAny::new(FieldRef {
                    app: data.clone(),
                    field,
                }),
                on_field,
            );
        if field == Field::Email {
            input = input.with_on_focus_lost(
                RefAny::new(FieldRef {
                    app: data.clone(),
                    field,
                }),
                on_email_done,
            );
        }
        input.dom().with_id(id)
    };
    let check = |checked: bool, flag: Flag, text: &str| {
        Dom::create_div()
            .with_css("display: flex; flex-direction: row; align-items: center; margin-top: 12px;")
            .with_child(
                CheckBox::create(checked)
                    .with_on_toggle(
                        RefAny::new(FlagRef {
                            app: data.clone(),
                            flag,
                        }),
                        on_flag,
                    )
                    .dom(),
            )
            .with_child(
                Dom::create_span_with_text(text)
                    .with_css("margin-left: 8px; cursor: pointer;")
                    .with_callback(
                        EventFilter::Hover(HoverEventFilter::Click),
                        RefAny::new(FlagRef {
                            app: data.clone(),
                            flag,
                        }),
                        on_flag_label,
                    ),
            )
    };
    let pair = |left: Dom, right: Dom| {
        Dom::create_div()
            .with_css("display: flex; flex-direction: row;")
            .with_child(left.with_css("flex-grow: 1;"))
            .with_child(right.with_css("width: 90px; margin-left: 8px;"))
    };
    let f = &form.form;
    let d = &form.defaults;
    let secret_label = if f.xoauth2 {
        "OAuth access token (XOAUTH2)"
    } else {
        "Password or app password"
    };
    let secret_placeholder = if form.editing {
        "Leave empty to keep the saved one"
    } else {
        ""
    };
    let mut panel = Dom::create_div()
        .with_css(
            "display: flex; flex-direction: column; flex-grow: 1; overflow: auto; padding: 20px \
             32px; background: #ffffff;",
        )
        .with_child(
            Dom::create_span_with_text(if form.editing {
                "Account settings"
            } else {
                "Add your mail account"
            })
            .with_css("font-size: 20px; font-weight: bold;"),
        )
        .with_child(
            Dom::create_span_with_text(
                "AzMail signs in over IMAP and copies every folder to files on this computer. \
                 The password stays in the system keyring.",
            )
            .with_css(NOTE),
        )
        .with_child(label("Email address"))
        .with_child(input(
            TextInput::create_email(),
            Field::Email,
            &f.email,
            "name@example.org",
            "acct-email",
        ))
        .with_child(label(secret_label))
        .with_child(input(
            TextInput::create_password(),
            Field::Secret,
            &form.secret,
            secret_placeholder,
            "acct-secret",
        ))
        .with_child(Dom::create_span_with_text(account::APP_PASSWORD_NOTE).with_css(NOTE));
    if !d.note.is_empty() {
        panel.add_child(
            Dom::create_span_with_text(d.note.as_str())
                .with_css("font-size: 13px; color: #2c4a7a; margin-top: 4px;"),
        );
    }
    panel.add_child(check(
        f.xoauth2,
        Flag::Xoauth2,
        "Sign in with an OAuth access token (XOAUTH2) instead of a password",
    ));
    panel.add_child(label("IMAP server and port"));
    panel.add_child(pair(
        input(
            TextInput::create(),
            Field::ImapHost,
            &f.imap_host,
            &d.imap_host,
            "acct-imap-host",
        ),
        input(
            TextInput::create(),
            Field::ImapPort,
            &f.imap_port,
            &d.imap_port,
            "acct-imap-port",
        ),
    ));
    panel.add_child(label("SMTP server and port (for sending, later)"));
    panel.add_child(pair(
        input(
            TextInput::create(),
            Field::SmtpHost,
            &f.smtp_host,
            &d.smtp_host,
            "acct-smtp-host",
        ),
        input(
            TextInput::create(),
            Field::SmtpPort,
            &f.smtp_port,
            &d.smtp_port,
            "acct-smtp-port",
        ),
    ));
    panel.add_child(label("User name"));
    panel.add_child(input(
        TextInput::create(),
        Field::Username,
        &f.username,
        &d.username,
        "acct-username",
    ));
    panel.add_child(label("Local mail folder"));
    panel.add_child(input(
        TextInput::create(),
        Field::Folder,
        &f.folder,
        &form.default_folder,
        "acct-folder",
    ));
    panel.add_child(check(
        f.plain,
        Flag::Plain,
        "Unencrypted connection (only for a test server on this computer)",
    ));
    if !form.error.is_empty() {
        panel.add_child(Dom::create_span_with_text(form.error.as_str()).with_css(ERROR));
    }
    let mut buttons = Dom::create_div().with_css(
        "display: flex; flex-direction: row; justify-content: flex-end; margin-top: 20px;",
    );
    if can_cancel {
        buttons.add_child(
            Button::create("Cancel")
                .with_on_click(data.clone(), on_cancel_form)
                .dom()
                .with_css("margin-right: 8px;"),
        );
    }
    buttons.add_child(
        Button::with_type("Save and sync", ButtonType::Primary)
            .with_on_click(data.clone(), on_save_account)
            .dom(),
    );
    panel.with_child(buttons)
}

// ==== Callback targets ====

struct FieldRef {
    app: RefAny,
    field: Field,
}

struct FlagRef {
    app: RefAny,
    flag: Flag,
}

struct AccountRef {
    app: RefAny,
    index: usize,
}

struct FolderRef {
    app: RefAny,
    key: String,
}

struct MessageRef {
    app: RefAny,
    uid: u32,
}

// ==== The form ====

extern "C" fn on_field(
    mut data: RefAny,
    _info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let target = data
        .downcast_ref::<FieldRef>()
        .map(|r| (r.app.clone(), r.field));
    if let Some((mut app, field)) = target {
        let text = state.get_text().as_str().to_string();
        if let Some(mut s) = app.downcast_mut::<MailApp>() {
            if let Some(form) = s.form.as_mut() {
                let f = &mut form.form;
                match field {
                    Field::Email => f.email = text,
                    Field::Secret => form.secret = Secret::new(text),
                    Field::Username => f.username = text,
                    Field::ImapHost => f.imap_host = text,
                    Field::ImapPort => f.imap_port = text,
                    Field::SmtpHost => f.smtp_host = text,
                    Field::SmtpPort => f.smtp_port = text,
                    Field::Folder => f.folder = text,
                }
            }
        }
    }
    OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    }
}

/// Leaving the address field redraws the form when the address changed what the empty fields
/// stand for (the provider's servers and note).
extern "C" fn on_email_done(
    mut data: RefAny,
    _info: CallbackInfo,
    _state: TextInputState,
) -> Update {
    let Some(mut app) = data.downcast_ref::<FieldRef>().map(|r| r.app.clone()) else {
        return Update::DoNothing;
    };
    let Some(mut s) = app.downcast_mut::<MailApp>() else {
        return Update::DoNothing;
    };
    let Some(form) = s.form.as_mut() else {
        return Update::DoNothing;
    };
    let now = form.form.defaults();
    if now == form.drawn {
        return Update::DoNothing;
    }
    form.drawn = now;
    Update::RefreshDom
}

fn set_flag(data: &mut RefAny, checked: Option<bool>) -> Update {
    let Some((mut app, flag)) = data
        .downcast_ref::<FlagRef>()
        .map(|r| (r.app.clone(), r.flag))
    else {
        return Update::DoNothing;
    };
    let Some(mut s) = app.downcast_mut::<MailApp>() else {
        return Update::DoNothing;
    };
    let Some(form) = s.form.as_mut() else {
        return Update::DoNothing;
    };
    let value = match flag {
        Flag::Plain => &mut form.form.plain,
        Flag::Xoauth2 => &mut form.form.xoauth2,
    };
    *value = checked.unwrap_or(!*value);
    form.error.clear();
    Update::RefreshDom
}

extern "C" fn on_flag(mut data: RefAny, _info: CallbackInfo, state: CheckBoxState) -> Update {
    set_flag(&mut data, Some(state.checked))
}

extern "C" fn on_flag_label(mut data: RefAny, _info: CallbackInfo) -> Update {
    set_flag(&mut data, None)
}

extern "C" fn on_add_account(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut s) = data.downcast_mut::<MailApp>() else {
        return Update::DoNothing;
    };
    s.form = Some(SetupForm::create(AccountForm::default(), false));
    s.notice.clear();
    Update::RefreshDom
}

extern "C" fn on_edit_account(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut s) = data.downcast_mut::<MailApp>() else {
        return Update::DoNothing;
    };
    let Some(form) = s.current_account().map(AccountForm::from_account) else {
        return Update::DoNothing;
    };
    s.form = Some(SetupForm::create(form, true));
    Update::RefreshDom
}

extern "C" fn on_cancel_form(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut s) = data.downcast_mut::<MailApp>() else {
        return Update::DoNothing;
    };
    if !s.accounts.is_empty() {
        s.form = None;
    }
    Update::RefreshDom
}

/// "Save and sync": writes the account file, puts the secret in the keyring, and syncs.
extern "C" fn on_save_account(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let job = {
        let Some(mut guard) = data.downcast_mut::<MailApp>() else {
            return Update::DoNothing;
        };
        let s = &mut *guard;
        let Some(form) = s.form.as_mut() else {
            return Update::DoNothing;
        };
        let account = match form.form.to_account() {
            Ok(account) => account,
            Err(e) => {
                form.error = e.to_string();
                return Update::RefreshDom;
            }
        };
        let typed = (!form.secret.is_empty()).then(|| form.secret.clone());
        let known = s.accounts.iter().any(|a| a.id == account.id);
        let secret = typed.or_else(test_secret);
        if secret.is_none() && !known && !s.secrets.contains_key(&account.id) {
            form.error = if account.auth == account::AuthKind::Xoauth2 {
                String::from("Paste your OAuth access token.")
            } else {
                String::from("Enter your password or app password.")
            };
            return Update::RefreshDom;
        }
        let saved = match account::save(&s.root, &account) {
            Ok(path) => path,
            Err(e) => {
                form.error = format!("Could not write the account file: {e}");
                return Update::RefreshDom;
            }
        };
        println!("AZMAIL_ACCOUNT_SAVED {}", saved.display());
        if let Some(secret) = secret {
            if s.keyring.is_none() {
                info.keyring_store(account::keyring_key(&account.id), secret.expose(), false);
                s.keyring = Some(KeyringOp::Store);
            }
            s.secrets.insert(account.id.clone(), secret);
        }
        let index = match s.accounts.iter().position(|a| a.id == account.id) {
            Some(i) => {
                s.accounts[i] = account;
                i
            }
            None => {
                s.accounts.push(account);
                s.accounts.len() - 1
            }
        };
        s.form = None;
        s.notice.clear();
        s.show_account(index);
        prepare_sync(s, &mut info)
    };
    if let Some((id, init)) = job {
        spawn_sync(&mut info, data.clone(), id, init);
    }
    Update::RefreshDom
}

// ==== Browsing ====

extern "C" fn on_select_account(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some((mut app, index)) = data
        .downcast_ref::<AccountRef>()
        .map(|r| (r.app.clone(), r.index))
    else {
        return Update::DoNothing;
    };
    let Some(mut s) = app.downcast_mut::<MailApp>() else {
        return Update::DoNothing;
    };
    if s.current == Some(index) || index >= s.accounts.len() {
        return Update::DoNothing;
    }
    s.show_account(index);
    Update::RefreshDom
}

extern "C" fn on_select_folder(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some((mut app, key)) = data
        .downcast_ref::<FolderRef>()
        .map(|r| (r.app.clone(), r.key.clone()))
    else {
        return Update::DoNothing;
    };
    let Some(mut s) = app.downcast_mut::<MailApp>() else {
        return Update::DoNothing;
    };
    if s.folder.as_deref() == Some(key.as_str()) {
        return Update::DoNothing;
    }
    s.folder = Some(key);
    s.shown = PAGE;
    s.open = None;
    s.reload_messages();
    Update::RefreshDom
}

extern "C" fn on_select_message(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some((mut app, uid)) = data
        .downcast_ref::<MessageRef>()
        .map(|r| (r.app.clone(), r.uid))
    else {
        return Update::DoNothing;
    };
    let Some(mut s) = app.downcast_mut::<MailApp>() else {
        return Update::DoNothing;
    };
    s.open_message(uid);
    Update::RefreshDom
}

extern "C" fn on_show_more(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut s) = data.downcast_mut::<MailApp>() else {
        return Update::DoNothing;
    };
    s.shown = s.shown.saturating_add(PAGE);
    Update::RefreshDom
}

fn set_html(data: &mut RefAny, html: bool) -> Update {
    let Some(mut s) = data.downcast_mut::<MailApp>() else {
        return Update::DoNothing;
    };
    let Some(open) = s.open.as_mut() else {
        return Update::DoNothing;
    };
    if open.html == html {
        return Update::DoNothing;
    }
    open.html = html;
    if html && open.sanitized.is_none() {
        let part = open.view.as_ref().and_then(|v| v.html.as_deref());
        open.sanitized = part.map(html::sanitize);
    }
    Update::RefreshDom
}

extern "C" fn on_plain_mode(mut data: RefAny, _info: CallbackInfo) -> Update {
    set_html(&mut data, false)
}

extern "C" fn on_html_mode(mut data: RefAny, _info: CallbackInfo) -> Update {
    set_html(&mut data, true)
}

// ==== The keyring ====

/// A keyring answer, by name (never with the secret).
fn keyring_outcome(result: &KeyringResult) -> &'static str {
    match result {
        KeyringResult::Stored => "stored",
        KeyringResult::Retrieved(_) => "retrieved",
        KeyringResult::Deleted => "deleted",
        KeyringResult::NotFound => "not found",
        KeyringResult::Denied => "denied",
        KeyringResult::Unavailable => "unavailable",
        KeyringResult::Error => "error",
    }
}

/// The answer to the awaited keyring operation.
extern "C" fn on_keyring_result(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let OptionKeyringResult::Some(result) = info.get_keyring_result() else {
        return Update::DoNothing;
    };
    let outcome = keyring_outcome(&result);
    println!("AZMAIL_KEYRING {outcome}");
    let job = {
        let Some(mut guard) = data.downcast_mut::<MailApp>() else {
            return Update::DoNothing;
        };
        let s = &mut *guard;
        match (s.keyring.take(), result) {
            (None, _) => return Update::DoNothing,
            (Some(KeyringOp::Store), KeyringResult::Stored) => {
                s.notice = String::from("The password is saved in the system keyring.");
                None
            }
            (Some(KeyringOp::Store), _) => {
                s.notice = format!(
                    "The password could not be saved in the system keyring ({outcome}): AzMail \
                     keeps it only until it is closed."
                );
                None
            }
            (Some(KeyringOp::Get { account }), KeyringResult::Retrieved(secret)) => {
                s.secrets
                    .insert(account.clone(), Secret::new(secret.as_str().to_string()));
                s.notice.clear();
                if s.current_account().map(|a| a.id.as_str()) == Some(account.as_str()) {
                    prepare_sync(s, &mut info)
                } else {
                    None
                }
            }
            (Some(KeyringOp::Get { account }), _) => {
                let form = s
                    .accounts
                    .iter()
                    .find(|a| a.id == account)
                    .map(AccountForm::from_account);
                if let Some(form) = form {
                    let mut setup = SetupForm::create(form, true);
                    setup.error = format!(
                        "Enter the password again: the system keyring has none for this account \
                         ({outcome})."
                    );
                    s.form = Some(setup);
                }
                s.sync = SyncState::Idle;
                None
            }
        }
    };
    if let Some((id, init)) = job {
        spawn_sync(&mut info, data.clone(), id, init);
    }
    Update::RefreshDom
}

// ==== Syncing: on an azul Thread, progress back through write-backs ====

extern "C" fn on_sync_now(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let job = {
        let Some(mut guard) = data.downcast_mut::<MailApp>() else {
            return Update::DoNothing;
        };
        prepare_sync(&mut guard, &mut info)
    };
    if let Some((id, init)) = job {
        spawn_sync(&mut info, data.clone(), id, init);
    }
    Update::RefreshDom
}

extern "C" fn on_stop_sync(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut s) = data.downcast_mut::<MailApp>() else {
        return Update::DoNothing;
    };
    if let SyncState::Running { thread, status, .. } = &mut s.sync {
        // The thread sees TerminateThread between batches, writes what it has and stops.
        info.remove_thread(*thread);
        *status = String::from("Stopping...");
    }
    Update::RefreshDom
}

/// What the sync thread starts with.
#[derive(Clone)]
struct SyncInit {
    account: Account,
    secret: Secret,
    mail_root: PathBuf,
    extra_ca: Option<PathBuf>,
}

/// Gets the current account's sync ready: with its secret, the thread's start data (and the
/// state says it runs); without one, a keyring read whose answer starts it
/// (`on_keyring_result`).
fn prepare_sync(s: &mut MailApp, info: &mut CallbackInfo) -> Option<(ThreadId, SyncInit)> {
    if matches!(s.sync, SyncState::Running { .. }) {
        return None;
    }
    let account = s.current_account()?.clone();
    let secret = s.secrets.get(&account.id).cloned().or_else(test_secret);
    let Some(secret) = secret else {
        if s.keyring.is_none() {
            info.keyring_get(account::keyring_key(&account.id));
            s.keyring = Some(KeyringOp::Get {
                account: account.id.clone(),
            });
        }
        s.sync = SyncState::Done(String::from(
            "Reading the password from the system keyring...",
        ));
        return None;
    };
    let mail_root = account::mail_root(&s.root, &account);
    println!("AZMAIL_SYNC_START {}", mail_root.display());
    let thread = ThreadId::unique();
    s.sync = SyncState::Running {
        thread,
        account: account.id.clone(),
        status: format!("Connecting to {}...", account.imap.host),
        percent: 0.0,
    };
    Some((
        thread,
        SyncInit {
            account,
            secret,
            mail_root,
            extra_ca: test_ca(),
        },
    ))
}

fn spawn_sync(info: &mut CallbackInfo, app: RefAny, id: ThreadId, init: SyncInit) {
    info.add_thread(id, Thread::create(RefAny::new(init), app, sync_thread));
}

/// What the thread reports.
#[derive(Clone)]
enum SyncEvent {
    Progress(Progress),
    Finished(Result<SyncReport, SyncError>),
}

struct SyncMessage {
    account: String,
    event: SyncEvent,
}

/// Runs the sync: the IMAP connection blocks here, never in a callback.
extern "C" fn sync_thread(
    mut init: RefAny,
    mut sender: ThreadSender,
    mut receiver: ThreadReceiver,
) {
    let Some(job) = init
        .downcast_ref::<SyncInit>()
        .map(|job| SyncInit::clone(&job))
    else {
        return;
    };
    let outcome = run_sync(&job, &mut sender, &mut receiver);
    post(&mut sender, &job.account.id, SyncEvent::Finished(outcome));
}

fn run_sync(
    job: &SyncInit,
    sender: &mut ThreadSender,
    receiver: &mut ThreadReceiver,
) -> Result<SyncReport, SyncError> {
    let mut source =
        imap_client::ImapSource::connect(&job.account, &job.secret, job.extra_ca.as_deref())?;
    let store = LocalFolder::new(job.mail_root.clone());
    let options = SyncOptions {
        now: now_unix(),
        ..SyncOptions::default()
    };
    let account = job.account.id.clone();
    let mut on_progress = |progress: Progress| -> bool {
        let delivered = post(sender, &account, SyncEvent::Progress(progress));
        let mut stop = false;
        while let OptionThreadSendMsg::Some(message) = receiver.recv() {
            if matches!(message, ThreadSendMsg::TerminateThread) {
                stop = true;
            }
        }
        delivered && !stop
    };
    let result = sync::sync_account(&mut source, &store, &options, &mut on_progress);
    source.logout();
    result
}

fn post(sender: &mut ThreadSender, account: &str, event: SyncEvent) -> bool {
    sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg {
        refany: RefAny::new(SyncMessage {
            account: account.to_string(),
            event,
        }),
        callback: WriteBackCallback {
            cb: on_sync_event,
            ctx: OptionRefAny::None,
        },
    }))
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// A report from the sync thread, on the UI thread.
extern "C" fn on_sync_event(mut app: RefAny, mut payload: RefAny, _info: CallbackInfo) -> Update {
    let Some((account, event)) = payload
        .downcast_ref::<SyncMessage>()
        .map(|m| (m.account.clone(), m.event.clone()))
    else {
        return Update::DoNothing;
    };
    let Some(mut guard) = app.downcast_mut::<MailApp>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    let running_this = matches!(&s.sync, SyncState::Running { account: a, .. } if *a == account);
    match event {
        SyncEvent::Progress(progress) => {
            if let SyncState::Running {
                status, percent, ..
            } = &mut s.sync
            {
                if !running_this {
                    return Update::DoNothing;
                }
                match progress {
                    Progress::Folder {
                        index,
                        count,
                        display,
                    } => {
                        *status = format!("Syncing {display} (folder {} of {count})", index + 1);
                        *percent = 0.0;
                    }
                    Progress::Messages {
                        display,
                        done,
                        total,
                    } => {
                        *status = format!("Syncing {display}: {done} of {total} messages");
                        *percent = if total == 0 {
                            100.0
                        } else {
                            done as f32 * 100.0 / total as f32
                        };
                    }
                }
            }
        }
        SyncEvent::Finished(Ok(report)) => {
            println!(
                "AZMAIL_SYNC_DONE fetched={} reused={} folders={}",
                report.fetched(),
                report.reused(),
                report.folders.len()
            );
            eprintln!(
                "[azmail] synced {} folder(s): {} new message(s)",
                report.folders.len(),
                report.fetched()
            );
            s.sync = SyncState::Done(format!(
                "Synced {} folders: {} new messages.",
                report.folders.len(),
                report.fetched()
            ));
            s.reload_folders();
            s.reload_messages();
        }
        SyncEvent::Finished(Err(e)) => {
            println!("AZMAIL_SYNC_FAILED {e}");
            eprintln!("[azmail] sync failed: {e}");
            if matches!(e, SyncError::Auth(_)) {
                // A wrong password in memory (or the keyring) would fail again: ask for it.
                s.secrets.remove(&account);
                let form = s
                    .accounts
                    .iter()
                    .find(|a| a.id == account)
                    .map(AccountForm::from_account);
                if let Some(form) = form {
                    let mut setup = SetupForm::create(form, true);
                    setup.error = format!("Sign-in failed: {e}");
                    s.form = Some(setup);
                }
            }
            s.sync = SyncState::Failed(format!("Sync failed: {e}"));
            s.reload_folders();
            s.reload_messages();
        }
    }
    Update::RefreshDom
}

// ==== Start ====

fn user_data_dir() -> Option<PathBuf> {
    FilePath::get_data_dir()
        .into_option()
        .map(|dir| PathBuf::from(dir.inner.as_str()))
}

pub fn start() {
    let root = account::data_root(
        std::env::var(account::DATA_VAR).ok().as_deref(),
        user_data_dir(),
    );
    let (accounts, skipped) = account::load_all(&root);
    for (path, reason) in &skipped {
        eprintln!("[azmail] left out {}: {reason}", path.display());
    }
    eprintln!(
        "[azmail] {} account(s) in {}",
        accounts.len(),
        root.display()
    );
    let first_run = accounts.is_empty();
    let mut state = MailApp {
        root,
        accounts,
        current: None,
        secrets: HashMap::new(),
        form: first_run.then(|| SetupForm::create(AccountForm::default(), false)),
        folders: Vec::new(),
        folder: None,
        messages: Vec::new(),
        shown: PAGE,
        open: None,
        sync: SyncState::Idle,
        keyring: None,
        notice: String::new(),
    };
    if !first_run {
        state.show_account(0);
    }
    let app = App::create(RefAny::new(state), AppConfig::create());
    let mut window = WindowCreateOptions::create(layout);
    window.window_state.size.dimensions = LogicalSize::create(1280.0, 860.0);
    window.window_state.title = AzString::from("AzMail");
    window.window_state.flags.decorations = WindowDecorations::NoTitle;
    app.run(window);
}

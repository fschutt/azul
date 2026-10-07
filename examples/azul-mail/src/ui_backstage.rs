//! File: the backstage in the Outlook 2010 look.
//!
//! ```text
//! File | Home | Send / Receive | Folder | View      (the ribbon's tab row stays: File is its
//! ---------+---------------------------------------  first tab, another tab leaves it)
//! Info     | Account Information
//! Print    | (@) ada@example.org  IMAP ... (framed: the account shown)
//! Help     | [+ Add Account]
//!          | [Account Settings]  Account Settings - what it changes
//! Options  | [Send/Receive]      Send/Receive - what it does, how it went
//! Exit     |                     Mailbox - where the files are
//! ```
//!
//! No back button (`hidden`): as in Outlook 2010, File is a tab - Home, Send / Receive, Folder,
//! View, File again or Escape lead back to the mail. The pages are made of azul-appkit's
//! backstage pieces (`azul_appkit::backstage`):
//!
//! - **Info**: every account as a card (a click shows it), Add Account (the wizard), Account
//!   Settings, Send/Receive and the mailbox; without an account New E-mail too (writing needs
//!   none: Local Folders keep it). The wizard and Account Settings are pages under Info (Info
//!   stays lit, as Outlook's dialogs open over File > Info).
//! - **Print**: the open message as a PDF (A4, memo style) made by azul's PDF writer; its first
//!   page comes back as a picture (PDF -> SVG -> pixels, on a Thread) for the preview, and
//!   Print writes the file to `exports/` in the AzMail folder (`AZMAIL_PRINTED <file>`).
//! - **Help**: the keyboard shortcuts, Options, and About AzMail (the facts, the About box).
//! - **Options**: the kit's settings page (Mail, Appearance, Data, Shortcuts, About) in a window
//!   of its own, as Outlook 2010's Options dialog (`ui_options.rs`); File stays behind it.
//! - **Exit** closes the window.

use std::path::PathBuf;

use azul::{
    callbacks::{BackstageOnNavSelectCallbackType, ButtonOnClickCallbackType},
    css::{CssProperty, CssPropertyWithConditions, LayoutDisplay},
    error::ResultParsedSvgSvgParseError,
    image::{ImageRef, RawImage},
    option::{OptionColorU, OptionCssPropertyWithConditionsVec},
    pdf::{ParsedPdf, Pdf},
    prelude::*,
    svg::{ParsedSvg, SvgFitTo, SvgParseOptions, SvgRenderOptions},
    vec::{CssPropertyWithConditionsVec, U8VecRef},
    widgets::{Backstage, BackstageNavItem},
};
use azul_appkit::backstage::{self as pieces, card, command, command_button, facts, note, section};

use crate::{
    ids, message,
    message::MessageView,
    ui_account,
    ui_main::{self, action_ref, on_action, Action},
    with_app, IoJob, MailApp, SyncState,
};

/// The backstage's pages: the navigation column's items first, in its order.
pub(crate) const PAGE_INFO: usize = 0;
pub(crate) const PAGE_PRINT: usize = 1;
pub(crate) const PAGE_HELP: usize = 2;
/// File > Options: the kit's settings page, in a window of its own (`ui_options.rs`).
pub(crate) const PAGE_OPTIONS: usize = 3;
pub(crate) const PAGE_EXIT: usize = 4;
/// File > Info > Add Account: the wizard.
pub(crate) const PAGE_ADD_ACCOUNT: usize = 5;
/// File > Info > Account Settings.
pub(crate) const PAGE_SETTINGS: usize = 6;

/// The navigation column (Outlook 2010: Info, Open, Print, Help, then Options and Exit).
const NAV: [&str; 5] = ["Info", "Print", "Help", "Options", "Exit"];

/// The nav item lit for `page`: Add Account and Account Settings are Info's.
fn nav_item(page: usize) -> usize {
    match page {
        PAGE_ADD_ACCOUNT | PAGE_SETTINGS => PAGE_INFO,
        page => page.min(NAV.len() - 1),
    }
}

/// A part style that draws nothing (`display: none`): the backstage's back button, and the
/// ribbon's band while File is open (only its tab row stays).
pub(crate) fn hidden() -> CssPropertyWithConditionsVec {
    CssPropertyWithConditionsVec::from_item(CssPropertyWithConditions::simple(
        CssProperty::display(LayoutDisplay::None),
    ))
}

// ==== The File tab ====

/// The File tab: the ribbon's tab row (File lit, its band hidden) over the backstage.
pub(crate) fn file_tab(s: &MailApp, app: &RefAny, page: usize) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
        .with_child(ui_main::ribbon(s, app, true))
        .with_child(backstage(s, app, page))
        .with_id(ids::BACKSTAGE)
}

fn backstage(s: &MailApp, app: &RefAny, page: usize) -> Dom {
    let content = match page {
        // The account editor's pages keep the padding they had in the backstage.
        PAGE_ADD_ACCOUNT => editor_page(ui_account::wizard_page(s, app)),
        PAGE_SETTINGS => editor_page(ui_account::settings_page(s, app)),
        PAGE_PRINT => print_page(s, app).with_id(ids::PAGE_PRINT),
        PAGE_HELP => help_page(s, app).with_id(ids::PAGE_HELP),
        _ => info_page(s, app).with_id(ids::PAGE_INFO),
    };
    let items: Vec<BackstageNavItem> = NAV
        .iter()
        .enumerate()
        .map(|(i, label)| {
            let item = BackstageNavItem::create(*label);
            if i == PAGE_OPTIONS {
                item.with_gap_before()
            } else {
                item
            }
        })
        .collect();
    let mut backstage = Backstage::create(items)
        .with_active_item(nav_item(page))
        .with_content(
            Dom::create_div()
                .with_css(
                    "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; \
                     overflow-y: auto;",
                )
                .with_child(content),
        )
        .with_on_nav_select(app.clone(), on_backstage_nav as BackstageOnNavSelectCallbackType);
    // Outlook 2010 has no back button: File is the ribbon's first tab, and the other tabs (File
    // again, Escape) lead back to the mail. No `on_back` either: Escape is the window's
    // (`ui_main::on_main_key`).
    backstage.style.back_button_style = OptionCssPropertyWithConditionsVec::Some(hidden());
    backstage.dom()
}

/// The wizard's / Account Settings' page in the backstage's content.
fn editor_page(content: Dom) -> Dom {
    Dom::create_div()
        .with_css(
            "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; padding: \
             20px 32px;",
        )
        .with_child(content)
}

extern "C" fn on_backstage_nav(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    with_app(&mut data, |s, app| {
        if s.editor.as_ref().is_some_and(|e| e.saving) {
            // The typed secret goes to the keyring when the files are written: stay.
            return Update::DoNothing;
        }
        match index {
            PAGE_OPTIONS => {
                // Outlook 2010's Options dialog: a window of its own over File, which stays.
                crate::ui_options::open(s, &mut info, crate::args::APP_CATEGORIES[0]);
                return Update::RefreshDomAllWindows;
            }
            PAGE_EXIT => {
                info.close_window();
            }
            PAGE_PRINT => {
                s.editor = None;
                s.backstage = Some(PAGE_PRINT);
                prepare_print(s, &mut info, &app);
            }
            page => {
                s.editor = None;
                s.backstage = Some(page.min(PAGE_HELP));
            }
        }
        Update::RefreshDom
    })
    .unwrap_or(Update::DoNothing)
}

// ==== Info ====

/// Which account a card of File > Info shows.
struct AccountRef {
    app: RefAny,
    index: usize,
}

/// File > Info: Account Information.
fn info_page(s: &MailApp, app: &RefAny) -> Dom {
    let mut children = Vec::new();
    if s.accounts.is_empty() {
        children.push(card(
            "person_add",
            "No account yet",
            &["Add an e-mail account to receive and send mail. AzMail keeps a copy of every \
               folder as files on this computer."],
            false,
        ));
    }
    for (i, account) in s.accounts.iter().enumerate() {
        // The synced folders (an Outbox with mail in it is none, and its count is no unread).
        let synced = |f: &&crate::listing::FolderInfo| f.key != crate::listing::OUTBOX_KEY;
        let folders = s.folders.get(i).map_or(0, |list| list.iter().filter(synced).count());
        let unread: usize = s
            .folders
            .get(i)
            .map_or(0, |list| list.iter().filter(synced).map(|f| f.unread).sum());
        let server = format!(
            "IMAP {}:{} - {folders} folders, {unread} unread",
            account.imap.host, account.imap.port
        );
        let settings = crate::send::SendSettings::load(&s.root, &account.id);
        let sending = crate::sending::describe(&settings);
        let shown = Some(i) == s.current;
        let mut account_card = card(
            "account_circle",
            &account.sender(),
            &[server.as_str(), sending.as_str()],
            shown,
        );
        if !shown {
            // Outlook's account drop-down: a click shows that account.
            account_card = account_card.with_css("cursor: pointer;").with_callback(
                EventFilter::Hover(HoverEventFilter::Click),
                RefAny::new(AccountRef {
                    app: app.clone(),
                    index: i,
                }),
                on_account_card,
            );
        }
        children.push(account_card);
    }
    children.push(
        Dom::create_div()
            .with_css("display: flex; flex-direction: row; margin-bottom: 8px;")
            .with_child(command_button(
                "Add Account",
                "person_add",
                ids::ADD_ACCOUNT,
                action_ref(app, Action::AddAccount),
                on_action as ButtonOnClickCallbackType,
            )),
    );
    if s.accounts.is_empty() {
        // Writing needs no account.
        children.push(command(
            command_button(
                "New E-mail",
                "mail",
                ids::INFO_NEW_MAIL,
                action_ref(app, Action::NewMail),
                on_action as ButtonOnClickCallbackType,
            ),
            "New E-mail",
            "Write a message without an account: AzMail sends it from this computer, straight \
             to the recipients' mail servers, and Local Folders keep it (Send/Receive sends \
             what waits in their Outbox).",
        ));
    }
    if !s.accounts.is_empty() {
        children.push(command(
            command_button(
                "Account Settings",
                "manage_accounts",
                ids::ACCOUNT_SETTINGS,
                action_ref(app, Action::AccountSettings),
                on_action as ButtonOnClickCallbackType,
            ),
            "Account Settings",
            "Modify the settings of this account: your name and password, the incoming \
             server, how mail is sent and signed.",
        ));
        let status = match &s.sync {
            SyncState::Running {
                status, percent, ..
            } => format!("Now: {status} ({percent:.0}%)."),
            SyncState::Done(text) | SyncState::Failed(text) => format!("Last time: {text}"),
            SyncState::Idle => String::new(),
        };
        children.push(command(
            command_button(
                "Send/Receive",
                "sync",
                ids::INFO_SEND_RECEIVE,
                action_ref(app, Action::SendReceive),
                on_action as ButtonOnClickCallbackType,
            ),
            "Send/Receive",
            &format!(
                "Receive every folder of this account and send what waits in the Outbox (F9). \
                 {status}"
            ),
        ));
    }
    let kept = if s.accounts.is_empty() {
        "Mail will be kept"
    } else {
        "Mail is kept"
    };
    children.push(command(
        Dom::create_div(),
        "Mailbox",
        &format!(
            "{kept} as plain files, one per message, in {}.",
            s.root.path().display()
        ),
    ));
    pieces::page("Account Information", children)
}

/// A click on an account's card: that account is shown.
extern "C" fn on_account_card(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some((mut app, index)) = data
        .downcast_ref::<AccountRef>()
        .map(|r| (r.app.clone(), r.index))
    else {
        return Update::DoNothing;
    };
    with_app(&mut app, |s, _| {
        if index >= s.accounts.len() || s.current == Some(index) {
            return Update::DoNothing;
        }
        s.show_account(index);
        Update::RefreshDom
    })
    .unwrap_or(Update::DoNothing)
}

// ==== Help ====

/// File > Help: support and tools on the left, About AzMail on the right (Outlook's Help page).
fn help_page(s: &MailApp, app: &RefAny) -> Dom {
    let about = crate::args::ABOUT;
    let left = vec![
        section("Support"),
        command(
            command_button(
                "Keyboard Shortcuts",
                "keyboard",
                ids::HELP_SHORTCUTS,
                action_ref(app, Action::Shortcuts),
                on_action as ButtonOnClickCallbackType,
            ),
            "Keyboard Shortcuts",
            "Every key AzMail knows, on one page (F1).",
        ),
        section(&format!("Tools for Working With {}", about.name)),
        command(
            command_button(
                "Options",
                "settings",
                ids::HELP_OPTIONS,
                action_ref(app, Action::Options),
                on_action as ButtonOnClickCallbackType,
            ),
            "Options",
            "The reading pane, the To-Do bar, the theme, the mode and the other program \
             settings.",
        ),
    ];
    let mut right = vec![
        Dom::create_div()
            .with_css("display: flex; flex-direction: row; align-items: center; margin-top: 4px;")
            .with_child(
                Dom::create_icon("mail")
                    .with_css("font-size: 40px; margin-right: 12px; color: system:accent;"),
            )
            .with_child(Dom::create_span_with_text(about.name).with_css("font-size: 28px;")),
        section(&format!("About {}", about.name)),
        facts(&[
            (String::from("Version"), String::from(about.version)),
            (String::from("License"), String::from(about.license)),
            (String::from("Mail folder"), s.root.path().display().to_string()),
            (String::from("Accounts"), s.accounts.len().to_string()),
        ]),
        note(about.summary),
        section("Built with"),
    ];
    let credits: Vec<(String, String)> = ui_main::CREDITS
        .iter()
        .map(|(name, license)| ((*name).to_string(), (*license).to_string()))
        .collect();
    right.push(facts(&credits));
    right.push(
        Dom::create_div()
            .with_css("display: flex; flex-direction: row; margin-top: 12px;")
            .with_child(command_button(
                &format!("About {}\u{2026}", about.name),
                "info",
                ids::HELP_ABOUT,
                action_ref(app, Action::About),
                on_action as ButtonOnClickCallbackType,
            )),
    );
    pieces::columns("Help", left, right)
}

// ==== Print: to a PDF file, with a picture of its first page ====

/// A4 at 96 dpi in CSS px: the page the message is printed on.
const A4: (f32, f32) = (794.0, 1123.0);
/// The preview's width in px (its height follows the page).
const PREVIEW_WIDTH: u32 = 360;
/// Lines of the message's text printed at most.
const PRINT_LINES: usize = 3000;
/// The folder (in the AzMail folder) printed files go to.
const EXPORTS: &str = "exports";

/// The open message as a PDF: made when File > Print is opened.
pub(crate) struct PrintJob {
    /// The message: its folder and UID.
    pub(crate) folder: String,
    pub(crate) uid: u32,
    /// Where Print writes it (`exports/<subject>.pdf` in the AzMail folder).
    pub(crate) key: String,
    pub(crate) bytes: Vec<u8>,
    /// The PDF's pages (known once the preview is drawn).
    pub(crate) pages: usize,
    /// The first page as a picture, and its size in px.
    pub(crate) preview: Option<(ImageRef, usize, usize)>,
    pub(crate) error: String,
    /// Where Print wrote it.
    pub(crate) saved: Option<PathBuf>,
}

/// A file name for a message's PDF: its subject's letters and digits, `-` between words.
pub(crate) fn pdf_name(subject: &str) -> String {
    let mut name = String::new();
    for c in subject.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            name.push(c);
        } else if !name.is_empty() && !name.ends_with('-') {
            name.push('-');
        }
    }
    let name: String = name.trim_end_matches('-').chars().take(60).collect();
    let name = name.trim_end_matches('-');
    if name.is_empty() {
        String::from("message")
    } else {
        name.to_string()
    }
}

/// The message on paper, Outlook's Memo Style: the owner's name over a rule, the header lines,
/// then the text.
fn print_dom(owner: &str, view: &MessageView, sent: &str) -> Dom {
    let mut body = Dom::create_body().with_css(
        "margin: 0px; padding: 48px; background: #ffffff; color: #000000; font-family: \
         sans-serif; font-size: 12px;",
    );
    body.add_child(
        Dom::create_div()
            .with_css(
                "font-size: 16px; font-weight: bold; padding-bottom: 4px; margin-bottom: 10px; \
                 border-bottom: 2px solid #000000;",
            )
            .with_child(Dom::create_span_with_text(owner)),
    );
    let subject = if view.subject.is_empty() {
        String::from("(no subject)")
    } else {
        view.subject.clone()
    };
    let mut rows: Vec<(&str, String)> =
        vec![("From:", view.from.clone()), ("Sent:", sent.to_string())];
    if !view.to.is_empty() {
        rows.push(("To:", view.to.clone()));
    }
    if !view.cc.is_empty() {
        rows.push(("Cc:", view.cc.clone()));
    }
    rows.push(("Subject:", subject));
    if !view.attachments.is_empty() {
        let names: Vec<&str> = view.attachments.iter().map(|a| a.name.as_str()).collect();
        rows.push(("Attachments:", names.join("; ")));
    }
    for (label, value) in rows {
        body.add_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; margin-bottom: 2px;")
                .with_child(
                    Dom::create_div()
                        .with_css("width: 110px; flex-shrink: 0; font-weight: bold;")
                        .with_child(Dom::create_span_with_text(label)),
                )
                .with_child(
                    Dom::create_div()
                        .with_css("flex-grow: 1; overflow-wrap: anywhere;")
                        .with_child(Dom::create_span_with_text(value)),
                ),
        );
    }
    let mut text =
        Dom::create_div().with_css("display: flex; flex-direction: column; margin-top: 18px;");
    for line in view.text.lines().take(PRINT_LINES) {
        text.add_child(
            Dom::create_div()
                .with_css("white-space: pre-wrap; min-height: 15px; overflow-wrap: anywhere;")
                .with_child(Dom::create_span_with_text(line)),
        );
    }
    body.with_child(text)
}

/// File > Print is opened: the open message as a PDF (azul's writer, in this callback - it lays
/// the message out), then its first page drawn on a Thread. Nothing open: no job.
pub(crate) fn prepare_print(s: &mut MailApp, info: &mut CallbackInfo, app: &RefAny) {
    let Some((folder, uid, view, date)) = s.open.as_ref().and_then(|open| {
        let view = open.view.clone()?;
        let date = if view.date.is_empty() {
            open.entry.date.clone()
        } else {
            view.date.clone()
        };
        Some((open.folder.clone(), open.entry.uid, view, date))
    }) else {
        s.print = None;
        return;
    };
    if s.print.as_ref().is_some_and(|p| p.folder == folder && p.uid == uid && p.error.is_empty()) {
        return;
    }
    let owner = s
        .current_account()
        .map(|a| {
            if a.name.trim().is_empty() {
                a.email.clone()
            } else {
                a.name.trim().to_string()
            }
        })
        .unwrap_or_else(|| String::from(crate::args::ABOUT.name));
    let sent = message::short_date_in(&date, &chrono::Local);
    let bytes = Pdf::create()
        .from_dom_in_callback(*info, print_dom(&owner, &view, &sent), A4.0, A4.1)
        .as_ref()
        .to_vec();
    let mut job = PrintJob {
        folder: folder.clone(),
        uid,
        key: format!("{EXPORTS}/{}.pdf", pdf_name(&view.subject)),
        bytes,
        pages: 0,
        preview: None,
        error: String::new(),
        saved: None,
    };
    if job.bytes.is_empty() {
        job.error =
            String::from("azul's PDF writer made no file (a build without its `pdf` feature?).");
    } else {
        println!("AZMAIL_PRINT_PDF {} {uid} {}", folder, job.bytes.len());
        let preview = PreviewJob {
            folder,
            uid,
            bytes: job.bytes.clone(),
        };
        info.add_thread(
            ThreadId::unique(),
            Thread::create(RefAny::new(preview), app.clone(), preview_thread),
        );
    }
    s.print = Some(job);
}

/// Print: the PDF into `exports/` in the AzMail folder (written on a Thread).
pub(crate) fn print_now(s: &mut MailApp, info: &mut CallbackInfo, app: RefAny) {
    let Some(job) = s.print.as_ref().filter(|j| !j.bytes.is_empty()) else {
        s.notice = String::from("Select a message to print first.");
        return;
    };
    crate::spawn_io(
        info,
        app,
        IoJob::SavePdf {
            root: s.root.clone(),
            key: job.key.clone(),
            bytes: job.bytes.clone(),
        },
    );
}

/// The PDF is written (`IoDone::PdfSaved`).
pub(crate) fn printed(s: &mut MailApp, key: &str, path: PathBuf) {
    println!("AZMAIL_PRINTED {}", path.display());
    s.notice = format!("Printed to {}", path.display());
    if let Some(job) = s.print.as_mut().filter(|j| j.key == key) {
        job.saved = Some(path);
    }
}

/// What the preview Thread draws.
#[derive(Clone)]
struct PreviewJob {
    folder: String,
    uid: u32,
    bytes: Vec<u8>,
}

/// What it drew.
struct PreviewDone {
    folder: String,
    uid: u32,
    pages: usize,
    image: Option<RawImage>,
    error: String,
}

/// The PDF's first page drawn `PREVIEW_WIDTH` px wide on white paper: the page's SVG (azul's
/// PDF -> SVG) through azul's SVG renderer, as AzPdf draws its thumbnails.
fn render_first_page(pdf: &ParsedPdf) -> Option<RawImage> {
    let svg = pdf.page_to_svg(0).into_option()?;
    let parsed = match ParsedSvg::from_string(svg, SvgParseOptions::create_default()) {
        ResultParsedSvgSvgParseError::Ok(parsed) => parsed,
        ResultParsedSvgSvgParseError::Err(_) => return None,
    };
    let mut options = SvgRenderOptions::create_default();
    options.fit = SvgFitTo::Width(PREVIEW_WIDTH);
    options.background_color = OptionColorU::Some(ColorU {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    });
    parsed.render(options).into_option()
}

extern "C" fn preview_thread(mut init: RefAny, mut sender: ThreadSender, _receiver: ThreadReceiver) {
    let Some(job) = init
        .downcast_ref::<PreviewJob>()
        .map(|job| PreviewJob::clone(&job))
    else {
        return;
    };
    let pdf = ParsedPdf::create_from_bytes(U8VecRef::from(&job.bytes[..]));
    let done = if pdf.is_valid() {
        let image = render_first_page(&pdf);
        let error = if image.is_none() {
            String::from("The first page could not be drawn.")
        } else {
            String::new()
        };
        PreviewDone {
            folder: job.folder,
            uid: job.uid,
            pages: pdf.page_count(),
            image,
            error,
        }
    } else {
        PreviewDone {
            folder: job.folder,
            uid: job.uid,
            pages: 0,
            image: None,
            error: format!("The PDF could not be read back: {}", pdf.get_error().as_str()),
        }
    };
    sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg {
        refany: RefAny::new(done),
        callback: WriteBackCallback {
            cb: on_preview_done,
            ctx: OptionRefAny::None,
        },
    }));
}

/// The first page is drawn: the Print page shows it (if that message is still the one).
extern "C" fn on_preview_done(mut app: RefAny, mut payload: RefAny, _info: CallbackInfo) -> Update {
    let taken = payload
        .downcast_mut::<PreviewDone>()
        .map(|mut d| (d.folder.clone(), d.uid, d.pages, d.image.take(), d.error.clone()));
    let Some((folder, uid, pages, image, error)) = taken else {
        return Update::DoNothing;
    };
    with_app(&mut app, |s, _| {
        let Some(job) = s
            .print
            .as_mut()
            .filter(|j| j.folder == folder && j.uid == uid)
        else {
            return Update::DoNothing;
        };
        job.pages = pages;
        job.error = error;
        if let Some(raw) = image {
            let (width, height) = (raw.width, raw.height);
            job.preview = ImageRef::create_rawimage(raw)
                .into_option()
                .map(|image| (image, width, height));
        }
        println!(
            "AZMAIL_PRINT_PREVIEW pages={pages} shown={}",
            job.preview.is_some()
        );
        Update::RefreshDom
    })
    .unwrap_or(Update::DoNothing)
}

/// File > Print: Print (to a PDF file), the printer and the style on the left, the preview on
/// the right (Outlook's Print page).
fn print_page(s: &MailApp, app: &RefAny) -> Dom {
    let job = s.print.as_ref();
    let into = format!("Into {}", s.root.path().join(EXPORTS).display());
    let mut left = vec![
        command(
            command_button(
                "Print",
                "print",
                ids::PRINT,
                action_ref(app, Action::Print),
                on_action as ButtonOnClickCallbackType,
            ),
            "Print",
            "Prints the open message to a PDF file: A4, Memo Style.",
        ),
        section("Printer"),
        card("picture_as_pdf", "PDF file", &[into.as_str()], true),
        section("Settings"),
        card(
            "description",
            "Memo Style",
            &["Your name over the message's header lines, then its text."],
            true,
        ),
    ];
    match job {
        Some(job) if !job.error.is_empty() => left.push(note(&job.error)),
        Some(PrintJob {
            saved: Some(path), ..
        }) => left.push(note(&format!("Printed to {}", path.display()))),
        _ => {}
    }
    let mut right = vec![section("Preview")];
    match job {
        None => right.push(note(
            "No message is open: select one in the message list, then come back to File > \
             Print.",
        )),
        Some(PrintJob {
            preview: Some((image, width, height)),
            pages,
            ..
        }) => {
            right.push(
                Dom::create_div()
                    .with_css(
                        "display: flex; flex-direction: column; align-items: flex-start; \
                         margin-top: 10px;",
                    )
                    .with_child(
                        Dom::create_image(image.clone())
                            .with_css(format!(
                                "width: {width}px; height: {height}px; border: 1px solid \
                                 system:separator;"
                            ))
                            .with_id(ids::PRINT_PREVIEW),
                    ),
            );
            let count = if *pages == 1 {
                String::from("1 page")
            } else {
                format!("Page 1 of {pages}")
            };
            right.push(note(&count));
        }
        Some(job) if job.error.is_empty() => right.push(note("Drawing the preview\u{2026}")),
        Some(_) => {}
    }
    pieces::columns("Print", left, right)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A subject becomes a file name of letters, digits and dashes; nothing usable is
    /// "message".
    #[test]
    fn a_subject_becomes_a_safe_pdf_name() {
        assert_eq!(pdf_name("Re: Garden plan for October"), "Re-Garden-plan-for-October");
        assert_eq!(pdf_name("  ../../etc/passwd  "), "etc-passwd");
        assert_eq!(pdf_name("Grüße aus München"), "Gr-e-aus-M-nchen");
        assert_eq!(pdf_name(""), "message");
        assert_eq!(pdf_name("???"), "message");
        assert!(pdf_name(&"x".repeat(200)).len() <= 60);
    }

    /// The wizard and Account Settings light Info; the other pages light themselves.
    #[test]
    fn the_account_pages_light_info() {
        assert_eq!(nav_item(PAGE_ADD_ACCOUNT), PAGE_INFO);
        assert_eq!(nav_item(PAGE_SETTINGS), PAGE_INFO);
        assert_eq!(nav_item(PAGE_PRINT), PAGE_PRINT);
        assert_eq!(nav_item(PAGE_HELP), PAGE_HELP);
        assert_eq!(nav_item(PAGE_EXIT), PAGE_EXIT);
    }
}

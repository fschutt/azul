//! Account Settings' "Other programs": the Azlin Bridge's settings to type into Apple Mail,
//! Thunderbird or Outlook (IMAP and SMTP), Finder or Explorer (WebDAV) and a calendar or contacts
//! program (CalDAV / CardDAV), read from the bridge's own settings file (azcloud-kit's `bridge`),
//! each with a Copy button. Copy password reads the bridge's password - its secrets file, or the
//! system keyring through [`crate::keyring_call`] - straight into the clipboard, never onto the
//! screen.

use std::{path::PathBuf, time::Duration};

use azcloud_kit::bridge::{self, BridgeSettings, PasswordSource, Row};
use azul::{
    callbacks::ButtonOnClickCallbackType,
    dom::ClipboardContent,
    file::FilePath,
    option::OptionString,
    prelude::*,
    str::String as AzString,
    vec::StyledTextRunVec,
};

use crate::{keyring_call, with_app, KeyringCall, KeyringOp, MailApp};

/// How long the bridge's port is asked whether it answers.
const PROBE: Duration = Duration::from_millis(200);

const PAGE: &str = "display: flex; flex-direction: column; max-width: 640px;";
const TEXT: &str = "font-size: 13px; margin-top: 4px;";
const HEADING: &str = "font-size: 12px; margin-top: 14px; margin-bottom: 4px; font-weight: bold; \
                       @theme(flora) { font-size: 11px; text-transform: uppercase; \
                       letter-spacing: 0.1em; color: system:secondary-text; }";
const NOTE: &str = "font-size: 12px; margin-top: 8px; opacity: 0.75; \
                    @theme(flora) { opacity: 1; color: system:secondary-text; }";

/// What "Other programs" shows.
#[derive(Debug, Clone, Default)]
pub(crate) struct BridgeView {
    /// The bridge's settings; `None` when it is not set up on this computer.
    pub(crate) settings: Option<BridgeSettings>,
    /// Whether it answered when the page opened.
    pub(crate) running: bool,
    /// What the last Copy did.
    pub(crate) note: String,
}

impl BridgeView {
    /// The bridge of this user as it is now: its settings file, and whether it answers.
    pub(crate) fn load() -> BridgeView {
        let config_dir = FilePath::get_config_dir()
            .into_option()
            .map(|dir| PathBuf::from(dir.inner.as_str()))
            .filter(|dir| !dir.as_os_str().is_empty());
        let settings = BridgeSettings::find(std::env::var(bridge::HOME_VAR).ok(), config_dir);
        let running = settings.as_ref().is_some_and(|s| s.running(PROBE));
        BridgeView {
            settings,
            running,
            note: String::new(),
        }
    }
}

/// A Copy button's data: the app, what it copies, what the note calls it.
struct CopyRef {
    app: RefAny,
    text: String,
    what: String,
}

/// Copy password's data.
struct PasswordRef {
    app: RefAny,
}

fn line(text: &str) -> Dom {
    Dom::create_span_with_text(text).with_css(TEXT)
}

fn clipboard(info: &mut CallbackInfo, text: &str) {
    info.set_clipboard_content(ClipboardContent {
        plain_text: AzString::from(text),
        styled_runs: StyledTextRunVec::create(),
        html: OptionString::None,
    });
}

fn copy_button(app: &RefAny, label: &str, text: &str, what: &str) -> Dom {
    Button::create(AzString::from(label))
        .with_on_click(
            RefAny::new(CopyRef {
                app: app.clone(),
                text: text.to_string(),
                what: what.to_string(),
            }),
            on_copy as ButtonOnClickCallbackType,
        )
        .dom()
}

fn row_of(app: &RefAny, row: &Row) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center; padding: 2px 0px;")
        .with_child(
            Dom::create_span_with_text(row.label.as_str())
                .with_css("width: 220px; flex-shrink: 0; font-size: 13px;"),
        )
        .with_child(
            Dom::create_span_with_text(row.value.as_str())
                .with_css("flex-grow: 1; font-size: 13px; font-family: monospace;"),
        )
        .with_child(copy_button(app, "Copy", &row.value, &row.label))
}

/// "Other programs": the bridge's settings, each with Copy; else how to set the bridge up.
pub(crate) fn section(view: &BridgeView, app: &RefAny) -> Dom {
    let mut page = Dom::create_div().with_css(PAGE);
    let Some(settings) = &view.settings else {
        page.add_child(line(
            "The Azlin Bridge lets Apple Mail, Thunderbird or Outlook, Finder or Explorer and \
             calendar and contacts programs reach your Azlin drive on this computer. It is not set \
             up here: run azul-bridge init --address <your address>, then azul-bridge serve \
             (azul-bridge autostart enable starts it at every login).",
        ));
        return page;
    };
    page.add_child(line(if view.running {
        "The Azlin Bridge is running on this computer. Set up the other program with these \
         settings and the bridge's password:"
    } else {
        "The Azlin Bridge is set up but not running: start it with azul-bridge serve \
         (azul-bridge autostart enable starts it at every login). The other programs use these \
         settings and the bridge's password:"
    }));
    for (heading, rows) in [
        ("Mail (Apple Mail, Thunderbird, Outlook)", settings.mail_rows()),
        ("Files (Finder, Explorer, the file managers)", settings.files_rows()),
        ("Calendars and contacts", settings.calendar_rows()),
    ] {
        page.add_child(Dom::create_span_with_text(heading).with_css(HEADING));
        for row in &rows {
            page.add_child(row_of(app, row));
        }
    }
    page.add_child(
        Dom::create_div()
            .with_css("display: flex; flex-direction: row; margin-top: 12px;")
            .with_child(copy_button(app, "Copy all settings", &settings.summary(), "every setting"))
            .with_child(
                Dom::create_div().with_css("margin-left: 8px;").with_child(
                    Button::create(AzString::from("Copy password"))
                        .with_on_click(
                            RefAny::new(PasswordRef { app: app.clone() }),
                            on_copy_password as ButtonOnClickCallbackType,
                        )
                        .dom(),
                ),
            ),
    );
    if !view.note.is_empty() {
        page.add_child(Dom::create_span_with_text(view.note.as_str()).with_css(NOTE));
    }
    page
}

fn set_note(s: &mut MailApp, note: &str) {
    if let Some(editor) = s.editor.as_mut() {
        editor.bridge.note = note.to_string();
    }
}

extern "C" fn on_copy(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, text, what)) = data
        .downcast_ref::<CopyRef>()
        .map(|c| (c.app.clone(), c.text.clone(), c.what.clone()))
    else {
        return Update::DoNothing;
    };
    clipboard(&mut info, &text);
    with_app(&mut app, |s, _| {
        set_note(s, &format!("Copied: {what}."));
        Update::RefreshDom
    })
    .unwrap_or(Update::DoNothing)
}

extern "C" fn on_copy_password(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut app) = data.downcast_ref::<PasswordRef>().map(|p| p.app.clone()) else {
        return Update::DoNothing;
    };
    with_app(&mut app, |s, _| {
        let source = s
            .editor
            .as_ref()
            .and_then(|editor| editor.bridge.settings.as_ref())
            .map(BridgeSettings::password_source);
        match source {
            Some(PasswordSource::File(path)) => match bridge::file_password(&path) {
                Some(password) => copy_password(s, &mut info, &password),
                None => set_note(
                    s,
                    "The bridge's secrets file has no password: azul-bridge password makes a new one.",
                ),
            },
            Some(PasswordSource::Keyring(entry)) => {
                set_note(s, "Asking the system keyring for the bridge's password ...");
                keyring_call(
                    s,
                    &mut info,
                    KeyringCall {
                        op: KeyringOp::GetBridge,
                        key: entry,
                        secret: None,
                    },
                );
            }
            None => {}
        }
        Update::RefreshDom
    })
    .unwrap_or(Update::DoNothing)
}

/// The bridge's password, read: into the clipboard, never onto the screen.
pub(crate) fn copy_password(s: &mut MailApp, info: &mut CallbackInfo, password: &str) {
    clipboard(info, password);
    set_note(
        s,
        "Copied the bridge's password: paste it where the other program asks for the password.",
    );
}

/// The keyring gave no password for the bridge (`outcome`: what it answered).
pub(crate) fn password_missing(s: &mut MailApp, outcome: &str) {
    set_note(
        s,
        &format!(
            "The system keyring has no password of the bridge ({outcome}): azul-bridge password \
             makes a new one."
        ),
    );
}

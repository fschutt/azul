//! Options > Drives' "Use with other programs": the Azlin Bridge's WebDAV address for Finder,
//! Explorer and the Linux file managers, and its mail and calendar addresses, read from the
//! bridge's own settings file (azcloud-kit's `bridge`), each with a Copy button. Copy password
//! reads the bridge's password - its secrets file, or the system keyring through AzDrive's keyring
//! queue ([`crate::keyring`]) - straight into the clipboard, never onto the screen.

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

use crate::{keyring, with_state, DriveState, KeyringCall, KeyringOp};

/// How long the bridge's port is asked whether it answers.
const PROBE: Duration = Duration::from_millis(200);

const TEXT: &str = "font-size: 13px; margin-top: 4px;";
const HEADING: &str = "font-size: 12px; margin-top: 12px; margin-bottom: 4px; font-weight: bold;";

/// What "Use with other programs" shows.
#[derive(Debug, Clone, Default)]
pub(crate) struct BridgeView {
    /// The bridge's settings; `None` when it is not set up on this computer.
    pub(crate) settings: Option<BridgeSettings>,
    /// Whether it answered when the Options opened.
    pub(crate) running: bool,
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
        BridgeView { settings, running }
    }
}

/// A Copy button's data: the app, what it copies, what the notice calls it.
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
    Dom::create_span_with_text(AzString::from(text)).with_css(TEXT)
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
            Dom::create_span_with_text(AzString::from(row.label.as_str()))
                .with_css("width: 220px; flex-shrink: 0; font-size: 13px;"),
        )
        .with_child(
            Dom::create_span_with_text(AzString::from(row.value.as_str()))
                .with_css("flex-grow: 1; font-size: 13px; font-family: monospace;"),
        )
        .with_child(copy_button(app, "Copy", &row.value, &row.label))
}

/// "Use with other programs": the bridge's addresses, each with Copy; else how to set it up.
pub(crate) fn section(view: &BridgeView, app: &RefAny) -> Dom {
    let mut page = Dom::create_div().with_css("display: flex; flex-direction: column;");
    let Some(settings) = &view.settings else {
        page.add_child(line(
            "The Azlin Bridge shows your Azlin drive to Finder, Explorer and the file managers \
             (and its mail and calendars to other programs) on this computer. It is not set up \
             here: run azul-bridge init --address <your address>, then azul-bridge serve \
             (azul-bridge autostart enable starts it at every login).",
        ));
        return page;
    };
    page.add_child(line(if view.running {
        "The Azlin Bridge is running on this computer: connect to it with these settings and the \
         bridge's password (Finder: Go > Connect to Server; Explorer: Map network drive)."
    } else {
        "The Azlin Bridge is set up but not running: start it with azul-bridge serve (azul-bridge \
         autostart enable starts it at every login). Then connect with these settings and the \
         bridge's password."
    }));
    for (heading, rows) in [
        ("Files (Finder, Explorer, the file managers)", settings.files_rows()),
        ("Mail (Apple Mail, Thunderbird, Outlook)", settings.mail_rows()),
        ("Calendars and contacts", settings.calendar_rows()),
    ] {
        page.add_child(Dom::create_span_with_text(AzString::from(heading)).with_css(HEADING));
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
    page
}

extern "C" fn on_copy(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, text, what)) = data
        .downcast_ref::<CopyRef>()
        .map(|c| (c.app.clone(), c.text.clone(), c.what.clone()))
    else {
        return Update::DoNothing;
    };
    clipboard(&mut info, &text);
    with_state(&mut app, &mut info, |_info, _app, s| s.info(format!("Copied: {what}.")))
}

extern "C" fn on_copy_password(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut app) = data.downcast_ref::<PasswordRef>().map(|p| p.app.clone()) else {
        return Update::DoNothing;
    };
    with_state(&mut app, &mut info, |info, _app, s| {
        let source = s.bridge.settings.as_ref().map(BridgeSettings::password_source);
        match source {
            Some(PasswordSource::File(path)) => match bridge::file_password(&path) {
                Some(password) => copy_password(info, s, &password),
                None => s.error("The bridge's secrets file has no password: azul-bridge password makes a new one."),
            },
            Some(PasswordSource::Keyring(entry)) => {
                keyring(info, s, KeyringOp::BridgePassword, KeyringCall::Get(entry));
            }
            None => {}
        }
    })
}

/// The bridge's password, read: into the clipboard, never onto the screen.
pub(crate) fn copy_password(info: &mut CallbackInfo, s: &mut DriveState, password: &str) {
    clipboard(info, password);
    s.success("Copied the bridge's password: paste it where the other program asks for the password.");
}

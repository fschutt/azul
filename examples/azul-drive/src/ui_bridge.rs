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
    time::{Duration as UiDuration, SystemTimeDiff},
    vec::StyledTextRunVec,
};

use azul_appkit::l10n::{label, Phrase, Text};

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
    Dom::create_span_with_text(label(text)).with_css(TEXT)
}

/// The clipboard's content for `text`: `concealed` keeps clipboard managers and history from
/// recording it.
fn content_of(text: &str, concealed: bool) -> ClipboardContent {
    ClipboardContent {
        plain_text: AzString::from(text),
        styled_runs: StyledTextRunVec::create(),
        html: OptionString::None,
        files: azul::vec::StringVec::create(),
        image: azul::option::OptionU8Vec::None,
        link: azul::option::OptionString::None,
        concealed,
    }
}

/// An address or a name on the clipboard (nothing secret).
pub(crate) fn clipboard(info: &mut CallbackInfo, text: &str) {
    info.set_clipboard_content(content_of(text, false));
}

/// How long a secret stays on the clipboard: a password manager's habit.
pub(crate) const CLEAR_SECRET_MS: u64 = 60_000;

/// What copying a secret asks of the window: the clipboard and a timer. [`CallbackInfo`] is the
/// window; a test is a recording of what was asked.
pub(crate) trait SecretClipboard {
    /// Puts `content` on the clipboard.
    fn put(&mut self, content: ClipboardContent);

    /// Calls the clear timer ([`on_clear_copied_secret`]) with `secret` after `delay_ms`.
    fn clear_later(&mut self, secret: CopiedSecret, delay_ms: u64);
}

impl SecretClipboard for CallbackInfo {
    fn put(&mut self, content: ClipboardContent) {
        self.set_clipboard_content(content);
    }

    fn clear_later(&mut self, secret: CopiedSecret, delay_ms: u64) {
        let get_time = self.get_system_time_fn();
        self.add_timer(
            TimerId::unique(),
            Timer::create(RefAny::new(secret), on_clear_copied_secret, get_time)
                .with_delay(UiDuration::System(SystemTimeDiff::from_millis(delay_ms))),
        );
    }
}

/// A secret on the clipboard - the recovery code, the bridge's password: flagged `concealed`, so
/// that clipboard managers and history leave it alone, and taken off again after
/// [`CLEAR_SECRET_MS`] unless something else was copied meanwhile (the timer keeps the text, to
/// tell). `cleared` is the line printed when it is taken off (AZ-K14: the bridge's password went
/// to the clipboard unconcealed and stayed for ever, while the recovery code did neither).
pub(crate) fn clipboard_secret(
    window: &mut dyn SecretClipboard,
    text: &str,
    cleared: &'static str,
) {
    window.put(content_of(text, true));
    window.clear_later(
        CopiedSecret {
            text: text.to_string(),
            cleared,
        },
        CLEAR_SECRET_MS,
    );
}

/// A secret that was copied is taken off the clipboard now - when the clipboard still holds it
/// (or cannot be read): what the user copied since stays.
pub(crate) fn clipboard_forget(info: &mut CallbackInfo, secret: &str) {
    let now = info.get_system_clipboard().into_option();
    if still_ours(now.as_ref().map(|c| c.plain_text.as_str()), secret) {
        info.set_clipboard_content(content_of("", false));
    }
}

/// A secret the clear timer takes off the clipboard again.
pub(crate) struct CopiedSecret {
    text: String,
    cleared: &'static str,
}

/// Whether the clipboard still holds the secret that was copied (or nothing readable): then it is
/// ours to clear; anything else was copied by the user since, and stays.
fn still_ours(current: Option<&str>, copied: &str) -> bool {
    current.is_none_or(|current| current == copied)
}

extern "C" fn on_clear_copied_secret(
    mut data: RefAny,
    info: TimerCallbackInfo,
) -> TimerCallbackReturn {
    let mut callback_info = info.callback_info;
    let Some((copied, cleared)) = data
        .downcast_ref::<CopiedSecret>()
        .map(|c| (c.text.clone(), c.cleared))
    else {
        return TimerCallbackReturn::terminate_unchanged();
    };
    let now = callback_info.get_system_clipboard().into_option();
    if !still_ours(now.as_ref().map(|c| c.plain_text.as_str()), &copied) {
        return TimerCallbackReturn::terminate_unchanged();
    }
    callback_info.set_clipboard_content(content_of("", false));
    println!("{cleared}");
    TimerCallbackReturn::terminate_unchanged()
}

fn copy_button(app: &RefAny, words: &str, text: &str, what: &str) -> Dom {
    Button::create(label(words))
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
            // The kit's words by their ids (azcloud-kit's bridge resource).
            Dom::create_span_with_text(label(row.label_id))
                .with_css("width: 220px; flex-shrink: 0; font-size: 13px;"),
        )
        .with_child(
            Dom::create_span_with_text(label(row.value_id.unwrap_or(&row.value)))
                .with_css("flex-grow: 1; font-size: 13px; font-family: monospace;"),
        )
        .with_child(copy_button(app, "azdrive-bridge-copy", &row.value, row.label_id))
}

/// "Use with other programs": the bridge's addresses, each with Copy; else how to set it up.
pub(crate) fn section(view: &BridgeView, app: &RefAny) -> Dom {
    let mut page = Dom::create_div().with_css("display: flex; flex-direction: column;");
    let Some(settings) = &view.settings else {
        page.add_child(line("azdrive-bridge-not-set-up"));
        return page;
    };
    page.add_child(line(if view.running {
        "azdrive-bridge-running"
    } else {
        "azdrive-bridge-not-running"
    }));
    for (heading, rows) in [
        ("azdrive-bridge-files", settings.files_rows()),
        ("azdrive-bridge-mail", settings.mail_rows()),
        ("azdrive-bridge-calendars", settings.calendar_rows()),
    ] {
        page.add_child(Dom::create_span_with_text(label(heading)).with_css(HEADING));
        for row in &rows {
            page.add_child(row_of(app, row));
        }
    }
    page.add_child(
        Dom::create_div()
            .with_css("display: flex; flex-direction: row; margin-top: 12px;")
            .with_child(copy_button(
                app,
                "azdrive-bridge-copy-all",
                &settings.summary(),
                "azdrive-bridge-every-setting",
            ))
            .with_child(
                Dom::create_div().with_css("margin-left: 8px;").with_child(
                    Button::create(label("azdrive-bridge-copy-password"))
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
    with_state(&mut app, &mut info, |_info, _app, s| {
        let what = azul_appkit::l10n::t_label(&what);
        s.info(Phrase::new("azdrive-bridge-copied").arg("what", what));
    })
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
                None => s.error(Text::key("azdrive-bridge-no-password")),
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
    password_copied(info, password);
    s.success(Text::key("azdrive-bridge-password-copied"));
}

/// The bridge's password on the clipboard as a secret ([`clipboard_secret`]); what "Copy
/// password" does to the window.
fn password_copied(window: &mut dyn SecretClipboard, password: &str) {
    clipboard_secret(
        window,
        password,
        "AZDRIVE_BRIDGE_PASSWORD_CLIPBOARD_CLEARED",
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A window that records what a copy asked of it.
    #[derive(Default)]
    struct Recorded {
        put: Vec<ClipboardContent>,
        timers: Vec<(String, u64)>,
    }

    impl SecretClipboard for Recorded {
        fn put(&mut self, content: ClipboardContent) {
            self.put.push(content);
        }

        fn clear_later(&mut self, secret: CopiedSecret, delay_ms: u64) {
            self.timers.push((secret.text, delay_ms));
        }
    }

    /// AZ-K14: "Copy password" put the bridge's password on the clipboard with `concealed: false`
    /// and never took it off, while the recovery code was concealed and cleared. Through the
    /// function "Copy password" calls: one copy, concealed, and one clear timer for that very text
    /// after a minute.
    #[test]
    fn az_k14_the_bridge_password_goes_to_the_clipboard_concealed_and_is_cleared_after_a_minute() {
        let mut window = Recorded::default();
        password_copied(&mut window, "k7m2p-9qxat-4bfw8");
        assert_eq!(window.put.len(), 1);
        assert!(window.put[0].concealed, "clipboard managers leave it alone");
        assert_eq!(window.put[0].plain_text.as_str(), "k7m2p-9qxat-4bfw8");
        assert_eq!(
            window.timers,
            [(String::from("k7m2p-9qxat-4bfw8"), 60_000)],
            "taken off again, once"
        );
    }

    /// AZ-K14: an address is no secret (not concealed, no timer), and the clear timer takes a
    /// secret off only while it is still what the clipboard holds: a later copy of the user's
    /// stays.
    #[test]
    fn az_k14_an_address_is_not_concealed_and_a_later_copy_is_not_cleared() {
        assert!(!content_of("http://127.0.0.1:1180/", false).concealed);
        assert!(content_of("k7m2p-9qxat-4bfw8", true).concealed);
        assert!(
            still_ours(Some("k7m2p-9qxat-4bfw8"), "k7m2p-9qxat-4bfw8"),
            "the secret: clear it"
        );
        assert!(
            still_ours(None, "k7m2p-9qxat-4bfw8"),
            "nothing readable: clear it"
        );
        assert!(
            !still_ours(Some("something the user copied since"), "k7m2p-9qxat-4bfw8"),
            "another copy stays"
        );
    }
}

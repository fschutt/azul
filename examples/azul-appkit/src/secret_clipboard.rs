//! A secret on the clipboard (feature `look`, in `azul`): the recovery code, the Azlin Bridge's
//! password - what AzDrive, AzMail and the bridge's tray copy for the user.
//!
//! The one place such a copy is made (AZ-K14, AZ-K12; moved here from AzDrive when AzMail and the
//! bridge's tray turned out to copy the same password plain and for ever, X-DELTA-AZDRIVE-KEYS-24):
//! the text goes to the clipboard flagged `concealed`, so that clipboard managers and history leave
//! it alone, and it is taken off again after [`CLEAR_SECRET_MS`] unless the user copied something
//! else meanwhile. What the window is asked for goes through [`SecretClipboard`], which
//! `CallbackInfo` implements; a test is a recording of it.
//!
//! The timer is a window timer: a program that ends before it fires would leave the secret on the
//! pasteboard. So every copy is also remembered here - by its SHA-256, never the text - until the
//! timer ends it, and [`forget_copied`] takes what is still on the clipboard off when the window
//! or the program closes.

use std::sync::{Mutex, MutexGuard, PoisonError};

use azul::{
    dom::ClipboardContent,
    option::OptionString,
    prelude::*,
    str::String as AzString,
    time::{Duration as UiDuration, SystemTimeDiff},
    vec::StyledTextRunVec,
};

/// How long a secret stays on the clipboard: a password manager's habit.
pub const CLEAR_SECRET_MS: u64 = 60_000;

/// The clipboard's content for `text`: `concealed` keeps clipboard managers and history from
/// recording it.
#[must_use]
pub fn content_of(text: &str, concealed: bool) -> ClipboardContent {
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

/// What copying a secret asks of the window: the clipboard and a timer. [`CallbackInfo`] is the
/// window; a test is a recording of what was asked.
pub trait SecretClipboard {
    /// Puts `content` on the clipboard.
    fn put(&mut self, content: ClipboardContent);

    /// Calls the clear timer with `secret` after `delay_ms`.
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

/// A secret that was copied, for the timer that takes it off the clipboard again.
pub struct CopiedSecret {
    text: String,
    cleared: &'static str,
}

impl CopiedSecret {
    /// The text that was copied.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }
}

/// A secret on the clipboard: flagged `concealed`, taken off again after [`CLEAR_SECRET_MS`] unless
/// something else was copied meanwhile (the timer keeps the text, to tell), and remembered until
/// then for [`forget_copied`]. `cleared` is the line printed when the timer takes it off.
pub fn clipboard_secret(window: &mut dyn SecretClipboard, text: &str, cleared: &'static str) {
    window.put(content_of(text, true));
    remember(text);
    window.clear_later(
        CopiedSecret {
            text: text.to_string(),
            cleared,
        },
        CLEAR_SECRET_MS,
    );
}

/// A secret that was copied is taken off the clipboard now - when the clipboard still holds it
/// (or cannot be read): what the user copied since stays. Whatever way it got there: the Copy
/// button, or a Cmd-C on the text the window shows.
pub fn clipboard_forget(info: &mut CallbackInfo, secret: &str) {
    let now = info.get_system_clipboard().into_option();
    if still_ours(now.as_ref().map(|c| c.plain_text.as_str()), secret) {
        info.set_clipboard_content(content_of("", false));
    }
    release(secret);
}

/// Takes off the clipboard every secret this program copied that it still holds, and forgets
/// them: for the moment the window or the program closes, before the clear timers can fire.
pub fn forget_copied(info: &mut CallbackInfo) {
    let digests = std::mem::take(&mut *copied());
    if digests.is_empty() {
        return;
    }
    let now = info.get_system_clipboard().into_option();
    let holds = now
        .as_ref()
        .map(|c| digest_of(c.plain_text.as_str()))
        .is_some_and(|held| digests.contains(&held));
    if holds {
        info.set_clipboard_content(content_of("", false));
    }
}

/// Whether the clipboard still holds the secret that was copied (or nothing readable): then it is
/// ours to clear; anything else was copied by the user since, and stays.
#[must_use]
pub fn still_ours(current: Option<&str>, copied: &str) -> bool {
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
    release(&copied);
    let now = callback_info.get_system_clipboard().into_option();
    if !still_ours(now.as_ref().map(|c| c.plain_text.as_str()), &copied) {
        return TimerCallbackReturn::terminate_unchanged();
    }
    callback_info.set_clipboard_content(content_of("", false));
    println!("{cleared}");
    TimerCallbackReturn::terminate_unchanged()
}

/// The SHA-256 of the secrets this program copied and no timer has ended yet.
static COPIED: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn copied() -> MutexGuard<'static, Vec<String>> {
    COPIED.lock().unwrap_or_else(PoisonError::into_inner)
}

fn digest_of(text: &str) -> String {
    azul_storage::sigv4::sha256_hex(text.as_bytes())
}

fn remember(text: &str) {
    let digest = digest_of(text);
    let mut list = copied();
    if !list.contains(&digest) {
        list.push(digest);
    }
}

fn release(text: &str) {
    let digest = digest_of(text);
    copied().retain(|held| *held != digest);
}

#[cfg(all(test, feature = "azul"))]
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
            self.timers.push((secret.text().to_string(), delay_ms));
        }
    }

    /// X-DELTA-AZDRIVE-KEYS-24: a secret goes to the clipboard concealed, with one clear timer for
    /// that very text after a minute, and is remembered for the close - by its digest.
    #[test]
    fn a_secret_goes_to_the_clipboard_concealed_and_is_cleared_after_a_minute() {
        release("k7m2p-9qxat-4bfw8");
        let mut window = Recorded::default();
        clipboard_secret(&mut window, "k7m2p-9qxat-4bfw8", "CLEARED");
        assert_eq!(window.put.len(), 1);
        assert!(window.put[0].concealed, "clipboard managers leave it alone");
        assert_eq!(window.put[0].plain_text.as_str(), "k7m2p-9qxat-4bfw8");
        assert_eq!(
            window.timers,
            [(String::from("k7m2p-9qxat-4bfw8"), 60_000)],
            "taken off again, once"
        );
        assert!(
            copied().contains(&digest_of("k7m2p-9qxat-4bfw8")),
            "and remembered for the close"
        );
        assert!(
            copied()
                .iter()
                .all(|held| held.len() == 64 && held.bytes().all(|b| b.is_ascii_hexdigit())),
            "by its digest, never its text"
        );
        release("k7m2p-9qxat-4bfw8");
        assert!(!copied().contains(&digest_of("k7m2p-9qxat-4bfw8")));
    }

    /// An address is no secret (not concealed), and the clear timer takes a secret off only while it
    /// is still what the clipboard holds: a later copy of the user's stays.
    #[test]
    fn an_address_is_not_concealed_and_a_later_copy_is_not_cleared() {
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

    /// The same secret copied twice is remembered once.
    #[test]
    fn a_secret_copied_twice_is_remembered_once() {
        release("again-and-again");
        let mut window = Recorded::default();
        clipboard_secret(&mut window, "again-and-again", "CLEARED");
        clipboard_secret(&mut window, "again-and-again", "CLEARED");
        let digest = digest_of("again-and-again");
        assert_eq!(copied().iter().filter(|held| **held == digest).count(), 1);
        release("again-and-again");
    }
}

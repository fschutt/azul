//! The phone's language, read from iOS.
//!
//! `NSLocale.preferredLanguages` is the person's list of languages in their order of preference, as
//! BCP 47 tags (`de-DE`, `zh-Hans-CN`, `en-US`). The style the app is made with says `en-US` for every
//! phone, so a German phone showed English: the window reads the list before it takes its style
//! ([`language_of_phone`]) and again when the app becomes active ([`adopt_language`]; iOS usually ends
//! and starts an app again when the language is changed in Settings, and the Simulator and a change
//! under a running app do not).

use std::sync::Arc;

use azul_css::system::SystemLanguage;
use objc::{class, msg_send, runtime::Object, sel, sel_impl};

use super::IOSWindow;
use crate::desktop::shell2::common::event::PlatformWindow;

/// How many entries of the list are read: a phone lists a handful of languages, and a list that is
/// longer than this is a broken answer.
const MAX_LANGUAGES: usize = 64;

/// The entries of `[NSLocale preferredLanguages]` in the person's order; empty where the call, the
/// list or an entry answers nil.
fn preferred_languages() -> Vec<String> {
    let mut out = Vec::new();
    // SAFETY: `preferredLanguages` is a class method of Foundation that answers an `NSArray` of
    // `NSString` (or nil); every object is checked for nil before a message goes to it, and the
    // strings are copied before the autorelease pool of the caller ends.
    unsafe {
        let list: *mut Object = msg_send![class!(NSLocale), preferredLanguages];
        if list.is_null() {
            return out;
        }
        let count: usize = msg_send![list, count];
        for index in 0..count.min(MAX_LANGUAGES) {
            let entry: *mut Object = msg_send![list, objectAtIndex: index];
            let tag = super::ns_string_to_rust(entry);
            if !tag.is_empty() {
                out.push(tag);
            }
        }
    }
    out
}

/// The language of the phone: the first entry of the person's list that names a language
/// ([`SystemLanguage::from_platform_tag`]), with the right-to-left-ness of the app's `known`
/// languages; `None` when the list is empty or names none, and the style keeps the language it has.
pub(super) fn language_of_phone(known: &[SystemLanguage]) -> Option<SystemLanguage> {
    preferred_languages()
        .iter()
        .find_map(|tag| SystemLanguage::from_platform_tag(tag, known))
}

/// The phone's language, when it is not the one the window's style carries: the style is replaced and
/// the engine does the rest (`PlatformWindow::adopt_system_style` translates the laid-out text again
/// in place, or builds `layout()` again when it read the locale). The caller asks for the regeneration
/// and the frame.
pub(super) fn adopt_language(window: &mut IOSWindow) {
    let known: Vec<SystemLanguage> = window
        .common
        .layout_window
        .as_ref()
        .map(|lw| lw.known_languages.as_ref().to_vec())
        .unwrap_or_default();
    let Some(language) = language_of_phone(&known) else {
        return;
    };
    if window.common.system_style.language == language {
        return;
    }
    let mut style = (*window.common.system_style).clone();
    style.language = language;
    // Contract shape (snapshot -> mutate -> pass): the adoption is the mutation, the caller's
    // regeneration the pass.
    window.snapshot_window_state_baseline("ios.adopt_language");
    let _changed = window.adopt_system_style(Arc::new(style));
}

//! What an Android intent that reached the app asks for, decided without the platform.
//!
//! The shell reads a few strings off the `Intent` (its action, type, data, the URIs of `EXTRA_STREAM` and of its `ClipData`, the
//! text of `EXTRA_TEXT`, its flags) and [`classify`] turns them into the plan the worker carries out: which of them are files
//! to copy, which are links, which is a text, what the sender wants done, and whether it granted the app anything that outlives
//! the activity (a persistable grant) or a write. A launch from the launcher (`ACTION_MAIN`) is NOT an open request.

use alloc::{string::String, vec::Vec};

use azul_core::open_files::{OpenAction, MAX_ITEMS as MAX_ITEMS_PER_REQUEST};

/// `Intent.FLAG_GRANT_READ_URI_PERMISSION`.
pub const FLAG_GRANT_READ_URI_PERMISSION: i32 = 0x1;
/// `Intent.FLAG_GRANT_WRITE_URI_PERMISSION`.
pub const FLAG_GRANT_WRITE_URI_PERMISSION: i32 = 0x2;
/// `Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION`.
pub const FLAG_GRANT_PERSISTABLE_URI_PERMISSION: i32 = 0x40;

/// The fields of an `Intent` the classifier reads.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IntentInfo {
    /// `getAction()`; empty when null.
    pub action: String,
    /// `getType()`; empty when null.
    pub mime: String,
    /// `getDataString()`.
    pub data: Option<String>,
    /// `EXTRA_STREAM`: the one URI of a `SEND`, the list of a `SEND_MULTIPLE`.
    pub streams: Vec<String>,
    /// The URIs of the intent's `ClipData`.
    pub clip_uris: Vec<String>,
    /// `EXTRA_TEXT`.
    pub text: Option<String>,
    /// `getFlags()`.
    pub flags: i32,
}

/// What the worker carries out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenPlan {
    /// What the sender wants done.
    pub action: OpenAction,
    /// The URIs of the files to copy, in order, without repeats.
    pub files: Vec<String>,
    /// Links: a `mailto:`, a `geo:`, a link of an app's own scheme.
    pub links: Vec<String>,
    /// Shared texts.
    pub texts: Vec<String>,
    /// The sender granted the app to WRITE the files back.
    pub write: bool,
    /// The sender offered a grant that outlives the activity: `takePersistableUriPermission` may be asked.
    pub persistable: bool,
    /// How many items beyond [`MAX_ITEMS_PER_REQUEST`] were cut.
    pub truncated: usize,
}

/// The scheme of `text` (`content`, `mailto`), lower case, or `None`.
#[must_use]
pub fn scheme_of(text: &str) -> Option<String> {
    let (scheme, rest) = text.split_once(':')?;
    let mut chars = scheme.chars();
    let valid = chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    (valid && !rest.is_empty()).then(|| scheme.to_ascii_lowercase())
}

fn is_file_scheme(scheme: &str) -> bool {
    matches!(scheme, "content" | "file")
}

/// The plan of `info`, or `None` when it asks nothing of the app (a launch, an action this app does not take, an intent with
/// nothing in it).
#[must_use]
pub fn classify(info: &IntentInfo) -> Option<OpenPlan> {
    const ACTION_VIEW: &str = "android.intent.action.VIEW";
    const ACTION_EDIT: &str = "android.intent.action.EDIT";
    const ACTION_SEND: &str = "android.intent.action.SEND";
    const ACTION_SEND_MULTIPLE: &str = "android.intent.action.SEND_MULTIPLE";
    const ACTION_SENDTO: &str = "android.intent.action.SENDTO";

    let action = match info.action.as_str() {
        ACTION_VIEW => OpenAction::View,
        ACTION_EDIT => OpenAction::Edit,
        ACTION_SEND | ACTION_SEND_MULTIPLE => OpenAction::Send,
        // a `mailto:` link of a mail app's "write to": a link like any other
        ACTION_SENDTO => OpenAction::View,
        _ => return None,
    };
    let mut files: Vec<String> = Vec::new();
    let mut links: Vec<String> = Vec::new();
    let mut texts: Vec<String> = Vec::new();
    let mut add_file = |uri: &str, files: &mut Vec<String>| {
        let uri = uri.trim();
        if !uri.is_empty() && !files.iter().any(|known| known == uri) {
            files.push(uri.to_string());
        }
    };

    if let Some(data) = info.data.as_deref().map(str::trim).filter(|d| !d.is_empty()) {
        match scheme_of(data) {
            Some(scheme) if is_file_scheme(&scheme) => add_file(data, &mut files),
            Some(_) => links.push(data.to_string()),
            None => {}
        }
    }
    for uri in info.streams.iter().chain(&info.clip_uris) {
        // a clip may carry a link (a place shared as geo:...): only content and file URIs are files
        match scheme_of(uri.trim()) {
            Some(scheme) if is_file_scheme(&scheme) => add_file(uri, &mut files),
            Some(_) if info.data.is_none() => {
                if !links.iter().any(|l| l == uri.trim()) {
                    links.push(uri.trim().to_string());
                }
            }
            _ => {}
        }
    }
    if matches!(action, OpenAction::Send) {
        if let Some(text) = info.text.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
            texts.push(text.to_string());
        }
    }

    let total = files.len() + links.len() + texts.len();
    if total == 0 {
        return None;
    }
    // at most MAX_ITEMS_PER_REQUEST items: files first, then links, then texts are cut from the end
    let mut truncated = 0;
    if total > MAX_ITEMS_PER_REQUEST {
        truncated = total - MAX_ITEMS_PER_REQUEST;
        let mut excess = truncated;
        for list in [&mut texts, &mut links, &mut files] {
            let cut = excess.min(list.len());
            list.truncate(list.len() - cut);
            excess -= cut;
        }
    }
    Some(OpenPlan {
        action,
        files,
        links,
        texts,
        write: info.flags & FLAG_GRANT_WRITE_URI_PERMISSION != 0,
        persistable: info.flags & FLAG_GRANT_PERSISTABLE_URI_PERMISSION != 0,
        truncated,
    })
}

/// What a PICKER answered (the result intent of `ACTION_OPEN_DOCUMENT`: one URI in the data, several in the clip data): the files to
/// take, in order, without repeats, at most [`MAX_ITEMS_PER_REQUEST`] (`truncated` counts the rest). `None` when the intent holds
/// no file (a result the picker cancelled). A URI of another scheme is not a file the person picked and is left out.
#[must_use]
pub fn classify_pick(info: &IntentInfo) -> Option<OpenPlan> {
    let mut files: Vec<String> = Vec::new();
    for uri in info.data.iter().chain(&info.clip_uris) {
        let uri = uri.trim();
        let is_file = scheme_of(uri).is_some_and(|scheme| is_file_scheme(&scheme));
        if is_file && !files.iter().any(|known| known == uri) {
            files.push(uri.to_string());
        }
    }
    if files.is_empty() {
        return None;
    }
    let truncated = files.len().saturating_sub(MAX_ITEMS_PER_REQUEST);
    files.truncate(MAX_ITEMS_PER_REQUEST);
    Some(OpenPlan {
        action: OpenAction::View,
        files,
        links: Vec::new(),
        texts: Vec::new(),
        write: info.flags & FLAG_GRANT_WRITE_URI_PERMISSION != 0,
        persistable: info.flags & FLAG_GRANT_PERSISTABLE_URI_PERMISSION != 0,
        truncated,
    })
}

#[cfg(test)]
#[path = "open_intent_test.rs"]
mod open_intent_test;

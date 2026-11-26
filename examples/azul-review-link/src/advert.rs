//! The text a node announces on the local network, so that another AzReview (or the phone) can tell "this is an AzReview of
//! mine, reviewing this project" from anything else that announces itself there. The engine carries it as a UTF-8 string of at
//! most 245 bytes (iroh's local discovery); this module is the FORMAT: std only, no engine types, total in both directions.
//!
//! `azreview/1;p=<project>;n=<display name>;u=<user name>;r=<role>`
//!
//! The version comes first (`azreview/1`: a text of another version is not ours, it is not read); then `key=value` fields
//! separated by `;`; a value escapes `\`, `;` and `=` with a backslash; a field this build does not know is ignored, the first
//! of a key wins, the order is free. `p` is the project: a SHORT HASH of the repository's identity (never its path, see
//! [`project_hash`]) and the only required field. `n` is what the pairing screen shows ("Studio Mac"), `u` the user's name, `r`
//! the role of the announcing screen. Every name is untrusted text from the network: controls, line breaks and the characters
//! that turn text around (bidi overrides and isolates) and the zero-width ones are dropped, the rest is cut to a cap on a
//! character boundary.

use sha2::{Digest, Sha256};

use crate::frame::Role;

/// What every text of this format starts with: the format's name and its version.
pub const ADVERT_PREFIX: &str = "azreview/1";
/// The most a text may be (the engine's limit for what a node announces).
pub const MAX_ADVERT_BYTES: usize = 245;
/// The most a display or user name may be, in bytes, after cleaning.
pub const MAX_ADVERT_NAME_BYTES: usize = 64;
/// How long the project hash [`project_hash`] makes is, in hex digits (8 bytes).
pub const PROJECT_HASH_LEN: usize = 16;
/// The longest project a parser takes, in hex digits.
const MAX_PROJECT_LEN: usize = 32;

/// What a node says about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Advert {
    /// A short hash of the repository's identity: the project this node reviews.
    pub project: String,
    /// What the other side shows for it.
    pub name: String,
    /// The user's name (empty: not said).
    pub user: String,
    /// What the announcing screen is (`None`: not said, or a role this build does not know).
    pub role: Option<Role>,
}

/// The project hash of a repository's identity (its normalised remote, its first commit: whatever the caller keeps stable):
/// the first 8 bytes of its SHA-256, in lower-case hex. It says nothing of where the repository is on a disk.
#[must_use]
pub fn project_hash(identity: &str) -> String {
    let digest = Sha256::digest(identity.as_bytes());
    digest[..PROJECT_HASH_LEN / 2].iter().map(|b| format!("{b:02x}")).collect()
}

/// The word a role has in the text.
fn role_word(role: Role) -> &'static str {
    match role {
        Role::Host => "host",
        Role::Phone => "phone",
        Role::Review => "review",
        Role::ReviewMirror => "mirror",
        Role::Whiteboard => "whiteboard",
    }
}

fn role_of_word(word: &str) -> Option<Role> {
    Some(match word {
        "host" => Role::Host,
        "phone" => Role::Phone,
        "review" => Role::Review,
        "mirror" => Role::ReviewMirror,
        "whiteboard" => Role::Whiteboard,
        _ => return None,
    })
}

/// Whether a character may not be in a name: a control (line breaks and tabs too), a character that turns text around, or one
/// that is there but not seen.
fn is_unwanted(c: char) -> bool {
    c.is_control()
        || matches!(c, '\u{061C}' | '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2064}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}')
}

/// A name as it may be shown: no unwanted character, trimmed, at most [`MAX_ADVERT_NAME_BYTES`] cut on a character boundary.
fn clean(text: &str) -> String {
    let kept: String = text.chars().filter(|c| !is_unwanted(*c)).collect();
    let trimmed = kept.trim();
    let mut out = String::new();
    for c in trimmed.chars() {
        if out.len() + c.len_utf8() > MAX_ADVERT_NAME_BYTES {
            break;
        }
        out.push(c);
    }
    out.trim_end().to_string()
}

/// A value with `\`, `;` and `=` escaped.
fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        if matches!(c, '\\' | ';' | '=') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// The text for `advert`: names cleaned and cut so that the whole is at most [`MAX_ADVERT_BYTES`] (the user's name gives way
/// before the display name does). A project that is no project hash is left out of the text's meaning by the reader: the
/// caller makes it with [`project_hash`].
#[must_use]
pub fn encode(advert: &Advert) -> String {
    let project: String = advert.project.chars().filter(char::is_ascii_hexdigit).map(|c| c.to_ascii_lowercase()).take(MAX_PROJECT_LEN).collect();
    let role = advert.role.map(role_word);
    let fixed = |name: &str, user: &str| {
        let mut text = format!("{ADVERT_PREFIX};p={project}");
        if !name.is_empty() {
            text.push_str(&format!(";n={}", escape(name)));
        }
        if !user.is_empty() {
            text.push_str(&format!(";u={}", escape(user)));
        }
        if let Some(role) = role {
            text.push_str(&format!(";r={role}"));
        }
        text
    };
    let (mut name, mut user) = (clean(&advert.name), clean(&advert.user));
    // the user's name gives way first, then the display name, a character at a time from the end
    while fixed(&name, &user).len() > MAX_ADVERT_BYTES && !user.is_empty() {
        user.pop();
        user = user.trim_end().to_string();
    }
    while fixed(&name, &user).len() > MAX_ADVERT_BYTES && !name.is_empty() {
        name.pop();
        name = name.trim_end().to_string();
    }
    fixed(&name, &user)
}

/// The fields of a text after its prefix, with the escapes undone; `None` for a broken escape.
fn fields(rest: &str) -> Option<Vec<(String, String)>> {
    let mut out = Vec::new();
    let (mut key, mut value, mut in_value, mut escaped) = (String::new(), String::new(), false, false);
    let mut push = |key: &mut String, value: &mut String, in_value: &mut bool| {
        if !key.is_empty() && *in_value {
            out.push((std::mem::take(key), std::mem::take(value)));
        } else {
            key.clear();
            value.clear();
        }
        *in_value = false;
    };
    for c in rest.chars() {
        if escaped {
            if in_value {
                value.push(c);
            } else {
                key.push(c);
            }
            escaped = false;
            continue;
        }
        match c {
            '\\' => escaped = true,
            ';' => push(&mut key, &mut value, &mut in_value),
            '=' if !in_value => in_value = true,
            _ if in_value => value.push(c),
            _ => key.push(c),
        }
    }
    if escaped {
        return None;
    }
    push(&mut key, &mut value, &mut in_value);
    Some(out)
}

/// The advert in `text`, or `None` for a text that is not one of this format and version (foreign, another version, over the
/// cap, no project, a project that is no hash, a broken escape).
#[must_use]
pub fn parse(text: &str) -> Option<Advert> {
    if text.len() > MAX_ADVERT_BYTES {
        return None;
    }
    let rest = text.strip_prefix(ADVERT_PREFIX)?;
    let rest = match rest.strip_prefix(';') {
        Some(rest) => rest,
        None if rest.is_empty() => return None,
        None => return None,
    };
    let mut advert = Advert { project: String::new(), name: String::new(), user: String::new(), role: None };
    let (mut has_project, mut has_name, mut has_user, mut has_role) = (false, false, false, false);
    for (key, value) in fields(rest)? {
        match key.as_str() {
            "p" if !has_project => {
                has_project = true;
                let hash = !value.is_empty() && value.len() <= MAX_PROJECT_LEN && value.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f'));
                if !hash {
                    return None;
                }
                advert.project = value;
            }
            "n" if !has_name => {
                has_name = true;
                advert.name = clean(&value);
            }
            "u" if !has_user => {
                has_user = true;
                advert.user = clean(&value);
            }
            "r" if !has_role => {
                has_role = true;
                advert.role = role_of_word(&value);
            }
            _ => {}
        }
    }
    has_project.then_some(advert)
}

#[cfg(test)]
#[path = "advert_test.rs"]
mod advert_test;

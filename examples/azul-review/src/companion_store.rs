//! What the phone companion keeps on this computer: whether it is on, the key this computer is known by to the phones, and the
//! phones that were paired with it. Pure: the text of the file and what it makes, no window and no network.
//!
//! The file is `<data root>/.azlin/companion/<app folder>.json`: the data root's own bookkeeping folder, which belongs to this
//! computer and which a drive never lists, so the key (a secret) and the trust records (who may command this computer) never travel
//! with the user's files to another computer or a bucket. It is written for the user only (`companion_io::write_private`).
//!
//! Reading never fails: a file that is not there is the default (the companion is OFF), one that is damaged is the default and says
//! what was wrong, a key that is no key is dropped (a new one is made at the next start, and the phones pair again), phones past
//! the cap are left out and said.

use std::path::{Path, PathBuf};

use azul_review_link::{
    access::TrustRecord,
    advert::{self, Advert},
    frame::Role,
    limits::{cut_bytes, MAX_NAME_BYTES, MAX_PAIRED_PHONES},
};
use serde::{Deserialize, Serialize};

/// The version of the file's layout this build writes.
pub const FILE_VERSION: u32 = 1;
/// The most bytes of the file that are read: a key and four phones are a few hundred.
pub const MAX_FILE_BYTES: u64 = 64 * 1024;
/// The bytes of the key (the endpoint's identity).
pub const KEY_BYTES: usize = 32;
/// The most characters of a phone's key (the transport's id, lowercase hex: 64).
pub const MAX_PEER_ID_BYTES: usize = 128;
/// What this computer is called to the phones until the user says another name.
pub const DEFAULT_NAME: &str = "AzReview";
/// What the project of the announcement is made from: the announcement says that this is an AzReview host and its name, and no
/// folder, no project, no user and no key. (The phones find a computer by the key they were paired with.)
const ADVERT_PROJECT_WORD: &str = "azreview-companion-host";

/// What is kept.
#[derive(Clone, PartialEq, Eq, Default)]
pub struct Stored {
    /// The user turned the companion on: it listens (after the first frame) at every start until it is turned off.
    pub enabled: bool,
    key: Option<[u8; KEY_BYTES]>,
    /// The paired phones, at most [`MAX_PAIRED_PHONES`].
    pub phones: Vec<TrustRecord>,
}

// The key is a secret: a log line, a failed assert or a panic message never says it.
impl std::fmt::Debug for Stored {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Stored")
            .field("enabled", &self.enabled)
            .field("key", &if self.key.is_some() { "<kept>" } else { "<none>" })
            .field("phones", &self.phones.len())
            .finish()
    }
}

/// The file as it is written.
#[derive(Debug, Default, Serialize, Deserialize)]
struct FileText {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    enabled: bool,
    /// The key, 64 lowercase hex digits (empty: none yet).
    #[serde(default)]
    key: String,
    #[serde(default)]
    phones: Vec<PhoneText>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct PhoneText {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
}

impl Stored {
    /// The key this computer is known by (`None` until the first start made one).
    #[must_use]
    pub fn key(&self) -> Option<[u8; KEY_BYTES]> {
        self.key
    }

    /// Keeps the key the endpoint made.
    pub fn set_key(&mut self, key: [u8; KEY_BYTES]) {
        self.key = Some(key);
    }

    /// Reads the file's text. Never fails: what cannot be read is the default, and the second value says what was wrong.
    #[must_use]
    pub fn parse(text: &str) -> (Stored, Vec<String>) {
        if text.trim().is_empty() {
            return (Stored::default(), Vec::new());
        }
        let file: FileText = match serde_json::from_str(text) {
            Ok(file) => file,
            Err(e) => return (Stored::default(), vec![format!("the companion's file is not readable and is left aside: {e}")]),
        };
        let mut problems = Vec::new();
        if file.version > FILE_VERSION {
            problems.push(format!("the companion's file is from a newer AzReview (version {}): what this one knows of it is read", file.version));
        }
        let key = if file.key.is_empty() {
            None
        } else {
            let key = from_hex_key(&file.key);
            if key.is_none() {
                problems.push("the key in the companion's file is damaged: a new one is made, and the phones are paired again".to_string());
            }
            key
        };
        let mut stored = Stored { enabled: file.enabled, key, phones: Vec::new() };
        for phone in file.phones {
            if !is_peer_id(&phone.id) {
                problems.push("a phone in the companion's file has no usable key and is left out".to_string());
                continue;
            }
            if stored.phones.iter().any(|p| p.peer == phone.id) {
                continue;
            }
            if stored.phones.len() >= MAX_PAIRED_PHONES {
                problems.push(format!("the companion's file names more than {MAX_PAIRED_PHONES} phones: the rest are left out"));
                break;
            }
            stored.phones.push(TrustRecord::phone(&phone.id, &clean_name(&phone.name)));
        }
        (stored, problems)
    }

    /// The file's text: pretty-printed JSON, the keys in a fixed order, a final newline.
    #[must_use]
    pub fn to_json(&self) -> String {
        let file = FileText {
            version: FILE_VERSION,
            enabled: self.enabled,
            key: self.key.map(|k| to_hex(&k)).unwrap_or_default(),
            phones: self.phones.iter().map(|p| PhoneText { id: p.peer.clone(), name: p.name.clone() }).collect(),
        };
        let mut text = serde_json::to_string_pretty(&file).unwrap_or_else(|_| "{}".to_string());
        text.push('\n');
        text
    }

    /// Keeps the phones the host says are paired (at most [`MAX_PAIRED_PHONES`]; a name is cleaned and cut).
    pub fn set_phones(&mut self, records: &[TrustRecord]) {
        self.phones = records
            .iter()
            .filter(|r| is_peer_id(&r.peer))
            .take(MAX_PAIRED_PHONES)
            .map(|r| TrustRecord::phone(&r.peer, &clean_name(&r.name)))
            .collect();
    }
}

/// A phone's name as it is kept and shown: no control character, at most [`MAX_NAME_BYTES`] bytes (cut on a character boundary).
#[must_use]
pub fn clean_name(name: &str) -> String {
    let plain: String = name.chars().filter(|c| !c.is_control()).collect();
    cut_bytes(plain.trim(), MAX_NAME_BYTES, "phone name").0
}

/// Whether `id` can be a phone's key as the transport says it: not empty, not long, letters, digits, dash and underscore.
#[must_use]
pub fn is_peer_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= MAX_PEER_ID_BYTES && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Lowercase hex of `bytes`.
#[must_use]
pub fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The 32 bytes of 64 hex digits (either case); `None` for anything else.
#[must_use]
pub fn from_hex_key(text: &str) -> Option<[u8; KEY_BYTES]> {
    let text = text.trim();
    if text.len() != KEY_BYTES * 2 || !text.is_ascii() {
        return None;
    }
    let mut key = [0u8; KEY_BYTES];
    for (i, byte) in key.iter_mut().enumerate() {
        *byte = u8::from_str_radix(text.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(key)
}

/// The text this computer announces on the local network: that it is an AzReview host, and its name. It carries no key, no secret,
/// no folder, no project and no user name (the cap of the engine, 245 bytes, is held by the encoder).
#[must_use]
pub fn advertised(name: &str) -> String {
    advert::encode(&Advert { project: advert::project_hash(ADVERT_PROJECT_WORD), name: clean_name(name), user: String::new(), role: Some(Role::Host) })
}

/// The file of this computer's companion under `data_root`: in the data root's bookkeeping folder, the one that stays on this
/// computer.
#[must_use]
pub fn store_path(data_root: &Path, app_folder: &str) -> PathBuf {
    // (the window's place is kept in the same folder: `<data root>/<bookkeeping>/windows/<app>.json`)
    let place = azul_appkit::window_place::place_path(data_root, app_folder);
    let bookkeeping = place.parent().and_then(Path::parent).map_or_else(|| data_root.join(".azlin"), Path::to_path_buf);
    bookkeeping.join("companion").join(format!("{app_folder}.json"))
}

#[cfg(test)]
#[path = "companion_store_test.rs"]
mod companion_store_test;

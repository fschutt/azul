//! The zstd dictionaries shipped inside the crate, for small text files.
//!
//! A small file compressed alone has no history to find repeats in; a dictionary gives zstd
//! one: common strings of its kind (JSON keys, XML tags, keywords). They are trained on PUBLIC
//! corpora only (see the README for the sources) and are static: a dictionary contains pieces
//! of its training data, so one trained on a user's files would leak them to everyone who reads
//! a file compressed with it. These carry nothing of anybody's drive.
//!
//! Every dictionary is named by its id in each segment that uses it (codec byte 4). An id is
//! never reused and a shipped file is never changed: a better dictionary gets a new id, and the
//! old one stays here so that every segment ever written still decodes. `tests/dicts.rs` pins
//! each file's SHA-256.

use crate::detect::Kind;

/// A dictionary's id, as segments name it.
pub type DictId = u16;

/// One shipped dictionary.
#[derive(Debug)]
pub struct Dictionary {
    pub id: DictId,
    /// The kind it is trained for, and used for when it is the newest of that kind.
    pub kind: Kind,
    /// Its file name in `dicts/`.
    pub name: &'static str,
    pub bytes: &'static [u8],
}

/// Every dictionary ever shipped, oldest first. Trained by `tools/train_dicts.py`.
const DICTIONARIES: &[Dictionary] = &[
    Dictionary {
        id: 1,
        kind: Kind::Json,
        name: "json-1.zdict",
        bytes: include_bytes!("../dicts/json-1.zdict"),
    },
    Dictionary {
        id: 2,
        kind: Kind::Markup,
        name: "markup-1.zdict",
        bytes: include_bytes!("../dicts/markup-1.zdict"),
    },
    Dictionary {
        id: 3,
        kind: Kind::Text,
        name: "text-1.zdict",
        bytes: include_bytes!("../dicts/text-1.zdict"),
    },
    Dictionary {
        id: 4,
        kind: Kind::Code,
        name: "code-1.zdict",
        bytes: include_bytes!("../dicts/code-1.zdict"),
    },
];

/// Every dictionary ever shipped.
#[must_use]
pub fn all() -> &'static [Dictionary] {
    DICTIONARIES
}

/// The dictionary `id`.
#[must_use]
pub fn by_id(id: DictId) -> Option<&'static Dictionary> {
    DICTIONARIES.iter().find(|d| d.id == id)
}

/// The dictionary new files of `kind` are compressed with: the newest of its kind.
#[must_use]
pub fn for_kind(kind: Kind) -> Option<DictId> {
    DICTIONARIES
        .iter()
        .filter(|d| d.kind == kind)
        .map(|d| d.id)
        .next_back()
}

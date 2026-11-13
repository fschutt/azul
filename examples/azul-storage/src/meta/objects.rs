//! git objects, byte for byte as git writes them in a repository of the
//! `sha256` object format (`extensions.objectFormat = sha256`, git 2.29 and
//! later): blobs, trees and commits, each named by the SHA-256 of
//! `"<kind> <length>\0" + body`.
//!
//! Byte-identical objects are what keeps `git-remote-azlin` simple: it hands
//! them to git as they are. Tags, symbolic links and submodules are not used
//! by a drive index and not written.

use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
    fmt,
    sync::Arc,
};

use sha2::{Digest, Sha256};

use super::{from_hex, hex, MetaError};

/// A git object id in the `sha256` object format.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjectId(pub [u8; 32]);

impl ObjectId {
    /// The id git gives `body` as an object of `kind`.
    #[must_use]
    pub fn of(kind: Kind, body: &[u8]) -> ObjectId {
        let mut hasher = Sha256::new();
        hasher.update(format!("{} {}\0", kind.name(), body.len()).as_bytes());
        hasher.update(body);
        let mut id = [0u8; 32];
        id.copy_from_slice(&hasher.finalize());
        ObjectId(id)
    }

    /// 64 lowercase hex digits, as git shows it.
    #[must_use]
    pub fn to_hex(&self) -> String {
        hex(&self.0)
    }

    /// The id of 64 hex digits; `None` for anything else.
    #[must_use]
    pub fn from_hex(text: &str) -> Option<ObjectId> {
        let bytes = from_hex(text)?;
        let id: [u8; 32] = bytes.try_into().ok()?;
        Some(ObjectId(id))
    }
}

impl fmt::Debug for ObjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ObjectId({})", self.to_hex())
    }
}

impl fmt::Display for ObjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

/// The kinds of object a drive index writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Commit,
    Tree,
    Blob,
}

impl Kind {
    /// The name in the object header: `commit`, `tree`, `blob`.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Kind::Commit => "commit",
            Kind::Tree => "tree",
            Kind::Blob => "blob",
        }
    }

    /// git's pack type number: commit 1, tree 2, blob 3.
    #[must_use]
    pub fn code(self) -> u8 {
        match self {
            Kind::Commit => 1,
            Kind::Tree => 2,
            Kind::Blob => 3,
        }
    }

    #[must_use]
    pub fn from_code(code: u8) -> Option<Kind> {
        match code {
            1 => Some(Kind::Commit),
            2 => Some(Kind::Tree),
            3 => Some(Kind::Blob),
            _ => None,
        }
    }
}

/// What a tree entry is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mode {
    /// A file (`100644`): in a drive index, a pointer file or a small inline file.
    File,
    /// A folder (`40000`).
    Tree,
}

impl Mode {
    /// The mode as git writes it in a tree.
    #[must_use]
    pub fn text(self) -> &'static str {
        match self {
            Mode::File => "100644",
            Mode::Tree => "40000",
        }
    }

    fn parse(text: &[u8]) -> Option<Mode> {
        match text {
            b"100644" | b"100755" => Some(Mode::File),
            b"40000" => Some(Mode::Tree),
            _ => None,
        }
    }
}

/// One entry of a tree: a file or a folder.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TreeEntry {
    /// One path segment: not empty, no `/`, no NUL, not `.` or `..`.
    pub name: String,
    pub mode: Mode,
    pub id: ObjectId,
}

/// git's order of tree entries: by name, a folder's name compared as if it
/// ended in `/` (so `a.b` comes before the folder `a`, the file `a` before
/// `a.b`).
#[must_use]
pub fn git_order(a: &TreeEntry, b: &TreeEntry) -> Ordering {
    let terminator = |e: &TreeEntry| if e.mode == Mode::Tree { b'/' } else { 0 };
    let (an, bn) = (a.name.as_bytes(), b.name.as_bytes());
    let common = an.len().min(bn.len());
    match an[..common].cmp(&bn[..common]) {
        Ordering::Equal => {}
        other => return other,
    }
    let ac = an.get(common).copied().unwrap_or_else(|| terminator(a));
    let bc = bn.get(common).copied().unwrap_or_else(|| terminator(b));
    ac.cmp(&bc).then_with(|| an.len().cmp(&bn.len()))
}

/// Whether `name` can be one entry of a tree.
#[must_use]
pub fn is_valid_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\0'])
}

/// A folder: its entries in git's order, names unique.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tree {
    entries: Vec<TreeEntry>,
}

impl Tree {
    #[must_use]
    pub fn new() -> Self {
        Tree::default()
    }

    /// The entries, in git's order.
    #[must_use]
    pub fn entries(&self) -> &[TreeEntry] {
        &self.entries
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The entry called `name`.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&TreeEntry> {
        self.entries.iter().find(|e| e.name == name)
    }

    /// The tree of these entries, put in git's order; refuses an invalid or a
    /// repeated name.
    pub fn from_entries(mut entries: Vec<TreeEntry>) -> Result<Tree, MetaError> {
        if let Some(bad) = entries.iter().find(|e| !is_valid_name(&e.name)) {
            return Err(MetaError::Corrupt {
                key: bad.name.clone(),
                reason: "not a valid name in a folder".to_string(),
            });
        }
        // A file and a folder of one name are not neighbours in git's order.
        {
            let mut names = HashSet::with_capacity(entries.len());
            if let Some(twice) = entries.iter().find(|e| !names.insert(e.name.as_str())) {
                return Err(MetaError::Corrupt {
                    key: twice.name.clone(),
                    reason: "two entries of one name in a folder".to_string(),
                });
            }
        }
        entries.sort_by(git_order);
        Ok(Tree { entries })
    }

    /// Adds the entry, replacing one of the same name. Refuses an invalid name.
    pub fn insert(&mut self, entry: TreeEntry) -> Result<(), MetaError> {
        if !is_valid_name(&entry.name) {
            return Err(MetaError::Corrupt {
                key: entry.name.clone(),
                reason: "not a valid name in a folder".to_string(),
            });
        }
        self.entries.retain(|e| e.name != entry.name);
        let at = self
            .entries
            .partition_point(|e| git_order(e, &entry) == Ordering::Less);
        self.entries.insert(at, entry);
        Ok(())
    }

    /// Removes the entry called `name`; whether there was one.
    pub fn remove(&mut self, name: &str) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| e.name != name);
        self.entries.len() != before
    }

    /// The body git stores: per entry `"<mode> <name>\0"` and the 32 id bytes.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.entries.len() * 64);
        for e in &self.entries {
            out.extend_from_slice(e.mode.text().as_bytes());
            out.push(b' ');
            out.extend_from_slice(e.name.as_bytes());
            out.push(0);
            out.extend_from_slice(&e.id.0);
        }
        out
    }

    /// The tree of a body [`Tree::encode`] (or git) wrote.
    pub fn decode(body: &[u8]) -> Result<Tree, String> {
        let mut entries: Vec<TreeEntry> = Vec::new();
        let mut rest = body;
        while !rest.is_empty() {
            let space = rest
                .iter()
                .position(|&b| b == b' ')
                .ok_or("a tree entry without a mode")?;
            let mode = Mode::parse(&rest[..space]).ok_or("a tree entry of an unknown mode")?;
            rest = &rest[space + 1..];
            let nul = rest
                .iter()
                .position(|&b| b == 0)
                .ok_or("a tree entry without the end of its name")?;
            let name = std::str::from_utf8(&rest[..nul])
                .map_err(|_| "a tree entry whose name is not UTF-8")?
                .to_string();
            rest = &rest[nul + 1..];
            if rest.len() < 32 {
                return Err("a tree entry cut off in its id".to_string());
            }
            let mut id = [0u8; 32];
            id.copy_from_slice(&rest[..32]);
            rest = &rest[32..];
            if !is_valid_name(&name) {
                return Err(format!("a tree entry with the invalid name {name:?}"));
            }
            let entry = TreeEntry {
                name,
                mode,
                id: ObjectId(id),
            };
            if let Some(last) = entries.last() {
                if git_order(last, &entry) != Ordering::Less {
                    return Err("tree entries out of order or repeated".to_string());
                }
            }
            entries.push(entry);
        }
        Ok(Tree { entries })
    }
}

/// Who made a commit, and when.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    /// The device's name (no `<`, `>` or line break; they are dropped when written).
    pub name: String,
    /// The member's address or the device id (same rule).
    pub email: String,
    /// Seconds since 1970-01-01 UTC.
    pub time: i64,
    /// The local time's offset from UTC in minutes (git writes it as `+hhmm`).
    pub offset_minutes: i32,
}

fn clean(text: &str) -> String {
    text.chars()
        .filter(|c| !matches!(c, '<' | '>' | '\n' | '\r' | '\0'))
        .collect::<String>()
        .trim()
        .to_string()
}

impl Signature {
    fn encode(&self) -> String {
        let sign = if self.offset_minutes < 0 { '-' } else { '+' };
        let offset = self.offset_minutes.unsigned_abs();
        format!(
            "{} <{}> {} {sign}{:02}{:02}",
            clean(&self.name),
            clean(&self.email),
            self.time,
            offset / 60,
            offset % 60
        )
    }

    fn decode(text: &str) -> Option<Signature> {
        let open = text.find('<')?;
        let close = open + text[open..].find('>')?;
        let name = text[..open].trim().to_string();
        let email = text[open + 1..close].to_string();
        let mut rest = text[close + 1..].split_whitespace();
        let time = rest.next()?.parse::<i64>().ok()?;
        let zone = rest.next()?;
        if zone.len() != 5 || !zone.is_ascii() {
            return None;
        }
        let (sign, digits) = zone.split_at(1);
        let hours = digits[..2].parse::<i32>().ok()?;
        let minutes = digits[2..].parse::<i32>().ok()?;
        let offset = hours * 60 + minutes;
        let offset_minutes = match sign {
            "+" => offset,
            "-" => -offset,
            _ => return None,
        };
        Some(Signature {
            name,
            email,
            time,
            offset_minutes,
        })
    }
}

/// A state of the drive: its root folder, the states it came from, who made
/// it and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    pub tree: ObjectId,
    pub parents: Vec<ObjectId>,
    pub author: Signature,
    pub committer: Signature,
    pub message: String,
}

impl Commit {
    /// The body git stores.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = format!("tree {}\n", self.tree.to_hex());
        for parent in &self.parents {
            out.push_str(&format!("parent {}\n", parent.to_hex()));
        }
        out.push_str(&format!("author {}\n", self.author.encode()));
        out.push_str(&format!("committer {}\n", self.committer.encode()));
        out.push('\n');
        out.push_str(&self.message);
        out.into_bytes()
    }

    /// The commit of a body [`Commit::encode`] (or git) wrote. Headers it does
    /// not know (`gpgsig`, `encoding`, ...) are skipped.
    pub fn decode(body: &[u8]) -> Result<Commit, String> {
        let text = std::str::from_utf8(body).map_err(|_| "a commit that is not UTF-8")?;
        let (headers, message) = text.split_once("\n\n").unwrap_or((text, ""));
        let mut tree = None;
        let mut parents = Vec::new();
        let mut author = None;
        let mut committer = None;
        for line in headers.lines() {
            let (field, value) = line.split_once(' ').unwrap_or((line, ""));
            match field {
                "tree" => tree = ObjectId::from_hex(value),
                "parent" => parents
                    .push(ObjectId::from_hex(value).ok_or("a commit with an invalid parent")?),
                "author" => author = Signature::decode(value),
                "committer" => committer = Signature::decode(value),
                _ => {}
            }
        }
        Ok(Commit {
            tree: tree.ok_or("a commit without a tree")?,
            parents,
            author: author.ok_or("a commit without an author")?,
            committer: committer.ok_or("a commit without a committer")?,
            message: message.to_string(),
        })
    }
}

/// The objects a device holds: every object of the packs it has read, and the
/// ones it wrote since. Content-addressed: inserting an object twice keeps one.
#[derive(Debug, Clone, Default)]
pub struct Objects {
    map: HashMap<ObjectId, (Kind, Arc<Vec<u8>>)>,
}

fn missing(id: &ObjectId) -> MetaError {
    MetaError::MissingObject { id: id.to_hex() }
}

fn corrupt(id: &ObjectId, reason: impl Into<String>) -> MetaError {
    MetaError::Corrupt {
        key: id.to_hex(),
        reason: reason.into(),
    }
}

impl Objects {
    #[must_use]
    pub fn new() -> Self {
        Objects::default()
    }

    /// Stores the object; its id.
    pub fn insert(&mut self, kind: Kind, body: Vec<u8>) -> ObjectId {
        let id = ObjectId::of(kind, &body);
        self.map.entry(id).or_insert_with(|| (kind, Arc::new(body)));
        id
    }

    /// Stores an object that arrived with its id (from a pack): refused when the
    /// id is not the object's.
    pub fn insert_checked(&mut self, id: ObjectId, kind: Kind, body: Vec<u8>) -> Result<(), MetaError> {
        if ObjectId::of(kind, &body) != id {
            return Err(corrupt(&id, "the object's bytes do not have its id"));
        }
        self.map.entry(id).or_insert_with(|| (kind, Arc::new(body)));
        Ok(())
    }

    #[must_use]
    pub fn contains(&self, id: &ObjectId) -> bool {
        self.map.contains_key(id)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// The object's kind and body.
    #[must_use]
    pub fn get(&self, id: &ObjectId) -> Option<(Kind, &[u8])> {
        self.map.get(id).map(|(kind, body)| (*kind, body.as_slice()))
    }

    /// Every id, in no order.
    pub fn ids(&self) -> impl Iterator<Item = &ObjectId> {
        self.map.keys()
    }

    pub fn write_blob(&mut self, bytes: &[u8]) -> ObjectId {
        self.insert(Kind::Blob, bytes.to_vec())
    }

    pub fn write_tree(&mut self, tree: &Tree) -> ObjectId {
        self.insert(Kind::Tree, tree.encode())
    }

    pub fn write_commit(&mut self, commit: &Commit) -> ObjectId {
        self.insert(Kind::Commit, commit.encode())
    }

    fn body_of(&self, id: &ObjectId, kind: Kind) -> Result<&[u8], MetaError> {
        match self.get(id) {
            None => Err(missing(id)),
            Some((found, body)) if found == kind => Ok(body),
            Some((found, _)) => Err(corrupt(
                id,
                format!("a {} where a {} was expected", found.name(), kind.name()),
            )),
        }
    }

    pub fn blob(&self, id: &ObjectId) -> Result<&[u8], MetaError> {
        self.body_of(id, Kind::Blob)
    }

    pub fn tree(&self, id: &ObjectId) -> Result<Tree, MetaError> {
        Tree::decode(self.body_of(id, Kind::Tree)?).map_err(|reason| corrupt(id, reason))
    }

    pub fn commit(&self, id: &ObjectId) -> Result<Commit, MetaError> {
        Commit::decode(self.body_of(id, Kind::Commit)?).map_err(|reason| corrupt(id, reason))
    }
}

//! A board as a file to share, and a file of another's board to open: [`export`] writes the `azboard/1` document
//! ([`crate::whiteboard_azboard`]) to a path the person chose, with its pictures inside it (`data:` addresses) or in a
//! folder beside it; [`import`] reads such a file back. Std only - the disk, `git` (once, for the remote and whether the tree
//! has changes) and nothing else; the PNG codec is the caller's. Both run on a Thread of their own (git can be slow).
//!
//! What a file is written with is told to the person before: the author name (the setting, never an e-mail address) and
//! where the pictures go. What is read is untrusted: the file is bounded ([`MAX_IMPORT_BYTES`]), a picture beside it is
//! only taken from the folder the file lies in and below, a path in it that leaves the workspace is left out.

use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use crate::{
    whiteboard::{BoardData, Limits},
    whiteboard_azboard::{self as azboard, Meta, Pictures},
    whiteboard_git::{self as git, RepoFacts},
    whiteboard_history::History,
    whiteboard_store::{self as store, ImageCodec, Loaded, StoreError},
};

/// The largest file that is imported, bytes: a board of the largest size with every picture in the clear is far below it.
pub const MAX_IMPORT_BYTES: u64 = 256 * 1024 * 1024;
/// How long git is waited for, at most, for each of the two questions asked of it.
pub const GIT_TIMEOUT: Duration = Duration::from_secs(8);

/// Where the pictures of an exported file go.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExportPictures {
    /// In the file itself, as `data:` addresses (the default: one file to hand on).
    #[default]
    Embedded,
    /// In a folder beside the file, named for it.
    Beside,
}

/// How much of a board's history an export holds.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ExportScope {
    /// The board as it is and all its snapshots, in order (the default).
    #[default]
    History,
    /// The board as it is, with no history.
    Board,
    /// One snapshot: its state is the document's, and it is the only one in the document's history.
    Snapshot(String),
}

/// What an export is given.
#[derive(Debug, Clone, PartialEq)]
pub struct ExportRequest {
    /// The board as it is.
    pub data: BoardData,
    /// Its snapshots (what [`Self::scope`] holds of them is written).
    pub history: History,
    pub scope: ExportScope,
    /// Which board it is and what the app knows of its repository and author.
    pub meta: Meta,
    pub pictures: ExportPictures,
    /// The workspace's folder: the repository is looked at from here, now.
    pub folder: PathBuf,
    /// Where the global git config is (`~`), for the name.
    pub home: Option<PathBuf>,
    /// The remote the links are made from, when the person chose one (`origin`, else the first, otherwise).
    pub remote: Option<String>,
}

/// What an export did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exported {
    pub path: PathBuf,
    /// The folder the pictures went to ([`ExportPictures::Beside`]).
    pub pictures_folder: Option<PathBuf>,
    /// The size of the file, bytes.
    pub bytes: u64,
    /// How many pictures the file has.
    pub pictures: usize,
    /// The links of the file name this remote.
    pub remote: Option<String>,
    /// How many code references have a link.
    pub links: usize,
}

/// The name a file is proposed under: the board's name with what a file name does not like turned to `-`, and
/// `.azboard.json`.
#[must_use]
pub fn suggested_name(board_name: &str) -> String {
    let mut slug = String::new();
    for c in board_name.chars() {
        let keep = if c.is_alphanumeric() || matches!(c, '_' | '.') { c } else { '-' };
        if keep == '-' && (slug.is_empty() || slug.ends_with('-')) {
            continue;
        }
        slug.push(keep);
        if slug.chars().count() >= 60 {
            break;
        }
    }
    let slug = slug.trim_matches(|c| c == '-' || c == '.');
    format!("{}.azboard.json", if slug.is_empty() { "board" } else { slug })
}

/// The folder of the pictures of the file `name`: `Parser.azboard.json` has `Parser.assets`.
#[must_use]
pub fn pictures_folder_name(name: &str) -> String {
    let stem = name.strip_suffix(".json").unwrap_or(name);
    let stem = stem.strip_suffix(".azboard").unwrap_or(stem);
    format!("{}.assets", if stem.is_empty() { "board" } else { stem })
}

/// Writes `bytes` to `path` the way a board file is kept: the folder is made if need be, the bytes go to a file of
/// their own beside it and that is renamed over `path`, so that the file is the old one or the new one, never half of one.
///
/// # Errors
/// What the file system said.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("board");
    let beside = path.with_file_name(format!(".{name}.tmp"));
    fs::write(&beside, bytes).map_err(|e| e.to_string())?;
    fs::rename(&beside, path).map_err(|e| {
        let _gone = fs::remove_file(&beside);
        e.to_string()
    })
}

/// The repository's facts as they are now: the files of `.git` ([`git::read`]), the remote git itself says
/// ([`git::remotes`], one `git remote -v`) and whether the tree has changes ([`git::dirty`], one `git status`). `known`
/// (what the app had) is kept for what git does not give, and when there is no repository at all.
#[must_use]
pub fn repo_now(req: &ExportRequest, timeout: Duration) -> (Option<RepoFacts>, Option<String>) {
    let Some(mut facts) = git::read(&req.folder, req.home.as_deref()) else {
        return (req.meta.repo.clone(), None);
    };
    let remotes = git::remotes(&req.folder, timeout);
    let chosen = git::choose_remote(&remotes, req.remote.as_deref()).cloned();
    if let Some((_, url)) = &chosen {
        facts.remote_url = Some(url.clone());
        match git::web_address(url) {
            Some((web, host)) => {
                facts.web_url = Some(web);
                facts.host = host;
            }
            None => {
                facts.web_url = None;
                facts.host = git::Host::Other;
            }
        }
    }
    facts.dirty = git::dirty(&req.folder, timeout);
    (Some(facts), chosen.map(|(name, _)| name))
}

/// Writes the board of `req` to `target` (a `.azboard.json` the person chose), with its pictures beside it when asked.
/// The pictures' folder is written first and the file last, so that the file is never there without them.
///
/// # Errors
/// What the file system said, as a sentence.
pub fn export(req: ExportRequest, target: &Path, timeout: Duration) -> Result<Exported, String> {
    let (repo, remote) = repo_now(&req, timeout);
    let meta = Meta { repo, ..req.meta };
    let file_name = target.file_name().and_then(|n| n.to_str()).ok_or_else(|| "the file has no name".to_string())?;
    let folder_name = pictures_folder_name(file_name);
    let pictures = match req.pictures {
        ExportPictures::Embedded => Pictures::Embedded,
        ExportPictures::Beside => Pictures::Beside(folder_name.clone()),
    };
    let written = match &req.scope {
        ExportScope::History => azboard::document_with(&req.data, &meta, &pictures, &req.history),
        ExportScope::Board => azboard::document(&req.data, &meta, &pictures),
        ExportScope::Snapshot(id) => {
            let snapshot = req.history.get(id).ok_or_else(|| format!("there is no snapshot {id}"))?;
            azboard::document_with(&snapshot.data, &meta, &pictures, &History { snapshots: vec![snapshot.clone()] })
        }
    };
    let links = written.text.matches("\"permalink\"").count();
    let mut pictures_folder = None;
    if req.pictures == ExportPictures::Beside && !written.pictures.is_empty() {
        let dir = target.with_file_name(&folder_name);
        for p in &written.pictures {
            write_atomic(&dir.join(&p.name), &p.bytes)?;
        }
        pictures_folder = Some(dir);
    }
    write_atomic(target, written.text.as_bytes())?;
    Ok(Exported {
        path: target.to_path_buf(),
        pictures_folder,
        bytes: written.text.len() as u64,
        pictures: written.assets,
        remote,
        links,
    })
}

/// A picture beside `file`, by the name the document gives it: only a file in the folder `file` lies in, or below it
/// (a link that leads out of it, as a symbolic link can, is not followed), not larger than a picture may be.
fn beside(dir: &Path, name: &str) -> Option<Vec<u8>> {
    if !azboard::safe_relative(name) {
        return None;
    }
    let root = fs::canonicalize(dir).ok()?;
    let path = fs::canonicalize(root.join(name)).ok()?;
    if !path.starts_with(&root) {
        return None;
    }
    let meta = fs::metadata(&path).ok()?;
    if !meta.is_file() || meta.len() > store::MAX_PICTURE_BYTES as u64 {
        return None;
    }
    fs::read(path).ok()
}

/// The board in `file`, an `azboard/1` document of any size up to [`MAX_IMPORT_BYTES`], with the pictures it names
/// beside it.
///
/// # Errors
/// The file cannot be read, is no board of this version, is of a newer one, or is larger than a board file is.
pub fn import(file: &Path, codec: &dyn ImageCodec, limits: &Limits) -> Result<Loaded, StoreError> {
    let meta = fs::metadata(file).map_err(|e| StoreError::NotABoard(format!("it cannot be read: {e}")))?;
    if !meta.is_file() {
        return Err(StoreError::NotABoard("it is not a file".to_string()));
    }
    if meta.len() > MAX_IMPORT_BYTES {
        return Err(StoreError::TooLarge { bytes: usize::try_from(meta.len()).unwrap_or(usize::MAX) });
    }
    let bytes = fs::read(file).map_err(|e| StoreError::NotABoard(format!("it cannot be read: {e}")))?;
    let dir = file.parent().map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    store::read_capped(&bytes, &|name| beside(&dir, name), codec, limits, usize::try_from(MAX_IMPORT_BYTES).unwrap_or(usize::MAX))
}

#[cfg(test)]
#[path = "whiteboard_export_test.rs"]
mod whiteboard_export_test;

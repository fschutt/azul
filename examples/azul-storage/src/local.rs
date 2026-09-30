//! A folder on disk as a drive. The Home drive of AzDrive, and a local export
//! target for AzMail.

use std::{
    fs::{self, File},
    io::{ErrorKind, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU32, Ordering},
    time::UNIX_EPOCH,
};

use crate::{
    key::{check_path_key, check_path_prefix, folder_of},
    ByteRange, Drive, DriveError, ListPage, ListRequest, ObjectInfo,
};

/// A folder on disk: the key `a/b.txt` is the file `<root>/a/b.txt`. Keys that
/// would leave the root (`..`, absolute paths) are refused before any file is
/// touched. Symbolic links inside the root are followed when listing one
/// folder, not when walking a whole prefix (no loops).
#[derive(Debug, Clone)]
pub struct LocalDrive {
    root: PathBuf,
}

/// One entry of a listing before paging.
enum Entry {
    Folder(String),
    Object(ObjectInfo),
}

impl Entry {
    fn key(&self) -> &str {
        match self {
            Entry::Folder(prefix) => prefix,
            Entry::Object(info) => &info.key,
        }
    }
}

/// A file next to `path` to write before renaming it over `path`.
pub(crate) fn temp_sibling(path: &Path) -> PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    path.with_file_name(format!(
        ".{name}.azul-storage-{}-{}.tmp",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ))
}

/// Writes `bytes` to `path` through a temporary file, so a reader never sees half
/// a file and a failure leaves the old one.
pub(crate) fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = temp_sibling(path);
    if let Err(e) = fs::write(&tmp, bytes) {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        e
    })
}

/// A temporary file of [`temp_sibling`] (left over by a crash): not listed.
fn is_temp_name(name: &str) -> bool {
    name.starts_with('.') && name.contains(".azul-storage-") && name.ends_with(".tmp")
}

fn object_info(key: String, meta: &fs::Metadata) -> ObjectInfo {
    ObjectInfo {
        key,
        size: meta.len(),
        modified: meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs()),
        etag: None,
    }
}

fn not_found_or_io(key: &str, e: std::io::Error) -> DriveError {
    if e.kind() == ErrorKind::NotFound {
        DriveError::NotFound {
            key: key.to_string(),
        }
    } else {
        DriveError::Io(format!("{key}: {e}"))
    }
}

impl LocalDrive {
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        LocalDrive { root: root.into() }
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The file of a (checked) key.
    fn path_of(&self, key: &str) -> Result<PathBuf, DriveError> {
        check_path_key(key)?;
        let mut path = self.root.clone();
        for segment in key.split('/') {
            path.push(segment);
        }
        Ok(path)
    }

    /// The directory of a (checked) folder prefix: `a/b/` -> `<root>/a/b`.
    fn dir_of(&self, folder: &str) -> PathBuf {
        let mut path = self.root.clone();
        for segment in folder.split('/').filter(|s| !s.is_empty()) {
            path.push(segment);
        }
        path
    }

    /// The folders and files directly in `folder` whose keys start with `prefix`.
    fn read_level(
        &self,
        folder: &str,
        prefix: &str,
        out: &mut Vec<Entry>,
    ) -> Result<(), DriveError> {
        let dir = self.dir_of(folder);
        if !dir.is_dir() {
            return Ok(());
        }
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let Some(name) = entry.file_name().to_str().map(str::to_string) else {
                continue; // not UTF-8: no key can name it
            };
            if is_temp_name(&name) {
                continue;
            }
            let key = format!("{folder}{name}");
            if !key.starts_with(prefix) {
                continue;
            }
            // Follows a symbolic link; a broken one is left out.
            let Ok(meta) = fs::metadata(entry.path()) else {
                continue;
            };
            if meta.is_dir() {
                out.push(Entry::Folder(format!("{key}/")));
            } else if meta.is_file() {
                out.push(Entry::Object(object_info(key, &meta)));
            }
        }
        Ok(())
    }

    /// Every file under `folder`, at any depth (symbolic links to folders are not
    /// followed).
    fn walk(&self, folder: &str, out: &mut Vec<ObjectInfo>) -> Result<(), DriveError> {
        let dir = self.dir_of(folder);
        if !dir.is_dir() {
            return Ok(());
        }
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let Some(name) = entry.file_name().to_str().map(str::to_string) else {
                continue;
            };
            if is_temp_name(&name) {
                continue;
            }
            let key = format!("{folder}{name}");
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                self.walk(&format!("{key}/"), out)?;
            } else if let Ok(meta) = fs::metadata(entry.path()) {
                if meta.is_file() {
                    out.push(object_info(key, &meta));
                }
            }
        }
        Ok(())
    }
}

impl Drive for LocalDrive {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        check_path_prefix(&request.prefix)?;
        let prefix = request.prefix.as_str();
        let folder = folder_of(prefix);
        let mut entries = Vec::new();
        match request.delimiter.as_deref() {
            Some("/") => self.read_level(folder, prefix, &mut entries)?,
            delimiter => {
                let mut objects = Vec::new();
                self.walk(folder, &mut objects)?;
                for info in objects.into_iter().filter(|o| o.key.starts_with(prefix)) {
                    let rest = &info.key[prefix.len()..];
                    match delimiter.filter(|d| !d.is_empty()).and_then(|d| {
                        rest.find(d)
                            .map(|i| format!("{prefix}{}", &rest[..i + d.len()]))
                    }) {
                        Some(common) => entries.push(Entry::Folder(common)),
                        None => entries.push(Entry::Object(info)),
                    }
                }
            }
        }
        // S3 order: by key, byte-wise; a folder once.
        entries.sort_by(|a, b| a.key().cmp(b.key()));
        entries.dedup_by(|a, b| a.key() == b.key());
        if let Some(after) = request.continuation.as_deref() {
            entries.retain(|e| e.key() > after);
        }
        let size = request.page_size() as usize;
        let next = if entries.len() > size {
            Some(entries[size - 1].key().to_string())
        } else {
            None
        };
        entries.truncate(size);
        let mut page = ListPage {
            next,
            ..ListPage::default()
        };
        for entry in entries {
            match entry {
                Entry::Folder(prefix) => page.folders.push(prefix),
                Entry::Object(info) => page.objects.push(info),
            }
        }
        Ok(page)
    }

    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        let path = self.path_of(key)?;
        if path.is_dir() {
            return Err(DriveError::NotFound {
                key: key.to_string(),
            });
        }
        fs::read(&path).map_err(|e| not_found_or_io(key, e))
    }

    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        let path = self.path_of(key)?;
        let mut file = File::open(&path).map_err(|e| not_found_or_io(key, e))?;
        let meta = file.metadata()?;
        if meta.is_dir() {
            return Err(DriveError::NotFound {
                key: key.to_string(),
            });
        }
        let len = meta.len();
        let out_of_range = || DriveError::InvalidRange {
            key: key.to_string(),
        };
        if range.start >= len {
            return Err(out_of_range());
        }
        let end = range.end.map_or(len - 1, |end| end.min(len - 1));
        if end < range.start {
            return Err(out_of_range());
        }
        file.seek(SeekFrom::Start(range.start))?;
        let mut bytes = vec![0u8; (end - range.start + 1) as usize];
        file.read_exact(&mut bytes)?;
        Ok(bytes)
    }

    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        let path = self.path_of(key)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        if path.is_dir() {
            return Err(DriveError::InvalidKey {
                key: key.to_string(),
                reason: "a folder has this name",
            });
        }
        write_atomically(&path, bytes)?;
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<(), DriveError> {
        let path = self.path_of(key)?;
        match fs::symlink_metadata(&path) {
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
            Err(e) => Err(DriveError::Io(format!("{key}: {e}"))),
            Ok(meta) if meta.is_dir() => Err(DriveError::InvalidKey {
                key: key.to_string(),
                reason: "it is a folder",
            }),
            Ok(_) => match fs::remove_file(&path) {
                Err(e) if e.kind() != ErrorKind::NotFound => {
                    Err(DriveError::Io(format!("{key}: {e}")))
                }
                _ => Ok(()),
            },
        }
    }

    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        let path = self.path_of(key)?;
        let meta = fs::metadata(&path).map_err(|e| not_found_or_io(key, e))?;
        if !meta.is_file() {
            return Err(DriveError::NotFound {
                key: key.to_string(),
            });
        }
        Ok(object_info(key.to_string(), &meta))
    }
}

//! A folder on disk as a drive: the user's data tree (every app's durable
//! data, with its `.azlin/cache` manifest - see [`crate::manifest`]), and,
//! without the manifest, any other folder (AzDrive's Home and Downloads).

use std::{
    fs::{self, File},
    io::{ErrorKind, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU32, Ordering},
    time::UNIX_EPOCH,
};

use crate::{
    key::{check_path_key, check_path_prefix, folder_of},
    manifest::{self, is_reserved_key, Changes, Manifest, ManifestEntry, Record, MANIFEST_DIR},
    ops::check_folder,
    sigv4::{sha256_hex, sha256_hex_of},
    ByteRange, Drive, DriveError, ListPage, ListRequest, ObjectInfo,
};

/// A folder on disk: the key `a/b.txt` is the file `<root>/a/b.txt`. Keys that
/// would leave the root (`..`, absolute paths) are refused before any file is
/// touched. Symbolic links inside the root are followed when listing one
/// folder, not when walking a whole prefix (no loops).
///
/// [`LocalDrive::new`] is the user's DATA TREE: every put / delete / rename /
/// copy / folder operation is recorded in `<root>/.azlin/cache` (see
/// [`crate::manifest`]), so the later S3 sync only diffs. A folder that is
/// not the data tree (Downloads, the user's home) is opened with
/// [`LocalDrive::without_manifest`]. Either way `.azlin/` at the root is the
/// drive's own bookkeeping: never listed, no key may name it.
#[derive(Debug, Clone)]
pub struct LocalDrive {
    root: PathBuf,
    /// Keeps `<root>/.azlin/cache`.
    manifest: bool,
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

fn reserved(key: &str) -> DriveError {
    DriveError::InvalidKey {
        key: key.to_string(),
        reason: "it is the drive's own bookkeeping (.azlin)",
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
    /// The user's data tree at `root`: keeps the `.azlin/cache` manifest.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        LocalDrive {
            root: root.into(),
            manifest: true,
        }
    }

    /// A folder that is not the data tree (Downloads, the user's home, a
    /// folder the user added as a drive): no manifest, so browsing and
    /// writing there leave no `.azlin/` behind.
    #[must_use]
    pub fn without_manifest(root: impl Into<PathBuf>) -> Self {
        LocalDrive {
            root: root.into(),
            manifest: false,
        }
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Whether this drive keeps the manifest (it is the data tree).
    #[must_use]
    pub fn keeps_manifest(&self) -> bool {
        self.manifest
    }

    /// The manifest's file: `<root>/.azlin/cache`.
    #[must_use]
    pub fn manifest_path(&self) -> PathBuf {
        self.root
            .join(MANIFEST_DIR)
            .join(crate::manifest::CACHE_FILE)
    }

    /// What the manifest records (empty when there is none yet, or for a
    /// drive without one).
    pub fn manifest(&self) -> Result<Manifest, DriveError> {
        if !self.manifest {
            return Ok(Manifest::default());
        }
        Ok(manifest::load(&self.manifest_path())?.0)
    }

    /// Brings the manifest up to date with the folder: what changed outside
    /// this drive's own calls (another program, files from before the
    /// manifest existed) is hashed and recorded, and returned. The sync's
    /// first step; blocking, call it from a worker thread.
    pub fn refresh_manifest(&self) -> Result<Changes, DriveError> {
        if !self.manifest {
            return Err(DriveError::Unsupported(String::from(
                "this folder keeps no manifest",
            )));
        }
        let mut found = Changes::default();
        manifest::rewrite(&self.manifest_path(), |m| {
            let (changes, touched) = manifest::scan(m, self)?;
            for key in changes.added.iter().chain(&changes.modified) {
                match self.entry_of(key) {
                    Some(entry) => m.insert(key, entry),
                    None => m.remove(key), // gone since the listing
                }
            }
            for key in &changes.deleted {
                m.remove(key);
            }
            for t in touched {
                m.insert(&t.key, t.entry);
            }
            found = changes;
            Ok(())
        })?;
        Ok(found)
    }

    /// The manifest entry of the file of `key` as it is now (its bytes read
    /// in pieces); `None` when there is no such file.
    fn entry_of(&self, key: &str) -> Option<ManifestEntry> {
        let path = self.path_of(key).ok()?;
        let meta = fs::metadata(&path).ok().filter(fs::Metadata::is_file)?;
        let hash = File::open(&path).and_then(sha256_hex_of).ok()?;
        let info = object_info(key.to_string(), &meta);
        Some(ManifestEntry {
            size: info.size,
            modified: info.modified,
            hash,
        })
    }

    /// Records `record` in the manifest, when this drive keeps one. Never
    /// fails the drive call that made the change: the data is written, and
    /// the manifest is a cache that `diff` / `refresh_manifest` repair.
    fn record(&self, record: &Record) {
        if self.manifest {
            let _ = manifest::append(&self.manifest_path(), record);
        }
    }

    /// Records the file of `key` as it is now.
    fn record_file(&self, key: &str) {
        if !self.manifest {
            return;
        }
        if let Some(entry) = self.entry_of(key) {
            self.record(&Record::Put {
                key: key.to_string(),
                entry,
            });
        }
    }

    /// The file of a (checked) key.
    fn path_of(&self, key: &str) -> Result<PathBuf, DriveError> {
        check_path_key(key)?;
        if is_reserved_key(key) {
            return Err(reserved(key));
        }
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

    /// The directory of a folder name (`docs/sub/`), checked: not the root,
    /// ending in `/`, inside the root.
    fn folder_path(&self, prefix: &str) -> Result<PathBuf, DriveError> {
        check_folder(prefix)?;
        check_path_prefix(prefix)?;
        if is_reserved_key(prefix) {
            return Err(reserved(prefix));
        }
        Ok(self.dir_of(prefix))
    }

    /// The file or directory of a key or folder name, checked.
    fn any_path(&self, key: &str) -> Result<PathBuf, DriveError> {
        if key.ends_with('/') {
            self.folder_path(key)
        } else {
            self.path_of(key)
        }
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
            if is_temp_name(&name) || (folder.is_empty() && name == MANIFEST_DIR) {
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
            if is_temp_name(&name) || (folder.is_empty() && name == MANIFEST_DIR) {
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
        if is_reserved_key(folder) {
            return Ok(ListPage::default()); // the bookkeeping is not content
        }
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
        if self.manifest {
            if let Ok(meta) = fs::metadata(&path) {
                let info = object_info(key.to_string(), &meta);
                self.record(&Record::Put {
                    key: key.to_string(),
                    entry: ManifestEntry {
                        size: info.size,
                        modified: info.modified,
                        hash: sha256_hex(bytes),
                    },
                });
            }
        }
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<(), DriveError> {
        let path = self.path_of(key)?;
        match fs::symlink_metadata(&path) {
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(e) => return Err(DriveError::Io(format!("{key}: {e}"))),
            Ok(meta) if meta.is_dir() => {
                return Err(DriveError::InvalidKey {
                    key: key.to_string(),
                    reason: "it is a folder",
                })
            }
            Ok(_) => match fs::remove_file(&path) {
                Err(e) if e.kind() != ErrorKind::NotFound => {
                    return Err(DriveError::Io(format!("{key}: {e}")))
                }
                _ => {}
            },
        }
        self.record(&Record::Delete {
            key: key.to_string(),
        });
        Ok(())
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

    /// A file copy on disk, through a temporary file next to the target.
    fn copy(&self, from: &str, to: &str) -> Result<(), DriveError> {
        let source = self.path_of(from)?;
        let target = self.path_of(to)?;
        match fs::metadata(&source) {
            Err(e) => return Err(not_found_or_io(from, e)),
            Ok(meta) if !meta.is_file() => {
                return Err(DriveError::InvalidKey {
                    key: from.to_string(),
                    reason: "it is a folder",
                })
            }
            Ok(_) => {}
        }
        if target.is_dir() {
            return Err(DriveError::InvalidKey {
                key: to.to_string(),
                reason: "a folder has this name",
            });
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = temp_sibling(&target);
        let copied = fs::copy(&source, &tmp).and_then(|_| fs::rename(&tmp, &target));
        if let Err(e) = copied {
            let _ = fs::remove_file(&tmp);
            return Err(DriveError::Io(format!("{from}: {e}")));
        }
        self.record_file(to);
        Ok(())
    }

    fn create_folder(&self, prefix: &str) -> Result<(), DriveError> {
        let dir = self.folder_path(prefix)?;
        if dir.is_file() {
            return Err(DriveError::InvalidKey {
                key: prefix.to_string(),
                reason: "a file has this name",
            });
        }
        fs::create_dir_all(&dir).map_err(|e| DriveError::Io(format!("{prefix}: {e}")))
    }

    /// One `rename` on disk: a file, or a directory with everything in it.
    fn rename(&self, from: &str, to: &str) -> Result<(), DriveError> {
        if from.ends_with('/') != to.ends_with('/') {
            return Err(DriveError::InvalidKey {
                key: to.to_string(),
                reason: "a file and a folder cannot trade places",
            });
        }
        if from.ends_with('/') && to.starts_with(from) {
            return Err(DriveError::InvalidKey {
                key: to.to_string(),
                reason: "a folder cannot move into itself",
            });
        }
        let source = self.any_path(from)?;
        let target = self.any_path(to)?;
        if fs::symlink_metadata(&source).is_err() {
            return Err(DriveError::NotFound {
                key: from.to_string(),
            });
        }
        if fs::symlink_metadata(&target).is_ok() {
            return Err(DriveError::InvalidKey {
                key: to.to_string(),
                reason: "something has this name already",
            });
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(&source, &target).map_err(|e| DriveError::Io(format!("{from}: {e}")))?;
        // A rename keeps the modification time: the entries move as they are.
        self.record(&Record::Rename {
            from: from.to_string(),
            to: to.to_string(),
        });
        Ok(())
    }

    fn delete_folder(&self, prefix: &str) -> Result<(), DriveError> {
        let dir = self.folder_path(prefix)?;
        match fs::symlink_metadata(&dir) {
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
            Err(e) => Err(DriveError::Io(format!("{prefix}: {e}"))),
            Ok(meta) if !meta.is_dir() => Err(DriveError::InvalidKey {
                key: prefix.to_string(),
                reason: "it is a file",
            }),
            Ok(_) => {
                fs::remove_dir_all(&dir).map_err(|e| DriveError::Io(format!("{prefix}: {e}")))?;
                self.record(&Record::Delete {
                    key: prefix.to_string(),
                });
                Ok(())
            }
        }
    }

    fn local_path(&self, key: &str) -> Option<PathBuf> {
        self.any_path(key).ok()
    }

    fn metadata(&self, key: &str) -> Result<Vec<(String, String)>, DriveError> {
        let path = self.any_path(key)?;
        let meta = fs::metadata(&path).map_err(|e| not_found_or_io(key, e))?;
        let mut pairs = vec![(
            String::from("Location"),
            path.to_string_lossy().into_owned(),
        )];
        if let Some(created) = meta
            .created()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        {
            pairs.push((String::from("Created"), created.as_secs().to_string()));
        }
        pairs.push((
            String::from("Read-only"),
            String::from(if meta.permissions().readonly() {
                "Yes"
            } else {
                "No"
            }),
        ));
        Ok(pairs)
    }
}

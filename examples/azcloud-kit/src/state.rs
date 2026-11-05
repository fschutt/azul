//! One device's state folder (`--state-dir`, `AZCLOUD_HOME`, else
//! `<OS config folder>/azcloud`): everything this device knows about its
//! drives, kept OUT of every folder it syncs (the `.azlin` folder syncs, so a
//! secret there would reach the bucket and every other device).
//!
//! - `drives.json`: the drives, in azul-storage's format without secrets (the
//!   file AzDrive reads; `AZUL_DRIVES=<state>/drives.json` points it here).
//! - `azlin.json`: each drive's Azlin side ([`crate::account::DriveRecord`]):
//!   the token server it came from, the node list, when the credentials expire.
//! - `secrets.json` (0600): the credentials and the drive token under the OS
//!   keyring's entry names ([`crate::secrets`]).
//! - `device.json`: this device's id and name (conflict copies, the index's
//!   `updated_by`).
//! - `transport.json`: the last transport decision ([`crate::transport`]).
//! - `sync/`: one local index per synced folder ([`crate::sync`]).
//! - `outbox/`: the grant calls whose answer was not read yet ([`crate::outbox`]).
//!
//! Files are written through a temporary file and a rename
//! (`azul_storage::durable::replace`), so a crash never leaves half of one; every one is
//! flushed to disk before the rename and its folder after, since a rotated token that is lost
//! locks the device out and a base that is lost makes every file a new one.

use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    time::Duration,
};

use azul_storage::footprint::Allow;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::{
    error::{CloudError, CloudResult, Context},
    lock::{HeldLock, LockDir},
    secrets::FileSecrets,
};

/// The drives, in azul-storage's format.
pub const DRIVES_FILE: &str = "drives.json";
/// The drives' Azlin side.
pub const ACCOUNT_FILE: &str = "azlin.json";
/// The secrets.
pub const SECRETS_FILE: &str = "secrets.json";
/// The last transport decision.
pub const TRANSPORT_FILE: &str = "transport.json";
/// This device.
pub const DEVICE_FILE: &str = "device.json";
/// The local indexes of the synced folders.
pub const SYNC_DIR: &str = "sync";
/// The variable naming this device the first time (else its id is its name).
pub const DEVICE_VAR: &str = "AZCLOUD_DEVICE";
/// The folder of the locks, in the state folder.
const LOCKS_DIR: &str = "locks";
/// The longest device name kept.
const MAX_DEVICE_NAME: usize = 32;

/// This device: an id minted once, a name for people.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub created_at: i64,
}

/// A device name as it may appear in a file name (`notes/a (conflict
/// <name> 2026-10-08).md`): letters, digits, `.`, `_`, `-`; anything else
/// becomes `-`; at most 32 characters; `None` when nothing is left.
#[must_use]
pub fn clean_device_name(name: &str) -> Option<String> {
    let mut out = String::new();
    for c in name.trim().chars() {
        if out.chars().count() >= MAX_DEVICE_NAME {
            break;
        }
        if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_matches('-').to_string();
    (!out.is_empty()).then_some(out)
}

/// Ten random characters of lowercase base32 (RFC 4648's alphabet), from the repo's one seed
/// source: the part of a device id after `dev-`.
fn random_name() -> String {
    const ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";
    let mut seed = azul_storage::ids::random_seed();
    (0..10)
        .map(|_| {
            let c = char::from(ALPHABET[(seed & 31) as usize]);
            seed >>= 5;
            c
        })
        .collect()
}

/// The state folder of one device.
#[derive(Clone, Debug)]
pub struct StateDir {
    root: PathBuf,
    /// Whether its secrets file may hold a drive's keys (the user said so).
    allow: Allow,
}

impl StateDir {
    /// The folder at `root`, made (readable by this user only) when it is not
    /// there.
    ///
    /// # Errors
    ///
    /// When the folder cannot be made.
    pub fn open(root: &Path) -> CloudResult<StateDir> {
        create_private_dir(root)
            .with_context(|| format!("the state folder {}", root.display()))?;
        Ok(StateDir {
            root: root.to_path_buf(),
            allow: Allow::Nothing,
        })
    }

    /// This folder with the user's word on keeping a drive's keys in its secrets file
    /// (`--allow-plain-secrets`): without it a command that would keep a key there stops and says
    /// so (AZ-K7).
    #[must_use]
    pub fn allowing_plain_secrets(mut self, allow: Allow) -> StateDir {
        self.allow = allow;
        self
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.root
    }

    #[must_use]
    pub fn drives_file(&self) -> PathBuf {
        self.root.join(DRIVES_FILE)
    }

    #[must_use]
    pub fn account_file(&self) -> PathBuf {
        self.root.join(ACCOUNT_FILE)
    }

    #[must_use]
    pub fn transport_file(&self) -> PathBuf {
        self.root.join(TRANSPORT_FILE)
    }

    #[must_use]
    pub fn sync_dir(&self) -> PathBuf {
        self.root.join(SYNC_DIR)
    }

    /// The open grant calls of this device's drives ([`crate::outbox`]).
    #[must_use]
    pub fn outbox(&self) -> crate::outbox::Outbox {
        crate::outbox::Outbox::new(self.root.join(crate::outbox::OUTBOX_DIR))
    }

    /// The secrets file.
    #[must_use]
    pub fn secrets(&self) -> FileSecrets {
        FileSecrets::new(self.root.join(SECRETS_FILE)).allowing(self.allow)
    }

    /// This device, minted the first time: named `name`, else what
    /// `AZCLOUD_DEVICE` says, else after its id.
    ///
    /// # Errors
    ///
    /// When `device.json` cannot be read or written.
    pub fn device(&self, name: Option<&str>) -> CloudResult<Device> {
        let path = self.root.join(DEVICE_FILE);
        if let Some(device) = read_json::<Device>(&path)? {
            return Ok(device);
        }
        let id = format!("dev-{}", random_name());
        let env_name = std::env::var(DEVICE_VAR).ok();
        let name = name
            .or(env_name.as_deref())
            .and_then(clean_device_name)
            .unwrap_or_else(|| id.clone());
        let device = Device {
            id,
            name,
            created_at: crate::now(),
        };
        write_json(&path, &device, false)?;
        Ok(device)
    }

    /// Takes the lock `name`, waiting up to `wait` for another process (or thread) to drop it. The
    /// OS's own file lock ([`azul_storage::lock::LockDir`], in `locks/`): the OS drops it when
    /// its process ends, so a crash leaves nothing to wait out - and a holder that takes longer
    /// than any age (a refresh across a slow network) is never judged dead by the time it has
    /// held it (AZ-W13: a lock "older than two minutes" was taken over from a live process).
    ///
    /// # Errors
    ///
    /// When the lock is still held after `wait`, or its file cannot be made or locked.
    pub fn lock(&self, name: &str, wait: Duration) -> CloudResult<StateLock> {
        let held = LockDir::new(self.root.join(LOCKS_DIR)).lock(name, wait)?;
        Ok(StateLock { _held: held })
    }

    /// Whether `folder` holds this state folder or lies inside it: syncing it
    /// would upload the secrets, and a download could overwrite them.
    #[must_use]
    pub fn overlaps(&self, folder: &Path) -> bool {
        let state = absolute(&self.root);
        let folder = absolute(folder);
        state.starts_with(&folder) || folder.starts_with(&state)
    }
}

/// A held lock; dropped = released.
#[derive(Debug)]
pub struct StateLock {
    _held: HeldLock,
}

/// The path made absolute, with the longest part of it that exists resolved
/// (symbolic links: macOS's `/var` is `/private/var`) and the rest kept as
/// written - so a folder that does not exist yet compares with one that does.
fn absolute(path: &Path) -> PathBuf {
    let path = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let mut existing = path.as_path();
    let mut rest = Vec::new();
    loop {
        if let Ok(real) = fs::canonicalize(existing) {
            let mut out = real;
            for part in rest.iter().rev() {
                out.push(part);
            }
            return out;
        }
        match (existing.parent(), existing.file_name()) {
            (Some(parent), Some(name)) => {
                rest.push(name.to_os_string());
                existing = parent;
            }
            _ => return path.clone(),
        }
    }
}

/// Makes `dir` and its parents; on Unix the last one readable by this user
/// only.
pub(crate) fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    if dir.is_dir() {
        return Ok(());
    }
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    private_dir_mode(&mut builder);
    builder.create(dir)
}

#[cfg(unix)]
fn private_dir_mode(builder: &mut fs::DirBuilder) {
    use std::os::unix::fs::DirBuilderExt;
    builder.mode(0o700);
}

#[cfg(not(unix))]
fn private_dir_mode(_builder: &mut fs::DirBuilder) {}

/// Writes `bytes` to `path` through a temporary file beside it and a rename
/// ([`azul_storage::durable::replace`]): the file is synced to the disk before the rename and the
/// folder after, so a power cut leaves the old file or the new one and never an empty one;
/// `private` makes it (and the folders made on the way) readable by this user only (Unix).
///
/// # Errors
///
/// The I/O error that stopped the write (the old file stays then).
pub fn write_atomic(path: &Path, bytes: &[u8], private: bool) -> std::io::Result<()> {
    let mode = if private {
        azul_storage::durable::Mode::Private
    } else {
        azul_storage::durable::Mode::Shared
    };
    azul_storage::durable::replace(path, bytes, mode)
}

/// Reads the JSON file at `path`; `None` when there is none.
///
/// # Errors
///
/// When the file cannot be read or is not the JSON of a `T`.
pub fn read_json<T: DeserializeOwned>(path: &Path) -> CloudResult<Option<T>> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map(Some).map_err(|e| {
            CloudError::failed(format!("{} cannot be read: {e}", path.display()))
        }),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("{}", path.display())),
    }
}

/// Writes `value` as pretty JSON with a final newline ([`write_atomic`]).
///
/// # Errors
///
/// When it cannot be written.
pub fn write_json<T: Serialize>(path: &Path, value: &T, private: bool) -> CloudResult<()> {
    let mut text = serde_json::to_string_pretty(value)
        .map_err(|e| CloudError::failed(format!("{}: {e}", path.display())))?;
    text.push('\n');
    write_atomic(path, text.as_bytes(), private).with_context(|| format!("{}", path.display()))
}

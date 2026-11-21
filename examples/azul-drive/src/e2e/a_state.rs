//! What group A's commands keep between steps besides the [`super::Harness`]: the files a
//! scenario holds locked, the devices whose folders are watched, a wait for a pass of their
//! own, the volumes made for devices. One per sandbox: a new `drive_harness` starts afresh.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
    sync::{Mutex, PoisonError},
    time::Instant,
};

use super::with_harness;

/// A volume of its own made for a device: its image, mounted at `mount`.
pub(super) struct Volume {
    pub image: PathBuf,
    pub mount: PathBuf,
    /// It is attached (detached by `drive_detach` until `drive_attach`).
    pub attached: bool,
    /// Its file system keeps owners (APFS, HFS+; FAT and exFAT keep none).
    pub owners: bool,
}

pub(super) struct AState {
    /// The sandbox it belongs to.
    pub run: PathBuf,
    /// Files held with an exclusive lock, by device and key (`drive_lock` with a path).
    pub locks: BTreeMap<(String, String), fs::File>,
    /// Devices whose folders AzDrive watches for changes (`drive_watch`).
    pub watched: BTreeSet<String>,
    /// A `drive_wait_auto` waiting: since when, and the device's pass count then.
    pub auto_waits: BTreeMap<String, (Instant, u64)>,
    /// Volumes by device (`drive_volume`, `drive_mount_inside`: by device and path).
    pub volumes: BTreeMap<(String, String), Volume>,
    /// Devices whose `drive_sync` with `verify` ran its read-back and waits for its pass.
    pub verified: BTreeSet<String>,
    /// The key entries `drive_lock` took off a remote's keyring, by remote (`drive_unlock`
    /// puts them back): entry name and secret.
    pub keys_aside: BTreeMap<String, Vec<(String, String)>>,
    /// The racers swapping a device's folder for a link (`drive_racer`), by device.
    pub racers: BTreeMap<String, super::a_files::Racer>,
    /// The soak running (`drive_soak`), and what the last one's passes saw (`kept_soak`).
    pub soak: Option<super::a_soak::Soak>,
    pub soaked: Option<BTreeSet<String>>,
}

static A: Mutex<Option<AState>> = Mutex::new(None);

/// `f` with group A's state of this sandbox (made afresh for a new one). Never call
/// [`with_harness`] inside `f`: the harness is read first.
pub(super) fn with_a<T>(f: impl FnOnce(&mut AState) -> T) -> Result<T, String> {
    let run = with_harness(|h| Ok(h.run.clone()))?;
    let mut guard = A.lock().unwrap_or_else(PoisonError::into_inner);
    if guard.as_ref().is_none_or(|a| a.run != run) {
        *guard = Some(AState {
            run,
            locks: BTreeMap::new(),
            watched: BTreeSet::new(),
            auto_waits: BTreeMap::new(),
            volumes: BTreeMap::new(),
            verified: BTreeSet::new(),
            keys_aside: BTreeMap::new(),
            racers: BTreeMap::new(),
            soak: None,
            soaked: None,
        });
    }
    guard
        .as_mut()
        .map(f)
        .ok_or_else(|| String::from("group A's state is gone"))
}

/// Group A's state as it is, without the harness (the sync's hooks, `drive_cleanup`): `None`
/// before any group A command ran.
pub(super) fn peek<T>(f: impl FnOnce(&mut AState) -> T) -> Option<T> {
    let mut guard = A.lock().unwrap_or_else(PoisonError::into_inner);
    guard.as_mut().map(f)
}

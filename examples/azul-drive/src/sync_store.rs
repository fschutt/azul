//! The synced drives' file states as ONE store the window and the search share (§13.7): the
//! window asks it for a row's state icon and a folder's cloud-only files, the search - on its
//! worker threads too - for a result's sync state and the plain local copy of a drive's file
//! (the `SyncLookup` seam). It answers from memory: what the last pass, open, pin or "Free up
//! space" left, published by the UI thread ([`crate::sync_jobs`]); a cheap clone shares it.
//!
//! A file is found two ways: by its key in an encrypted drive's own listing (its names are the
//! files', under the pairing's folder of the drive), and by its path in a drive on this
//! computer that lies in a pairing's folder (the synced folder, browsed as a folder of Home).
//! A plain drive's own listing holds the sync's blobs, not its files: nothing of it is found.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard},
};

use azcloud_kit::sync::session::{FileState, LocalCopies, SyncSetup, SyncStates};

use crate::{jobs, sync_view::key_under};

/// One pairing as the store keeps it.
struct Pair {
    setup: SyncSetup,
    states: Arc<SyncStates>,
    /// The drive's own listing names its files (an encrypted drive opened as one).
    names_its_files: bool,
    /// The file moving now (its key under the pairing) and whether it goes up.
    moving: Option<(String, bool)>,
}

#[derive(Default)]
struct Inner {
    /// By drive id.
    pairs: HashMap<String, Pair>,
    /// The drives on this computer and their folders.
    roots: Vec<(String, PathBuf)>,
}

/// The synced drives' states, shared.
#[derive(Clone, Default)]
pub(crate) struct SyncStore {
    inner: Arc<RwLock<Inner>>,
}

/// A pairing's key of `key` of drive `drive`: its drive id and the key under its folder (a
/// folder's ending in `/`, the folder itself `""`).
fn locate_in(inner: &Inner, drive: &str, key: &str) -> Option<(String, String)> {
    let is_folder = key.is_empty() || key.ends_with('/');
    // Pairings never lie in one another (a pairing refuses a folder in or around another one):
    // at most one finds it.
    for (id, pair) in &inner.pairs {
        if id == drive {
            if pair.names_its_files {
                if let Some(rel) = key.strip_prefix(pair.setup.prefix.as_str()) {
                    return Some((id.clone(), rel.to_string()));
                }
            }
            continue;
        }
        // Encrypted local copies keep no plaintext folder.
        if pair.names_its_files && pair.setup.local_copies != LocalCopies::Decrypted {
            continue;
        }
        let Some((_, root)) = inner.roots.iter().find(|(d, _)| d == drive) else {
            continue;
        };
        let path = jobs::path_in(root, key);
        if let Some(rel) = key_under(&pair.setup.folder, &path, is_folder) {
            return Some((id.clone(), rel));
        }
    }
    None
}

impl SyncStore {
    fn read(&self) -> RwLockReadGuard<'_, Inner> {
        self.inner.read().unwrap_or_else(PoisonError::into_inner)
    }

    fn write(&self) -> RwLockWriteGuard<'_, Inner> {
        self.inner.write().unwrap_or_else(PoisonError::into_inner)
    }

    /// The pairings are `setups` from now on (a drive no longer among them is forgotten, a new
    /// one starts with no states); `names_its_files` says which drives' own listings name their
    /// files; `roots` are the drives on this computer.
    pub(crate) fn set_pairs(
        &self,
        setups: &[SyncSetup],
        names_its_files: &dyn Fn(&str) -> bool,
        roots: Vec<(String, PathBuf)>,
    ) {
        let mut inner = self.write();
        inner.pairs.retain(|id, _| setups.iter().any(|p| p.drive_id == *id));
        for setup in setups {
            let names = names_its_files(&setup.drive_id);
            match inner.pairs.get_mut(&setup.drive_id) {
                Some(pair) => {
                    pair.setup = setup.clone();
                    pair.names_its_files = names;
                }
                None => {
                    inner.pairs.insert(
                        setup.drive_id.clone(),
                        Pair {
                            setup: setup.clone(),
                            states: Arc::new(SyncStates::default()),
                            names_its_files: names,
                            moving: None,
                        },
                    );
                }
            }
        }
        inner.roots = roots;
    }

    /// Drive `drive_id`'s states from now on (a drive not paired is left alone).
    pub(crate) fn set_states(&self, drive_id: &str, states: SyncStates) {
        if let Some(pair) = self.write().pairs.get_mut(drive_id) {
            pair.states = Arc::new(states);
        }
    }

    /// The file of drive `drive_id` moving now (`None`: none).
    pub(crate) fn set_moving(&self, drive_id: &str, moving: Option<(String, bool)>) {
        if let Some(pair) = self.write().pairs.get_mut(drive_id) {
            pair.moving = moving;
        }
    }

    /// Drive `drive_id`'s states (empty while it is not paired).
    #[must_use]
    pub(crate) fn states(&self, drive_id: &str) -> Arc<SyncStates> {
        self.read()
            .pairs
            .get(drive_id)
            .map_or_else(|| Arc::new(SyncStates::default()), |p| Arc::clone(&p.states))
    }

    /// The pairing `key` of drive `drive` lies in, and its key under the pairing's folder.
    #[must_use]
    pub(crate) fn locate(&self, drive: &str, key: &str) -> Option<(String, String)> {
        locate_in(&self.read(), drive, key)
    }

    /// The state of `key` of drive `drive` (a folder's ending in `/`): moving, else as the
    /// states say; `None` outside every pairing, or for a file the drive does not have.
    #[must_use]
    pub(crate) fn file_state(&self, drive: &str, key: &str) -> Option<FileState> {
        let inner = self.read();
        let (id, rel) = locate_in(&inner, drive, key)?;
        let pair = inner.pairs.get(&id)?;
        if let Some((moving, up)) = &pair.moving {
            if *moving == rel {
                return Some(if *up {
                    FileState::Uploading { done: 0, total: 0 }
                } else {
                    FileState::Downloading { done: 0, total: 0 }
                });
            }
        }
        if rel.is_empty() || rel.ends_with('/') {
            pair.states.folder_state(&rel)
        } else {
            pair.states.state_of(&rel)
        }
    }

    /// The file on this computer holding the plain contents of `key` of drive `drive` as the
    /// last sync left them: a synced file that is on this device, not one in the cloud only or
    /// kept encrypted.
    #[must_use]
    pub(crate) fn local_copy(&self, drive: &str, key: &str) -> Option<PathBuf> {
        let inner = self.read();
        let (id, rel) = locate_in(&inner, drive, key)?;
        if rel.is_empty() || rel.ends_with('/') {
            return None;
        }
        let pair = inner.pairs.get(&id)?;
        if pair.names_its_files && pair.setup.local_copies != LocalCopies::Decrypted {
            return None;
        }
        let record = pair.states.files.get(&rel)?;
        if record.cloud_only || record.encrypted_copy {
            return None;
        }
        let path = azcloud_kit::sync::local::path_of(&pair.setup.folder, &rel);
        path.is_file().then_some(path)
    }

    /// Whether any drive syncs.
    #[must_use]
    pub(crate) fn any(&self) -> bool {
        !self.read().pairs.is_empty()
    }
}

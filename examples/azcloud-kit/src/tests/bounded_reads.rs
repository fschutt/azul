//! X-DELTA-AZDRIVE-KEYS-43 (AZ-C3 scenario B, AZ-M5): an object whose size the index, a HEAD or the
//! caller names is asked for WITHIN that size, so a storage that streams on - or answers with a
//! body swapped for a bigger one - is refused on its way in and not after the memory is full. The
//! reads here went through `Drive::get`, which takes whatever comes.

use std::sync::{Arc, Mutex, PoisonError};

use azul_storage::{
    meta::MemoryBucket, ByteRange, Drive, DriveError, Limit, ListPage, ListRequest, ObjectInfo,
    Precondition,
};

use crate::sync::{drive_store::DriveStore, Conditional, RemoteStore};

/// A bucket that writes down how its objects are read - the limit of each `get_limited`, each
/// plain `get` - and that, like a transport with a limit, refuses an object over the limit before it
/// hands out a byte of it.
pub(super) struct Reads {
    pub(super) inner: MemoryBucket,
    limited: Mutex<Vec<(String, u64)>>,
    plain: Mutex<Vec<String>>,
}

impl Reads {
    pub(super) fn new() -> Reads {
        Reads {
            inner: MemoryBucket::new(),
            limited: Mutex::new(Vec::new()),
            plain: Mutex::new(Vec::new()),
        }
    }

    /// The limits `key` was asked for within, in order.
    pub(super) fn limits_of(&self, key: &str) -> Vec<u64> {
        self.limited
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .filter(|(k, _)| k == key)
            .map(|(_, max)| *max)
            .collect()
    }

    /// How many times `key` was read by a plain GET.
    pub(super) fn plain_gets_of(&self, key: &str) -> usize {
        self.plain
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .filter(|k| *k == key)
            .count()
    }
}

impl Drive for Reads {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        self.inner.list(request)
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        self.plain
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(key.to_string());
        self.inner.get(key)
    }
    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        self.inner.get_range(key, range)
    }
    fn get_limited(&self, key: &str, limit: Limit) -> Result<Vec<u8>, DriveError> {
        self.limited
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((key.to_string(), limit.max));
        let info = self.inner.head(key)?;
        limit.check_object(key, info.size)?;
        self.inner.get(key)
    }
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        self.inner.put(key, bytes)
    }
    fn delete(&self, key: &str) -> Result<(), DriveError> {
        self.inner.delete(key)
    }
    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        self.inner.head(key)
    }
    fn put_if(
        &self,
        key: &str,
        bytes: &[u8],
        condition: &Precondition,
    ) -> Result<Option<String>, DriveError> {
        self.inner.put_if(key, bytes, condition)
    }
    fn conditional_writes(&self) -> bool {
        self.inner.conditional_writes()
    }
}

/// The sync's blob fetch names the size the index holds of it: a body of another size is refused
/// by the limit, not read whole and found out afterwards.
#[test]
fn x_delta_azdrive_keys_43_a_synced_blob_is_fetched_within_the_size_the_index_names() {
    let reads = Arc::new(Reads::new());
    reads.put("blobs/aa", b"eight by").unwrap();
    let store = DriveStore::new(reads.clone());

    assert_eq!(
        store.fetch("blobs/aa", 8).unwrap().as_deref(),
        Some(&b"eight by"[..])
    );
    assert_eq!(
        reads.limits_of("blobs/aa"),
        vec![8],
        "asked within the index's size"
    );
    assert_eq!(
        reads.plain_gets_of("blobs/aa"),
        0,
        "never by a GET that takes whatever comes"
    );

    // the storage holds more than the index says: refused, naming the limit
    let err = store.fetch("blobs/aa", 5).unwrap_err();
    assert!(err.to_string().contains("at most 5 bytes"), "{err}");
    assert_eq!(reads.plain_gets_of("blobs/aa"), 0);
    // nothing there is still nothing
    assert_eq!(store.fetch("blobs/none", 8).unwrap(), None);
}

/// The sync index is read within the size the HEAD just said, and a missing Content-Length (a HEAD
/// that reads 0) falls back to the ceiling and not to "nothing may be read".
#[test]
fn x_delta_azdrive_keys_43_the_sync_index_is_read_within_the_size_its_head_said() {
    let reads = Arc::new(Reads::new());
    reads.put("idx.json", b"{\"files\":{}}").unwrap();
    let size = reads.head("idx.json").unwrap().size;
    let store = DriveStore::new(reads.clone());
    match store.get_unless("idx.json", None).unwrap() {
        Conditional::Found { body, .. } => assert_eq!(body.len() as u64, size),
        other => panic!("{other:?}"),
    }
    assert_eq!(reads.limits_of("idx.json"), vec![size]);
    assert_eq!(reads.plain_gets_of("idx.json"), 0);
}

#[test]
fn x_delta_azdrive_keys_43_the_head_limit_is_the_head_size_unless_the_head_said_nothing() {
    let ceiling = 1 << 30;
    assert_eq!(Limit::as_the_head_said("x", 700, ceiling).max, 700);
    assert_eq!(
        Limit::as_the_head_said("x", 0, ceiling).max,
        ceiling,
        "a missing Content-Length reads as 0: the ceiling, not an object that may be empty only"
    );
    assert_eq!(
        Limit::as_the_head_said("x", ceiling + 1, ceiling).max,
        ceiling,
        "a HEAD that claims more than the ceiling is held to it"
    );
}

/// The drive's period vault is read within the size its HEAD said.
#[cfg(feature = "encryption")]
#[test]
fn x_delta_azdrive_keys_43_the_period_vault_is_read_within_the_size_its_head_said() {
    use azul_storage::crypto::DriveKey;

    use crate::vault::{DriveVault, TokenVault, UnlockedDrive, VaultState};

    let reads = Arc::new(Reads::new());
    let key = DriveKey::generate().unwrap();
    let drive = UnlockedDrive::new("d_1", reads.clone(), key);
    DriveVault::new(&drive)
        .join(&VaultState::default())
        .unwrap();
    let size = reads.head(DriveVault::KEY).unwrap().size;

    let read = DriveVault::new(&drive).read().unwrap();
    assert!(read.tokens.is_empty());
    let limits = reads.limits_of(DriveVault::KEY);
    assert!(
        !limits.is_empty() && limits.iter().all(|max| *max == size),
        "every read of the vault is within its size ({size}): {limits:?}"
    );
    assert_eq!(
        reads.plain_gets_of(DriveVault::KEY),
        0,
        "never by a GET that takes whatever comes"
    );
}

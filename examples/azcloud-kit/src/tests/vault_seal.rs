//! The period vault on this computer holds the issue keys of paid checkouts (bearers for months)
//! and the period tokens: sealed, they are ciphertext on disk under a random key the keyring
//! holds and nothing else does (AZ-K13). A keyring that cannot give the key refuses; the vault is
//! never written in the clear instead.

use std::{path::Path, sync::Arc};

use azul_storage::{
    footprint::KeyEntry,
    keyring::{KeyringError, KeyringNamespace, KeyringStore, MemoryKeyring},
    testing::TempDir,
};

use super::period::{stored, ISSUE_KEY};
use crate::{
    error::CloudError,
    period::PeriodTokenStore,
    vault::{TokenVault, VaultGrant, VaultState},
    SharedKeyring,
};

fn shared(dir: &TempDir, keyring: Arc<dyn KeyringStore>) -> SharedKeyring {
    SharedKeyring::with_lock_root(keyring, KeyringNamespace::azlin_shared(), dir.path())
}

/// A store sealed through a fresh keyring, in a folder of its own.
fn sealed(dir: &TempDir) -> (PeriodTokenStore, Arc<MemoryKeyring>) {
    let keyring = Arc::new(MemoryKeyring::new());
    let store = PeriodTokenStore::sealed(
        dir.path().join("period-tokens"),
        shared(dir, keyring.clone()),
    );
    (store, keyring)
}

/// Every file below `dir`, with its bytes.
fn files_below(dir: &Path) -> Vec<(std::path::PathBuf, Vec<u8>)> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(files_below(&path));
        } else if let Ok(bytes) = std::fs::read(&path) {
            out.push((path, bytes));
        }
    }
    out
}

fn contains(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|w| w == needle.as_bytes())
}

/// What was written is ciphertext: no token, no issue key and no checkout in the clear, and the
/// store reads it back.
#[test]
fn a_sealed_vault_is_ciphertext_on_disk_and_reads_back() {
    let dir = TempDir::new("azcloud-vault-seal");
    let (store, keyring) = sealed(&dir);
    store.add("d_1", &[stored("aa"), stored("bb")]).unwrap();
    store
        .local("d_1")
        .unwrap()
        .join(&VaultState::with_grant(VaultGrant::new(
            "ck_1", "100GB", 3, ISSUE_KEY,
        )))
        .unwrap();

    let raw = std::fs::read(store.path_of("d_1").unwrap()).unwrap();
    assert!(
        azul_storage::crypto::blob::is_sealed(&raw),
        "the file is not sealed"
    );
    for secret in ["aa".repeat(32), ISSUE_KEY.to_string(), String::from("ck_1")] {
        assert!(!contains(&raw, &secret), "the vault file shows {secret}");
    }
    assert!(serde_json::from_slice::<serde_json::Value>(&raw).is_err());
    assert_eq!(
        store.tokens("d_1").unwrap(),
        vec![stored("aa"), stored("bb")]
    );
    assert!(store
        .local("d_1")
        .unwrap()
        .read()
        .unwrap()
        .grant("ck_1")
        .is_some());
    assert!(
        keyring
            .get(&KeyEntry::VaultKey.name("d_1"))
            .unwrap()
            .is_some(),
        "the key is in the keyring, under the drive's entry"
    );
}

/// The key lives in the keyring and nowhere else: not in the vault's folder, not beside the file,
/// not in the second copy.
#[test]
fn the_key_is_never_written_next_to_the_vault() {
    let dir = TempDir::new("azcloud-vault-seal");
    let (store, keyring) = sealed(&dir);
    store.add("d_1", &[stored("aa")]).unwrap();
    let local = store.local("d_1").unwrap();
    local
        .second_copy()
        .join(&VaultState::with_tokens(&[stored("aa")]))
        .unwrap();
    let key = keyring
        .get(&KeyEntry::VaultKey.name("d_1"))
        .unwrap()
        .unwrap();
    assert!(key.len() >= 32, "{key}");
    let mut seen = 0;
    for (path, bytes) in files_below(dir.path()) {
        seen += 1;
        assert!(!contains(&bytes, &key), "{} holds the key", path.display());
    }
    assert!(seen >= 2, "the vault and its copy were written: {seen}");
}

/// A vault an older version wrote is plain JSON: a sealed store reads it as it is, and the next
/// change seals it.
#[test]
fn a_plain_vault_of_an_older_version_is_read_and_sealed_by_the_next_change() {
    let dir = TempDir::new("azcloud-vault-seal");
    let folder = dir.path().join("period-tokens");
    PeriodTokenStore::new(&folder)
        .add("d_1", &[stored("aa")])
        .unwrap();
    let path = folder.join("d_1.json");
    assert!(!azul_storage::crypto::blob::is_sealed(
        &std::fs::read(&path).unwrap()
    ));

    let keyring = Arc::new(MemoryKeyring::new());
    let store = PeriodTokenStore::sealed(&folder, shared(&dir, keyring));
    assert_eq!(
        store.tokens("d_1").unwrap(),
        vec![stored("aa")],
        "read as it is"
    );
    assert!(
        !azul_storage::crypto::blob::is_sealed(&std::fs::read(&path).unwrap()),
        "reading changes nothing"
    );
    // A change that adds nothing still seals the file.
    store.add("d_1", &[stored("aa")]).unwrap();
    assert!(azul_storage::crypto::blob::is_sealed(
        &std::fs::read(&path).unwrap()
    ));
    store.add("d_1", &[stored("bb")]).unwrap();
    assert_eq!(
        store.tokens("d_1").unwrap(),
        vec![stored("aa"), stored("bb")]
    );
}

/// A keyring that cannot give or take the key refuses with its own error and nothing reaches the
/// disk: the vault is never written in the clear instead.
struct NoKeyring;

impl KeyringStore for NoKeyring {
    fn get(&self, _key: &str) -> Result<Option<String>, KeyringError> {
        Err(KeyringError::Unavailable)
    }
    fn set(&self, _key: &str, _secret: &str) -> Result<(), KeyringError> {
        Err(KeyringError::Unavailable)
    }
    fn delete(&self, _key: &str) -> Result<(), KeyringError> {
        Err(KeyringError::Unavailable)
    }
}

#[test]
fn a_keyring_that_cannot_give_the_key_refuses_and_writes_nothing_in_the_clear() {
    let dir = TempDir::new("azcloud-vault-seal");
    let store = PeriodTokenStore::sealed(
        dir.path().join("period-tokens"),
        shared(&dir, Arc::new(NoKeyring)),
    );
    let why = store.add("d_1", &[stored("aa")]).unwrap_err();
    assert!(
        matches!(why.root(), CloudError::Keyring(KeyringError::Unavailable)),
        "{why:?}"
    );
    assert!(
        files_below(&dir.path().join("period-tokens"))
            .iter()
            .all(|(path, _)| path.extension().is_none_or(|e| e != "json")),
        "a vault file was written"
    );
}

/// A sealed vault whose key is gone from the keyring is an error, never an empty vault: a change
/// would otherwise start over and lose what the file held.
#[test]
fn a_sealed_vault_without_its_key_is_an_error_not_an_empty_vault() {
    let dir = TempDir::new("azcloud-vault-seal");
    let (store, keyring) = sealed(&dir);
    store.add("d_1", &[stored("aa")]).unwrap();
    keyring.delete(&KeyEntry::VaultKey.name("d_1")).unwrap();
    assert!(store.tokens("d_1").is_err());
    assert!(
        store.add("d_1", &[stored("bb")]).is_err(),
        "a change started over"
    );
    assert!(
        keyring
            .get(&KeyEntry::VaultKey.name("d_1"))
            .unwrap()
            .is_none(),
        "a new key was made over a file the old one sealed"
    );
}

/// A sealed file is bound to its drive: moved to another drive's name, with that drive holding
/// the same key, it does not open.
#[test]
fn a_sealed_file_moved_to_another_drive_does_not_open() {
    let dir = TempDir::new("azcloud-vault-seal");
    let (store, keyring) = sealed(&dir);
    store.add("d_1", &[stored("aa")]).unwrap();
    let key = keyring
        .get(&KeyEntry::VaultKey.name("d_1"))
        .unwrap()
        .unwrap();
    keyring.set(&KeyEntry::VaultKey.name("d_2"), &key).unwrap();
    std::fs::copy(store.path_of("d_1").unwrap(), store.path_of("d_2").unwrap()).unwrap();
    assert!(store.tokens("d_2").is_err());
}

/// The second copy is sealed too, and heals the vault when its file is gone.
#[test]
fn the_second_copy_is_sealed_and_heals_a_lost_vault() {
    let dir = TempDir::new("azcloud-vault-seal");
    let (store, _keyring) = sealed(&dir);
    let local = store.local("d_1").unwrap();
    local
        .join(&VaultState::with_tokens(&[stored("aa")]))
        .unwrap();
    local
        .second_copy()
        .join(&VaultState::with_tokens(&[stored("aa")]))
        .unwrap();
    let copy = store.dir().join("d_1.copy.json");
    assert!(azul_storage::crypto::blob::is_sealed(
        &std::fs::read(&copy).unwrap()
    ));
    std::fs::remove_file(store.path_of("d_1").unwrap()).unwrap();
    assert_eq!(store.tokens("d_1").unwrap(), vec![stored("aa")]);
}

/// A store without a seal cannot read a sealed file: it says so, and does not start an empty
/// vault over it.
#[test]
fn a_store_without_a_seal_does_not_read_a_sealed_vault_as_empty() {
    let dir = TempDir::new("azcloud-vault-seal");
    let (store, _keyring) = sealed(&dir);
    store.add("d_1", &[stored("aa")]).unwrap();
    let plain = PeriodTokenStore::new(store.dir());
    let why = plain.tokens("d_1").unwrap_err();
    assert!(why.to_string().contains("sealed"), "{why}");
    assert!(plain.add("d_1", &[stored("bb")]).is_err());
}

/// A background job reads through a keyring of its own over the same entries (it never prompts):
/// the store `through` it reads what the interactive one wrote.
#[test]
fn a_store_through_another_keyring_over_the_same_entries_reads_the_same_vault() {
    let dir = TempDir::new("azcloud-vault-seal");
    let (store, keyring) = sealed(&dir);
    store.add("d_1", &[stored("aa")]).unwrap();
    let quiet = store.through(shared(&dir, keyring));
    assert!(quiet.is_sealed());
    assert_eq!(quiet.tokens("d_1").unwrap(), vec![stored("aa")]);
    assert!(!PeriodTokenStore::new(store.dir())
        .through(shared(&dir, Arc::new(NoKeyring)))
        .is_sealed());
}

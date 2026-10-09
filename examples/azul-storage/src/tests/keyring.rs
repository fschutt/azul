//! The keyring as the storage layer sees it: one text per entry name, read and written from any
//! thread, errors that never show a secret.

use std::sync::Arc;

use crate::keyring::{KeyringError, KeyringStore, MemoryKeyring};

#[test]
fn a_keyring_entry_reads_back_what_was_stored_and_debug_shows_no_secret() {
    let keyring = MemoryKeyring::new();
    assert_eq!(keyring.get("azul-storage/s3/d_1"), Ok(None));
    keyring.set("azul-storage/s3/d_1", "sesame").unwrap();
    assert_eq!(
        keyring.get("azul-storage/s3/d_1").unwrap().as_deref(),
        Some("sesame")
    );
    keyring.set("azul-storage/s3/d_1", "open").unwrap();
    assert_eq!(
        keyring.get("azul-storage/s3/d_1").unwrap().as_deref(),
        Some("open"),
        "a store replaces what was there"
    );
    keyring.delete("azul-storage/s3/d_1").unwrap();
    keyring
        .delete("azul-storage/s3/d_1")
        .expect("deleting an entry that is not there is no error");
    assert_eq!(keyring.get("azul-storage/s3/d_1"), Ok(None));
    keyring.set("azcloud/checkouts", "sesame").unwrap();
    let shown = format!("{keyring:?}");
    assert!(
        shown.contains("azcloud/checkouts") && !shown.contains("sesame"),
        "{shown}"
    );
}

#[test]
fn threads_share_one_keyring_behind_its_trait() {
    let keyring: Arc<dyn KeyringStore> = Arc::new(MemoryKeyring::new());
    let threads: Vec<_> = (0..8)
        .map(|i| {
            let keyring = keyring.clone();
            std::thread::spawn(move || keyring.set(&format!("entry-{i}"), "value").unwrap())
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
    for i in 0..8 {
        assert_eq!(
            keyring.get(&format!("entry-{i}")).unwrap().as_deref(),
            Some("value")
        );
    }
}

#[test]
fn a_keyring_error_says_what_went_wrong() {
    assert!(KeyringError::Unavailable
        .to_string()
        .contains("no keyring"));
    assert!(KeyringError::Denied.to_string().contains("refused"));
    assert!(KeyringError::Failed(String::from("locked"))
        .to_string()
        .contains("locked"));
}

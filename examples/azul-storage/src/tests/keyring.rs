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

#[test]
fn the_azlin_apps_share_one_keyring_namespace_and_any_other_app_keeps_its_own() {
    use crate::keyring::KeyringNamespace;
    assert_eq!(KeyringNamespace::azlin_shared().as_str(), "io.azlin.shared");
    assert_eq!(
        KeyringNamespace::app().as_str(),
        "",
        "empty: the app's own id, which azul fills in"
    );
    assert_eq!(
        KeyringNamespace::named(" org.example.Editor ").as_str(),
        "org.example.Editor"
    );
}

/// A keyring that refuses `refused` (as the user's "No" to the system's prompt does) and counts
/// every read.
struct Refusing {
    inner: MemoryKeyring,
    refused: &'static str,
    reads: std::sync::atomic::AtomicUsize,
}

impl KeyringStore for Refusing {
    fn get(&self, key: &str) -> Result<Option<String>, KeyringError> {
        self.reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if key == self.refused {
            return Err(KeyringError::Denied);
        }
        self.inner.get(key)
    }

    fn set(&self, key: &str, secret: &str) -> Result<(), KeyringError> {
        self.inner.set(key, secret)
    }

    fn delete(&self, key: &str) -> Result<(), KeyringError> {
        self.inner.delete(key)
    }
}

fn refusing(refused: &'static str) -> Refusing {
    let inner = MemoryKeyring::new();
    inner.set("a", "1").unwrap();
    inner.set("c", "3").unwrap();
    Refusing {
        inner,
        refused,
        reads: std::sync::atomic::AtomicUsize::new(0),
    }
}

fn keys(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_string()).collect()
}

/// "Unlock drives": one round reads every drive's entry, one after the other, and the user's
/// first "No" ends it - the entries after it answer Denied unread, so a refusal is never
/// followed by one more prompt per drive.
#[test]
fn a_round_that_asks_reads_each_entry_once_and_ends_at_the_first_refusal() {
    use crate::keyring::{read_entries, Asking};
    let keyring = refusing("b");
    let answers = read_entries(&keyring, &keys(&["a", "b", "c", "missing"]), Asking::User);
    let names: Vec<&str> = answers.iter().map(|(key, _)| key.as_str()).collect();
    assert_eq!(names, ["a", "b", "c", "missing"], "one answer per entry, in order");
    assert_eq!(answers[0].1, Ok(Some(String::from("1"))));
    assert_eq!(answers[1].1, Err(KeyringError::Denied));
    assert_eq!(answers[2].1, Err(KeyringError::Denied), "not read after the refusal");
    assert_eq!(answers[3].1, Err(KeyringError::Denied));
    assert_eq!(keyring.reads.load(std::sync::atomic::Ordering::SeqCst), 2);
}

/// Background work reads quietly - no read can raise a prompt - so a quiet round reads every
/// entry: one that would need the user does not hold back the drives that need nobody.
#[test]
fn a_quiet_round_reads_every_entry_even_after_a_refusal() {
    use crate::keyring::{read_entries, Asking};
    let keyring = refusing("a");
    let answers = read_entries(&keyring, &keys(&["a", "c", "missing"]), Asking::Never);
    assert_eq!(answers[0].1, Err(KeyringError::Denied));
    assert_eq!(answers[1].1, Ok(Some(String::from("3"))));
    assert_eq!(answers[2].1, Ok(None), "an entry that is not there");
    assert_eq!(keyring.reads.load(std::sync::atomic::Ordering::SeqCst), 3);
}

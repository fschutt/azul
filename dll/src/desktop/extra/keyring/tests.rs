//! The keyring a worker thread reads and writes: a blocking call answers on the calling thread
//! (no frame, no result channel), from the same headless stand-in a callback's request reaches;
//! `AZ_KEYRING_FILE` keeps that stand-in in a file - for the next run, shared by the processes
//! of one test. Only the stand-ins are driven here: a test never touches the real keyring.

use std::{
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

use azul_core::keyring::{KeyringRequest, KeyringResult};
use azul_css::AzString;

use super::{answer_with, Backend};

/// A key no other test uses (the in-memory stand-in is the process's one).
fn unique(what: &str) -> String {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    format!(
        "azul-keyring-test/{what}/{}",
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

/// A keyring file of its own under the system's temporary folder (none there yet).
fn keyring_file(what: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "azul-keyring-test-{what}-{}-{}.json",
        std::process::id(),
        unique("file").rsplit('/').next().unwrap_or_default()
    ));
    let _ = std::fs::remove_file(&path);
    path
}

fn store(key: &str, secret: &str) -> KeyringRequest {
    KeyringRequest::Store {
        key: AzString::from(key),
        secret: AzString::from(secret),
        require_biometry: false,
    }
}

fn get(key: &str) -> KeyringRequest {
    KeyringRequest::Get {
        key: AzString::from(key),
    }
}

fn delete(key: &str) -> KeyringRequest {
    KeyringRequest::Delete {
        key: AzString::from(key),
    }
}

#[test]
fn a_blocking_call_answers_on_the_calling_thread_from_the_headless_keyring() {
    let key = unique("blocking");
    assert_eq!(
        answer_with(&store(&key, "s3cr3t"), &Backend::Memory),
        KeyringResult::Stored
    );
    // A worker thread reads what was stored at once: the answer is the call's return value.
    let read = key.clone();
    let got = std::thread::spawn(move || answer_with(&get(&read), &Backend::Memory))
        .join()
        .unwrap();
    assert_eq!(got, KeyringResult::Retrieved(AzString::from("s3cr3t")));
    assert_eq!(
        answer_with(&delete(&key), &Backend::Memory),
        KeyringResult::Deleted
    );
    assert_eq!(
        answer_with(&get(&key), &Backend::Memory),
        KeyringResult::NotFound
    );
    assert_eq!(
        answer_with(&delete(&key), &Backend::Memory),
        KeyringResult::Deleted,
        "deleting what is not there answers Deleted, as the platform backends do"
    );
}

#[test]
fn a_keyring_file_keeps_the_headless_keyring_for_the_next_run_and_the_other_processes() {
    let path = keyring_file("persist");
    let file = Backend::File(path.clone());
    assert_eq!(answer_with(&get("a"), &file), KeyringResult::NotFound);
    assert_eq!(answer_with(&store("a", "1"), &file), KeyringResult::Stored);
    assert_eq!(answer_with(&store("b", "2"), &file), KeyringResult::Stored);
    // The next run: nothing in memory, the file has them.
    assert_eq!(
        answer_with(&get("a"), &Backend::File(path.clone())),
        KeyringResult::Retrieved(AzString::from("1"))
    );
    // Two processes at once (threads, each opening the file itself): none loses the other's.
    let writers: Vec<_> = (0..2)
        .map(|writer| {
            let path = path.clone();
            std::thread::spawn(move || {
                for i in 0..20 {
                    let key = format!("w{writer}-{i}");
                    assert_eq!(
                        answer_with(&store(&key, &key), &Backend::File(path.clone())),
                        KeyringResult::Stored
                    );
                }
            })
        })
        .collect();
    for writer in writers {
        writer.join().unwrap();
    }
    for writer in 0..2 {
        for i in 0..20 {
            let key = format!("w{writer}-{i}");
            assert_eq!(
                answer_with(&get(&key), &file),
                KeyringResult::Retrieved(AzString::from(key.as_str())),
                "{key} lost"
            );
        }
    }
    assert_eq!(answer_with(&delete("a"), &file), KeyringResult::Deleted);
    assert_eq!(answer_with(&get("a"), &file), KeyringResult::NotFound);
    assert_eq!(
        answer_with(&get("b"), &file),
        KeyringResult::Retrieved(AzString::from("2"))
    );
    // A damaged file answers an error, never a panic or an empty keyring.
    std::fs::write(&path, b"not json").unwrap();
    assert_eq!(answer_with(&get("b"), &file), KeyringResult::Error);
    let _ = std::fs::remove_file(&path);
}

//! The keyring a worker thread reads and writes: a blocking call answers on the calling thread
//! (no frame, no result channel), from the same headless stand-in a callback's request reaches;
//! `AZ_KEYRING_FILE` keeps that stand-in in a file - for the next run, shared by the processes
//! of one test. Every entry lives in a namespace (a keyring service): an app's own by default, one
//! the Azlin apps share when they name it; an entry of the service every azul app used to share
//! moves into the namespace that asks for it. Only the stand-ins are driven here: a test never
//! touches the real keyring.

use std::{
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

use azul_core::keyring::{KeyringRequest, KeyringResult};
use azul_css::AzString;

use azul_core::keyring::KeyringNamespace;

use super::{answer_in, service_of, Backend};

/// The namespace the tests' entries live in (an app's own id).
const TEST_SERVICE: &str = "org.example.KeyringTest";

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
        answer_in(TEST_SERVICE, &store(&key, "s3cr3t"), &Backend::Memory),
        KeyringResult::Stored
    );
    // A worker thread reads what was stored at once: the answer is the call's return value.
    let read = key.clone();
    let got = std::thread::spawn(move || answer_in(TEST_SERVICE, &get(&read), &Backend::Memory))
        .join()
        .unwrap();
    assert_eq!(got, KeyringResult::Retrieved(AzString::from("s3cr3t")));
    assert_eq!(
        answer_in(TEST_SERVICE, &delete(&key), &Backend::Memory),
        KeyringResult::Deleted
    );
    assert_eq!(
        answer_in(TEST_SERVICE, &get(&key), &Backend::Memory),
        KeyringResult::NotFound
    );
    assert_eq!(
        answer_in(TEST_SERVICE, &delete(&key), &Backend::Memory),
        KeyringResult::Deleted,
        "deleting what is not there answers Deleted, as the platform backends do"
    );
}

#[test]
fn a_keyring_file_keeps_the_headless_keyring_for_the_next_run_and_the_other_processes() {
    let path = keyring_file("persist");
    let file = Backend::File(path.clone());
    assert_eq!(answer_in(TEST_SERVICE, &get("a"), &file), KeyringResult::NotFound);
    assert_eq!(answer_in(TEST_SERVICE, &store("a", "1"), &file), KeyringResult::Stored);
    assert_eq!(answer_in(TEST_SERVICE, &store("b", "2"), &file), KeyringResult::Stored);
    // The next run: nothing in memory, the file has them.
    assert_eq!(
        answer_in(TEST_SERVICE, &get("a"), &Backend::File(path.clone())),
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
                        answer_in(TEST_SERVICE, &store(&key, &key), &Backend::File(path.clone())),
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
                answer_in(TEST_SERVICE, &get(&key), &file),
                KeyringResult::Retrieved(AzString::from(key.as_str())),
                "{key} lost"
            );
        }
    }
    assert_eq!(answer_in(TEST_SERVICE, &delete("a"), &file), KeyringResult::Deleted);
    assert_eq!(answer_in(TEST_SERVICE, &get("a"), &file), KeyringResult::NotFound);
    assert_eq!(
        answer_in(TEST_SERVICE, &get("b"), &file),
        KeyringResult::Retrieved(AzString::from("2"))
    );
    // A damaged file answers an error, never a panic or an empty keyring.
    std::fs::write(&path, b"not json").unwrap();
    assert_eq!(answer_in(TEST_SERVICE, &get("b"), &file), KeyringResult::Error);
    let _ = std::fs::remove_file(&path);
}

/// A run that names a keyring file keeps its keyring there, windowed or headless: two real
/// windows of one app on a test stack (two AzDrive profiles, say) would otherwise share the
/// system keyring's entries - and with them each other's unfinished checkouts. Without a file a
/// windowed run uses the system keyring and a headless one the in-memory stand-in; a blank name
/// is no file.
#[test]
fn a_keyring_file_is_the_keyring_of_a_windowed_run_too() {
    let file = std::ffi::OsString::from("/tmp/a-test-stack-keyring.json");
    assert_eq!(
        super::backend_for(false, Some(file.clone())),
        Backend::File(PathBuf::from(&file))
    );
    assert_eq!(
        super::backend_for(true, Some(file.clone())),
        Backend::File(PathBuf::from(&file))
    );
    assert_eq!(super::backend_for(false, None), Backend::Platform);
    assert_eq!(super::backend_for(true, None), Backend::Memory);
    assert_eq!(
        super::backend_for(false, Some(std::ffi::OsString::new())),
        Backend::Platform
    );
}

fn retrieved(secret: &str) -> KeyringResult {
    KeyringResult::Retrieved(AzString::from(secret))
}

#[test]
fn two_namespaces_do_not_see_each_others_entries() {
    let key = unique("namespaces");
    let (one, two) = ("org.example.One", "org.example.Two");
    assert_eq!(answer_in(one, &store(&key, "one"), &Backend::Memory), KeyringResult::Stored);
    assert_eq!(answer_in(two, &get(&key), &Backend::Memory), KeyringResult::NotFound);
    assert_eq!(answer_in(two, &store(&key, "two"), &Backend::Memory), KeyringResult::Stored);
    assert_eq!(answer_in(one, &get(&key), &Backend::Memory), retrieved("one"));
    assert_eq!(answer_in(one, &delete(&key), &Backend::Memory), KeyringResult::Deleted);
    assert_eq!(answer_in(two, &get(&key), &Backend::Memory), retrieved("two"));
}

#[test]
fn a_read_in_a_namespace_moves_the_entry_of_the_old_shared_service_once() {
    let key = unique("moved");
    let legacy = KeyringNamespace::LEGACY_SERVICE;
    assert_eq!(answer_in(legacy, &store(&key, "token-g"), &Backend::Memory), KeyringResult::Stored);
    assert_eq!(
        answer_in("io.azlin.shared", &get(&key), &Backend::Memory),
        retrieved("token-g")
    );
    assert_eq!(
        answer_in(legacy, &get(&key), &Backend::Memory),
        KeyringResult::NotFound,
        "moved, not copied: no other app finds it under the old service"
    );
    assert_eq!(
        answer_in("org.example.Other", &get(&key), &Backend::Memory),
        KeyringResult::NotFound
    );
    assert_eq!(
        answer_in("io.azlin.shared", &get(&key), &Backend::Memory),
        retrieved("token-g")
    );
}

#[test]
fn an_old_entry_never_comes_back_after_a_store_or_a_delete_in_the_namespace() {
    let legacy = KeyringNamespace::LEGACY_SERVICE;
    let stored = unique("stored");
    answer_in(legacy, &store(&stored, "spent"), &Backend::Memory);
    assert_eq!(
        answer_in(TEST_SERVICE, &store(&stored, "fresh"), &Backend::Memory),
        KeyringResult::Stored
    );
    assert_eq!(answer_in(legacy, &get(&stored), &Backend::Memory), KeyringResult::NotFound);
    assert_eq!(answer_in(TEST_SERVICE, &get(&stored), &Backend::Memory), retrieved("fresh"));

    let deleted = unique("deleted");
    answer_in(legacy, &store(&deleted, "spent"), &Backend::Memory);
    assert_eq!(
        answer_in(TEST_SERVICE, &delete(&deleted), &Backend::Memory),
        KeyringResult::Deleted
    );
    assert_eq!(
        answer_in(TEST_SERVICE, &get(&deleted), &Backend::Memory),
        KeyringResult::NotFound,
        "a deleted entry is not read back from the old service"
    );
}

#[test]
fn a_keyring_file_reads_its_flat_entries_as_the_old_service_and_keeps_namespaces_apart() {
    let path = keyring_file("namespaces");
    // What a test stack seeds: entry name -> secret, as every app shared it.
    std::fs::write(&path, br#"{"azul-storage/s3/d_1":"session"}"#).unwrap();
    let file = Backend::File(path.clone());
    assert_eq!(
        answer_in("io.azlin.shared", &get("azul-storage/s3/d_1"), &file),
        retrieved("session")
    );
    assert_eq!(
        answer_in("org.example.Other", &store("note", "x"), &file),
        KeyringResult::Stored
    );
    let kept: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(
        kept,
        serde_json::json!({
            "io.azlin.shared": {"azul-storage/s3/d_1": "session"},
            "org.example.Other": {"note": "x"},
        })
    );
    assert_eq!(answer_in("org.example.Other", &get("azul-storage/s3/d_1"), &file), KeyringResult::NotFound);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_namespace_names_its_service_and_the_default_one_is_the_apps_own_id() {
    assert_eq!(
        service_of(&KeyringNamespace::create(AzString::from(" io.azlin.shared "))),
        "io.azlin.shared"
    );
    assert_eq!(
        service_of(&KeyringNamespace::app()),
        crate::desktop::app_identity::current().id
    );
    assert_ne!(service_of(&KeyringNamespace::app()), KeyringNamespace::LEGACY_SERVICE);
}

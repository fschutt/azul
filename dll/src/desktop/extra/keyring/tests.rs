//! The keyring a worker thread reads and writes: a blocking call answers on the calling thread
//! (no frame, no result channel), from the same headless stand-in a callback's request reaches;
//! `AZ_KEYRING_FILE` keeps that stand-in in a file - for the next run, shared by the processes
//! of one test - in a build with the cargo feature `test-hooks`; a shipped build reads none of
//! the keyring's environment switches. Every entry lives in a namespace (a keyring service): an app's own by default, one
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

use super::{
    answer_in, answer_with, blob_refusal, has_nul, service_of, Backend, Prompts,
    CREDENTIAL_BLOB_MAX,
};

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

/// The keyring a run with these facts uses: `choice` is `AZ_KEYRING`, `file` is
/// `AZ_KEYRING_FILE`. Only a build with the cargo feature `test-hooks` has a choice, a file or a
/// headless run in its facts ([`super::RunFacts::from_env`]): the tests below that name them
/// describe such a build, and a release build's are checked in
/// `a_build_without_the_test_hooks_reads_none_of_the_keyring_switches`.
fn backend(choice: Option<&str>, file: Option<&str>, headless: bool, debug_build: bool) -> Backend {
    super::backend_for(&super::RunFacts {
        choice: choice.map(std::ffi::OsString::from),
        file: file.map(std::ffi::OsString::from),
        headless,
        debug_build,
    })
}

/// A run that names a keyring file keeps its keyring there, windowed or headless: two real
/// windows of one app on a test stack (two AzDrive profiles, say) would otherwise share the
/// system keyring's entries - and with them each other's unfinished checkouts. Without a file a
/// windowed release run uses the system keyring and a headless one the in-memory stand-in; a
/// blank name is no file.
#[test]
fn a_keyring_file_is_the_keyring_of_a_windowed_run_too() {
    let file = "/tmp/a-test-stack-keyring.json";
    assert_eq!(backend(None, Some(file), false, false), Backend::File(PathBuf::from(file)));
    assert_eq!(backend(None, Some(file), true, false), Backend::File(PathBuf::from(file)));
    assert_eq!(backend(None, None, false, false), Backend::Platform);
    assert_eq!(backend(None, None, true, false), Backend::Memory);
    assert_eq!(backend(None, Some(""), false, false), Backend::Platform);
}

/// Every rebuild of a debug build is a new code signature, which the system keychain asks the
/// user about again (one password prompt per entry and run): a debug build keeps its secrets in
/// memory unless `AZ_KEYRING=system` asks for the system keyring, or a file is named.
#[test]
fn a_debug_build_keeps_its_secrets_in_memory_unless_told_to_use_the_system_keyring() {
    assert_eq!(backend(None, None, false, true), Backend::Memory);
    assert_eq!(backend(Some("system"), None, false, true), Backend::Platform);
    assert_eq!(
        backend(None, Some("/tmp/dev.json"), false, true),
        Backend::File(PathBuf::from("/tmp/dev.json"))
    );
    // A word it does not know chooses nothing: the build decides.
    assert_eq!(backend(Some("keychain"), None, false, true), Backend::Memory);
    assert_eq!(backend(Some("keychain"), None, false, false), Backend::Platform);
}

/// `AZ_KEYRING` picks the keyring outright: `memory`, `file` (the one `AZ_KEYRING_FILE` names;
/// memory without one) or `system` - in any case and with spaces around it.
#[test]
fn az_keyring_picks_the_system_keyring_a_file_or_memory() {
    let file = "/tmp/a-profile/keyring.json";
    assert_eq!(backend(Some("memory"), None, false, false), Backend::Memory);
    assert_eq!(backend(Some("memory"), Some(file), false, false), Backend::Memory);
    assert_eq!(backend(Some(" File "), Some(file), false, false), Backend::File(PathBuf::from(file)));
    assert_eq!(backend(Some("file"), None, false, false), Backend::Memory);
    assert_eq!(backend(Some("SYSTEM"), Some(file), false, false), Backend::Platform);
}

/// A headless or E2E run never reaches the system keyring, whatever `AZ_KEYRING` says: a test on
/// a developer's Mac would otherwise raise the keychain's prompts.
#[test]
fn a_headless_run_never_reaches_the_system_keyring_even_when_told_to() {
    assert_eq!(backend(Some("system"), None, true, false), Backend::Memory);
    assert_eq!(
        backend(Some("system"), Some("/tmp/t.json"), true, false),
        Backend::File(PathBuf::from("/tmp/t.json"))
    );
}

/// The environment of a run that holds every test switch of the keyring.
fn every_switch(name: &str) -> Option<std::ffi::OsString> {
    match name {
        "AZ_KEYRING" => Some("memory".into()),
        "AZ_KEYRING_FILE" => Some("/tmp/a-stack/keyring.json".into()),
        "AZ_BACKEND" => Some("headless".into()),
        "AZ_E2E_TEST" => Some("/tmp/scenario.json".into()),
        _ => None,
    }
}

/// K5/P9 (decision d, 2026-11-04): a RELEASE or distribution build never reads the keyring's
/// test switches. It used to honour `AZ_KEYRING=memory|file`, `AZ_KEYRING_FILE=<path>` (a
/// plaintext JSON file), `AZ_BACKEND=headless` and any value of `AZ_E2E_TEST`, silently: the
/// drive session, the drive key and the member key then went to process memory (lost at exit:
/// an encrypted drive locked) or into a plain file, set by a shell profile or a wrapper. Without
/// the cargo feature `test-hooks` the facts of a run hold none of them, so a release build uses
/// the system keyring, and a debug build memory - by the build alone.
#[test]
fn a_build_without_the_test_hooks_reads_none_of_the_keyring_switches() {
    let release = super::RunFacts::from_env(false, every_switch, false);
    assert_eq!(release.choice, None);
    assert_eq!(release.file, None);
    assert!(!release.headless);
    assert_eq!(super::backend_for(&release), Backend::Platform);

    // The same environment in a debug build: memory, because it is a debug build.
    let debug = super::RunFacts::from_env(false, every_switch, true);
    assert_eq!(super::backend_for(&debug), Backend::Memory);
    assert!(!debug.headless && debug.choice.is_none() && debug.file.is_none());
}

/// With the hooks, the same environment is what it was: the switches pick the keyring.
#[test]
fn a_build_with_the_test_hooks_reads_the_keyring_switches() {
    let facts = super::RunFacts::from_env(true, every_switch, false);
    assert_eq!(facts.choice, Some(std::ffi::OsString::from("memory")));
    assert_eq!(
        facts.file,
        Some(std::ffi::OsString::from("/tmp/a-stack/keyring.json"))
    );
    assert!(facts.headless);
    assert_eq!(super::backend_for(&facts), Backend::Memory);

    let only = |name: &'static str, value: &'static str| {
        move |asked: &str| (asked == name).then(|| std::ffi::OsString::from(value))
    };
    assert!(super::RunFacts::from_env(true, only("AZ_BACKEND", "headless"), false).headless);
    assert!(!super::RunFacts::from_env(true, only("AZ_BACKEND", "metal"), false).headless);
    assert!(
        super::RunFacts::from_env(true, only("AZ_E2E_TEST", ""), false).headless,
        "any value of AZ_E2E_TEST, as before"
    );
    assert_eq!(
        super::backend_for(&super::RunFacts::from_env(
            true,
            only("AZ_KEYRING_FILE", "/tmp/stack.json"),
            false
        )),
        Backend::File(PathBuf::from("/tmp/stack.json"))
    );
}

/// The process's own environment, as `of_this_run` reads it: only a build with the cargo feature
/// `test-hooks` sees the variables.
#[test]
fn this_process_reads_its_environment_only_with_the_test_hooks() {
    // A variable no other test of this module reads.
    std::env::set_var(super::KEYRING_FILE_VAR, "/tmp/az-keyring-hooks-test.json");
    let facts = super::RunFacts::of_this_run();
    std::env::remove_var(super::KEYRING_FILE_VAR);
    if cfg!(feature = "test-hooks") {
        assert_eq!(
            facts.file,
            Some(std::ffi::OsString::from("/tmp/az-keyring-hooks-test.json"))
        );
    } else {
        assert_eq!(facts.file, None, "a build without the hooks does not look");
    }
}

/// The plaintext keyring file is the owner's alone, also when it was made before with broader
/// modes (it held a test stack's secrets in the default 0644).
#[cfg(unix)]
#[test]
fn a_keyring_file_is_readable_by_its_owner_alone() {
    use std::os::unix::fs::PermissionsExt;
    let mode = |path: &PathBuf| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
    let path = keyring_file("mode");
    let file = Backend::File(path.clone());
    assert_eq!(
        answer_in(TEST_SERVICE, &store("a", "1"), &file),
        KeyringResult::Stored
    );
    assert_eq!(mode(&path), 0o600, "made 0600");

    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(answer_in(TEST_SERVICE, &get("a"), &file), retrieved("1"));
    assert_eq!(mode(&path), 0o600, "tightened at the next op, a read too");
    let _ = std::fs::remove_file(&path);
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

/// A quiet op (what an app's background work reads with) answers on the stand-ins exactly as a
/// plain one: only the system keyring has prompts to leave out.
#[test]
fn a_quiet_op_answers_like_a_plain_one_on_the_stand_ins() {
    let key = unique("quiet");
    let quiet = |req: &KeyringRequest, backend: &Backend| {
        answer_with(TEST_SERVICE, req, backend, Prompts::Never)
    };
    assert_eq!(quiet(&get(&key), &Backend::Memory), KeyringResult::NotFound);
    assert_eq!(quiet(&store(&key, "s3cr3t"), &Backend::Memory), KeyringResult::Stored);
    assert_eq!(quiet(&get(&key), &Backend::Memory), retrieved("s3cr3t"));
    assert_eq!(
        answer_in(TEST_SERVICE, &get(&key), &Backend::Memory),
        retrieved("s3cr3t"),
        "a plain read finds what a quiet store wrote"
    );
    assert_eq!(quiet(&delete(&key), &Backend::Memory), KeyringResult::Deleted);

    let path = keyring_file("quiet");
    let file = Backend::File(path.clone());
    assert_eq!(quiet(&store("a", "1"), &file), KeyringResult::Stored);
    assert_eq!(quiet(&get("a"), &file), retrieved("1"));
    assert_eq!(answer_in(TEST_SERVICE, &get("a"), &file), retrieved("1"));
    let _ = std::fs::remove_file(&path);
}

/// The C API's quiet ops, beside the blocking ones of a namespace. Only their shapes are checked
/// here - a call would reach this run's keyring.
#[test]
fn the_keyring_offers_quiet_ops_in_a_namespace() {
    let get: fn(KeyringNamespace, AzString) -> KeyringResult = super::Keyring::get_quietly_in;
    let store: fn(KeyringNamespace, AzString, AzString, bool) -> KeyringResult =
        super::Keyring::store_quietly_in;
    let delete: fn(KeyringNamespace, AzString) -> KeyringResult =
        super::Keyring::delete_quietly_in;
    let _ = (get, store, delete);
}

/// AZ-W16: libsecret took the secret `a\0b` through a C string and kept "" (`unwrap_or_default`), a
/// key with a NUL looked up another entry. Every backend refuses a NUL the same way now, so what
/// works on one computer does not become nothing on another.
#[test]
fn az_w16_a_key_or_secret_with_a_nul_is_an_error_on_every_backend_and_is_never_kept_as_nothing() {
    let file = keyring_file("nul");
    for backend in [Backend::Memory, Backend::File(file.clone())] {
        let key = unique("nul");
        assert_eq!(
            answer_in(TEST_SERVICE, &store(&key, "a\0b"), &backend),
            KeyringResult::Error,
            "a secret with a NUL"
        );
        assert_eq!(
            answer_in(TEST_SERVICE, &get(&key), &backend),
            KeyringResult::NotFound,
            "and nothing was kept, not even an empty one"
        );
        for request in [store("bad\0key", "s"), get("bad\0key"), delete("bad\0key")] {
            assert_eq!(
                answer_in(TEST_SERVICE, &request, &backend),
                KeyringResult::Error,
                "a key with a NUL: {request:?}"
            );
        }
        assert_eq!(
            answer_in("service\0name", &store(&key, "s"), &backend),
            KeyringResult::Error,
            "a service with a NUL"
        );
        // The control: text without a NUL, an empty secret, and the quiet way are as before.
        assert_eq!(
            answer_in(TEST_SERVICE, &store(&key, "plain"), &backend),
            KeyringResult::Stored
        );
        assert_eq!(
            answer_with(TEST_SERVICE, &get(&key), &backend, Prompts::Never),
            KeyringResult::Retrieved(AzString::from("plain"))
        );
        assert_eq!(
            answer_with(TEST_SERVICE, &store(&key, "a\0b"), &backend, Prompts::Never),
            KeyringResult::Error,
            "quiet or not"
        );
    }
    // The predicate itself.
    assert!(has_nul("s", &store("k", "a\0")));
    assert!(!has_nul("s", &store("k", "")));
    assert!(!has_nul("s", &get("k")));
}

/// AZ-W16: `CredWriteW` refuses a blob over 2560 bytes with a bare error; the sentence says both
/// sizes, and the limit is the Credential Manager's (the callers' lists are bounded by it).
#[test]
fn az_w16_a_secret_over_what_one_credential_holds_is_refused_with_both_sizes() {
    assert_eq!(CREDENTIAL_BLOB_MAX, 5 * 512);
    assert_eq!(blob_refusal(0), None);
    assert_eq!(blob_refusal(CREDENTIAL_BLOB_MAX), None, "exactly the most fits");
    let over = blob_refusal(CREDENTIAL_BLOB_MAX + 1).expect("one byte too many");
    assert!(over.contains("2560") && over.contains("2561"), "{over}");
    assert!(blob_refusal(100_000).expect("far too many").contains("100000"));
}

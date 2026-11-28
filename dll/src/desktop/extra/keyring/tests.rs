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

use azul_core::keyring::{
    KeyringNamespace, KeyringNeed, KeyringProtection, KeyringPrompt, KeyringRequest,
    KeyringResult, KeyringSupport,
};
use azul_css::AzString;
use azul_layout::managers::stand_in_user::{ScriptedOutcome, StandInUser};

use super::{
    answer_in, answer_with, blob_refusal, has_nul, need_in, quiet_verdict, service_of, support_in,
    Backend, Collection, Interaction, CREDENTIAL_BLOB_MAX,
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
        protection: KeyringProtection::DeviceUnlocked,
        prompt: KeyringPrompt::default(),
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

/// The process's own facts are its build's hooks (`azul_core::run_mode::test_hooks()`, read once
/// and only by a build with the cargo feature `test-hooks`): a variable set later is not seen, and
/// a build without the feature has no opinion whatever the environment holds. (That the hooks
/// read each variable is `azul-core`'s `run_mode` tests.)
#[test]
fn this_process_asks_the_hooks_of_its_build_and_nothing_else() {
    // A variable no other test of this module reads.
    std::env::set_var(super::KEYRING_FILE_VAR, "/tmp/az-keyring-hooks-test.json");
    let facts = super::RunFacts::of_this_run();
    std::env::remove_var(super::KEYRING_FILE_VAR);
    let hooks = azul_core::run_mode::test_hooks();
    assert_eq!(facts.file, hooks.keyring_file);
    assert_eq!(facts.choice, hooks.keyring_choice);
    assert_eq!(facts.headless, hooks.keyring_headless);
    if !cfg!(feature = "test-hooks") {
        assert_eq!(facts.file, None, "a build without the hooks does not look");
        assert_eq!(facts.choice, None);
        assert!(!facts.headless);
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
        answer_with(TEST_SERVICE, req, backend, Interaction::Quiet, &StandInUser::new())
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
    let store: fn(KeyringNamespace, AzString, AzString) -> KeyringResult =
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
            answer_with(TEST_SERVICE, &get(&key), &backend, Interaction::Quiet, &StandInUser::new()),
            KeyringResult::Retrieved(AzString::from("plain"))
        );
        assert_eq!(
            answer_with(
                TEST_SERVICE,
                &store(&key, "a\0b"),
                &backend,
                Interaction::Quiet,
                &StandInUser::new()
            ),
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
    assert_eq!(
        blob_refusal(CREDENTIAL_BLOB_MAX),
        None,
        "exactly the most fits"
    );
    let over = blob_refusal(CREDENTIAL_BLOB_MAX + 1).expect("one byte too many");
    assert!(over.contains("2560") && over.contains("2561"), "{over}");
    assert!(blob_refusal(100_000)
        .expect("far too many")
        .contains("100000"));
}

/// AZ-W16: a quiet op (a start, a timer, a poll) must never raise the keyring's unlock dialog. With
/// the default collection locked libsecret asks for its password even for a quiet read; the op looks
/// first and answers `WouldPrompt`. The truth table of what the collection's state settles, on every host.
#[test]
fn az_w16_a_quiet_op_answers_would_prompt_for_a_locked_collection_instead_of_the_unlock_dialog() {
    let (read, write, remove) = (get("k"), store("k", "s"), delete("k"));
    // Locked: nothing is touched, whatever the op.
    for request in [&read, &write, &remove] {
        assert_eq!(
            quiet_verdict(Collection::Locked, request),
            Some(KeyringResult::WouldPrompt),
            "{request:?}"
        );
    }
    // No default collection: a read finds nothing and a delete has nothing to delete (as the
    // platform backends answer), but a store would make one, which asks for a password.
    assert_eq!(
        quiet_verdict(Collection::Missing, &read),
        Some(KeyringResult::NotFound)
    );
    assert_eq!(
        quiet_verdict(Collection::Missing, &remove),
        Some(KeyringResult::Deleted)
    );
    assert_eq!(
        quiet_verdict(Collection::Missing, &write),
        Some(KeyringResult::WouldPrompt)
    );
    // Open, or not told (an old libsecret, a refused call): the op goes on to libsecret.
    for state in [Collection::Unlocked, Collection::Unknown] {
        for request in [&read, &write, &remove] {
            assert_eq!(quiet_verdict(state, request), None, "{state:?} {request:?}");
        }
    }
}


// ───────── The protection model, on the stand-ins ───────────────────────

const KEY: &str = "vault";

fn store_protected(key: &str, secret: &str, protection: KeyringProtection) -> KeyringRequest {
    KeyringRequest::Store {
        key: AzString::from(key),
        secret: AzString::from(secret),
        protection,
        prompt: KeyringPrompt::create(AzString::from("Unlock your vault")),
    }
}

fn unlock(key: &str) -> KeyringRequest {
    KeyringRequest::Unlock {
        key: AzString::from(key),
        prompt: KeyringPrompt::create(AzString::from("Unlock your vault")),
    }
}

/// An op of the user's action: the sheet may show (what a callback's request gets).
fn with_sheet(req: &KeyringRequest, backend: &Backend, user: &StandInUser) -> KeyringResult {
    answer_with(TEST_SERVICE, req, backend, Interaction::UserCheck, user)
}

/// A worker thread's op (blocking, or quiet): no sheet.
fn without_sheet(
    req: &KeyringRequest,
    backend: &Backend,
    user: &StandInUser,
    interaction: Interaction,
) -> KeyringResult {
    answer_with(TEST_SERVICE, req, backend, interaction, user)
}

/// A user with a biometric and a passcode, and a key no other test uses.
fn enrolled_user() -> StandInUser {
    let user = StandInUser::new();
    user.enroll(true, true);
    user
}

/// Every flow below runs on the memory stand-in and on the file one: the same answers.
fn both_stand_ins() -> [Backend; 2] {
    [Backend::Memory, Backend::File(keyring_file("model"))]
}

#[test]
fn a_protected_item_is_opened_by_the_users_match_and_never_by_a_get() {
    for backend in both_stand_ins() {
        let user = enrolled_user();
        let key = unique("match");
        assert_eq!(
            with_sheet(&store_protected(&key, "s3cr3t", KeyringProtection::Biometric), &backend, &user),
            KeyringResult::Stored
        );
        // A get shows nothing and gives nothing.
        assert_eq!(
            with_sheet(&get(&key), &backend, &user),
            KeyringResult::WouldPrompt,
            "a get of a protected item never opens it"
        );
        // The user's match opens it.
        assert_eq!(with_sheet(&unlock(&key), &backend, &user), retrieved("s3cr3t"));
        assert_eq!(with_sheet(&delete(&key), &backend, &user), KeyringResult::Deleted);
        assert_eq!(with_sheet(&unlock(&key), &backend, &user), KeyringResult::NotFound);
    }
}

#[test]
fn a_worker_threads_call_never_shows_the_sheet_and_never_makes_or_opens_a_protected_item() {
    for backend in both_stand_ins() {
        let user = enrolled_user();
        let key = unique("worker");
        for interaction in [Interaction::Dialogs, Interaction::Quiet] {
            assert_eq!(
                without_sheet(
                    &store_protected(&key, "s", KeyringProtection::UserPresence),
                    &backend,
                    &user,
                    interaction
                ),
                KeyringResult::WouldPrompt,
                "{interaction:?}: a protected store needs the user's action"
            );
            assert_eq!(
                without_sheet(&unlock(&key), &backend, &user, interaction),
                KeyringResult::WouldPrompt,
                "{interaction:?}: an unlock needs the sheet"
            );
        }
        // Nothing was stored by the refused calls.
        assert_eq!(with_sheet(&get(&key), &backend, &user), KeyringResult::NotFound);
        // And a protected item made behind the user's action is not opened by a worker thread.
        assert_eq!(
            with_sheet(&store_protected(&key, "s", KeyringProtection::UserPresence), &backend, &user),
            KeyringResult::Stored
        );
        for interaction in [Interaction::Dialogs, Interaction::Quiet] {
            assert_eq!(
                without_sheet(&get(&key), &backend, &user, interaction),
                KeyringResult::WouldPrompt
            );
        }
        // No answer was used up: the script is untouched by any of it.
        assert_eq!(user.pending_outcomes(), 0);
    }
}

#[test]
fn the_user_who_does_not_pass_is_told_why() {
    for backend in both_stand_ins() {
        let user = enrolled_user();
        let key = unique("refusals");
        with_sheet(&store_protected(&key, "s3cr3t", KeyringProtection::Biometric), &backend, &user);
        user.script([
            ScriptedOutcome::Mismatch,
            ScriptedOutcome::Cancel,
            ScriptedOutcome::Lockout,
            ScriptedOutcome::Match,
        ]);
        assert_eq!(with_sheet(&unlock(&key), &backend, &user), KeyringResult::Failed);
        assert_eq!(with_sheet(&unlock(&key), &backend, &user), KeyringResult::Cancelled);
        assert_eq!(with_sheet(&unlock(&key), &backend, &user), KeyringResult::LockedOut);
        assert_eq!(with_sheet(&unlock(&key), &backend, &user), retrieved("s3cr3t"));
        // Not one of them is `Denied`: that is the system refusing the app.
    }
}

#[test]
fn a_level_the_device_cannot_meet_stores_nothing() {
    for backend in both_stand_ins() {
        let nothing = StandInUser::new();
        for level in [KeyringProtection::UserPresence, KeyringProtection::Biometric] {
            let key = unique("notenrolled");
            assert_eq!(
                with_sheet(&store_protected(&key, "s", level), &backend, &nothing),
                KeyringResult::NotEnrolled,
                "{level:?}: nothing enrolled"
            );
            assert_eq!(with_sheet(&get(&key), &backend, &nothing), KeyringResult::NotFound);
        }
        // A passcode meets `UserPresence`, not `Biometric`.
        let passcode_only = StandInUser::new();
        passcode_only.enroll(false, true);
        let key = unique("passcode");
        assert_eq!(
            with_sheet(&store_protected(&key, "s", KeyringProtection::Biometric), &backend, &passcode_only),
            KeyringResult::NotEnrolled
        );
        assert_eq!(
            with_sheet(&store_protected(&key, "s", KeyringProtection::UserPresence), &backend, &passcode_only),
            KeyringResult::Stored
        );
    }
}

#[test]
fn a_changed_enrolment_invalidates_a_biometric_item_and_spares_a_user_presence_one() {
    for backend in both_stand_ins() {
        let user = enrolled_user();
        let (bio, presence) = (unique("bio"), unique("presence"));
        with_sheet(&store_protected(&bio, "b", KeyringProtection::Biometric), &backend, &user);
        with_sheet(&store_protected(&presence, "p", KeyringProtection::UserPresence), &backend, &user);
        user.enrollment_changed();
        assert_eq!(with_sheet(&unlock(&bio), &backend, &user), KeyringResult::Invalidated);
        assert_eq!(
            with_sheet(&get(&bio), &backend, &user),
            KeyringResult::Invalidated,
            "a get says so too, without the sheet"
        );
        assert_eq!(
            with_sheet(&unlock(&presence), &backend, &user),
            retrieved("p"),
            "the passcode still works"
        );
        // Invalidated is not NotFound: the record is there until the app stores again or deletes.
        assert_eq!(
            need_in(TEST_SERVICE, &bio, &backend, &user),
            KeyringNeed::Invalidated
        );
        // Store again under the new set.
        assert_eq!(
            with_sheet(&store_protected(&bio, "b2", KeyringProtection::Biometric), &backend, &user),
            KeyringResult::Stored
        );
        assert_eq!(with_sheet(&unlock(&bio), &backend, &user), retrieved("b2"));
    }
}

#[test]
fn the_probe_says_what_a_read_needs_without_showing_anything() {
    for backend in both_stand_ins() {
        let user = enrolled_user();
        let (plain, presence, bio, absent) =
            (unique("plain"), unique("presence"), unique("bio"), unique("absent"));
        with_sheet(&store(&plain, "x"), &backend, &user);
        with_sheet(&store_protected(&presence, "x", KeyringProtection::UserPresence), &backend, &user);
        with_sheet(&store_protected(&bio, "x", KeyringProtection::Biometric), &backend, &user);
        user.script([ScriptedOutcome::Cancel]);
        let need = |key: &str| need_in(TEST_SERVICE, key, &backend, &user);
        assert_eq!(need(&plain), KeyringNeed::Nothing);
        assert_eq!(need(&presence), KeyringNeed::UserPresence);
        assert_eq!(need(&bio), KeyringNeed::Biometric);
        assert_eq!(need(&absent), KeyringNeed::NotFound);
        assert_eq!(need("a\0b"), KeyringNeed::Error);
        assert_eq!(user.pending_outcomes(), 1, "a probe is no sheet: the script is untouched");
    }
}

#[test]
fn the_stand_in_says_what_it_can_do_for_each_level() {
    let backend = Backend::Memory;
    let nothing = StandInUser::new();
    assert_eq!(support_in(KeyringProtection::DeviceUnlocked, &backend, &nothing), KeyringSupport::Hardware);
    assert_eq!(support_in(KeyringProtection::UserPresence, &backend, &nothing), KeyringSupport::NotEnrolled);
    assert_eq!(support_in(KeyringProtection::Biometric, &backend, &nothing), KeyringSupport::NotEnrolled);
    nothing.enroll(false, true);
    assert_eq!(support_in(KeyringProtection::UserPresence, &backend, &nothing), KeyringSupport::Hardware);
    assert_eq!(support_in(KeyringProtection::Biometric, &backend, &nothing), KeyringSupport::NotEnrolled);
    nothing.enroll(true, true);
    assert_eq!(support_in(KeyringProtection::Biometric, &backend, &nothing), KeyringSupport::Hardware);
}

#[test]
fn an_unprotected_item_is_read_by_an_unlock_without_a_sheet_and_replaced_by_a_store() {
    for backend in both_stand_ins() {
        let user = enrolled_user();
        let key = unique("plain-unlock");
        with_sheet(&store(&key, "old"), &backend, &user);
        user.script([ScriptedOutcome::Cancel]);
        assert_eq!(with_sheet(&unlock(&key), &backend, &user), retrieved("old"));
        assert_eq!(user.pending_outcomes(), 1, "an unprotected item shows no sheet");
        // A protected store over it makes it protected; a plain store over that makes it plain.
        with_sheet(&store_protected(&key, "new", KeyringProtection::UserPresence), &backend, &user);
        assert_eq!(with_sheet(&get(&key), &backend, &user), KeyringResult::WouldPrompt);
        with_sheet(&store(&key, "plain-again"), &backend, &user);
        assert_eq!(with_sheet(&get(&key), &backend, &user), retrieved("plain-again"));
    }
}

#[test]
fn a_keyring_file_keeps_the_level_and_the_enrolment_it_was_stored_under() {
    let path = keyring_file("level");
    let backend = Backend::File(path.clone());
    let user = enrolled_user();
    let key = unique("level");
    with_sheet(&store_protected(&key, "s3cr3t", KeyringProtection::Biometric), &backend, &user);
    // The next run of the app: a user of its own, with the same enrolment.
    let next = StandInUser::new();
    next.enroll(true, true);
    assert_eq!(
        need_in(TEST_SERVICE, &key, &Backend::File(path.clone()), &next),
        KeyringNeed::Biometric
    );
    // A file with an entry the model does not know is an error, not a weaker item.
    std::fs::write(&path, r#"{"svc":{"k":{"secret":"s","protection":9}}}"#).expect("write the file");
    assert_eq!(
        answer_with("svc", &get("k"), &Backend::File(path.clone()), Interaction::Dialogs, &next),
        KeyringResult::Error
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_protected_op_whose_name_holds_a_nul_is_an_error_like_every_other() {
    let user = enrolled_user();
    assert_eq!(
        with_sheet(&store_protected("a\0b", "s", KeyringProtection::Biometric), &Backend::Memory, &user),
        KeyringResult::Error
    );
    assert_eq!(
        with_sheet(&unlock("a\0b"), &Backend::Memory, &user),
        KeyringResult::Error
    );
}

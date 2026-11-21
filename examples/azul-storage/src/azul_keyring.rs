//! [`KeyringStore`] over azul's keyring, for the apps. Feature `azul`.
//!
//! azul's `Keyring::get_blocking_in` / `store_blocking_in` / `delete_blocking_in` run the
//! keyring op on the calling thread and return its answer, in the keyring service of this
//! keyring's [`KeyringNamespace`]: the entries `CallbackInfo::keyring_*_in` of the same namespace
//! reads and writes on the UI thread (in a headless or E2E run azul's stand-in, which both
//! reach). Call it from an azul `Thread`, never from a UI callback: a read may wait for the
//! system's unlock prompt.
//!
//! A quiet keyring ([`AzulKeyring::quiet`]: azul's `*_quietly_in` ops) never shows a prompt:
//! where the system would have to ask the user it answers [`KeyringError::Denied`]. Background
//! work reads and writes with it, so a start or a timer never raises a dialog.
//!
//! # The tests' keyring (cargo feature `memory-keyring`)
//!
//! With the feature every [`AzulKeyring`] - [`azlin_keyring`] builds the Azlin apps' one, and no
//! other code builds one - keeps its entries in a map of this process: no call into azul, no
//! system keychain, no file, no lock folder of the user's, and [`KeyringStore::protection`] says
//! [`Protection::Memory`]. It is what a TEST build of an app needs: the libazul the tests link is
//! built without its own `test-hooks`, so it ignores `AZ_KEYRING=memory`, and a test that reached
//! `azlin_keyring()` wrote (and read) the owner's real login keychain (X-TESTFIX-AZUL2-4: two
//! AzDrive tests left their drives' keys there, and a rebuilt test binary then blocked on the
//! keychain's access prompt). The feature is named in the `[dev-dependencies]` of a crate whose
//! tests reach the keyring and nowhere else (`examples/azul-srcgate/tests/memory_keyring_gate.rs`
//! reads every manifest for it): a build that ships would lose every drive key at exit.
//!
//! The manifests are the first line of defence and the artifact is the second. With resolver 2 a plain
//! `cargo build --release` leaves a dev-dependency's features out, but `cargo build --release
//! --all-targets` (or `--workspace --all-targets`, or `--bins --tests`) builds azul-storage once for every
//! target of the invocation, with the feature on, and the AzMail or bridge binary that ships links that
//! one (X-DELTA-STRUCT3-5). So a build with the feature holds `MEMORY_KEYRING_MARKER` in its binary - a
//! `#[used]` static that [`azlin_keyring`] and every op of the map read, so the linker keeps it - and
//! `scripts/check_no_test_hooks.py` and `scripts/sign-release.sh release` refuse any binary that holds an
//! `AZLIN-BUILD-MARKER:` string, as they refuse azul-appkit's `test-hooks` build.
//!
//! One map for the process, the entries of every namespace apart in it, as the system keeps them.
//! Nothing in memory needs the user, so a quiet keyring ([`Asking::Never`]) answers what a user's
//! keyring does - the system's quiet ops answer [`KeyringError::Denied`] only where the system
//! would have had to ask.

use azul::misc::KeyringNamespace as AzulNamespace;

use crate::{
    keyring::{Asking, KeyringError, KeyringNamespace, KeyringStore, Protection, ENTRY_LOCK_WAIT},
    lock::{HeldLock, LockDir},
};

/// The system keyring through azul, in one namespace. Call only from an azul `Thread`.
#[derive(Debug, Clone)]
pub struct AzulKeyring {
    /// The keyring service every entry of this keyring lives in.
    namespace: KeyringNamespace,
    /// Never show the system's prompt ([`AzulKeyring::quiet`]): background work's keyring.
    quiet: bool,
    /// The folder of the locks every process of this user shares, which the entries changed in
    /// several steps are held under ([`KeyringStore::hold`]); `None`: this process's threads only.
    locks: Option<LockDir>,
}

impl AzulKeyring {
    /// The keyring of `namespace` (the Azlin apps: [`KeyringNamespace::azlin_shared`]); an op
    /// may show the system's prompt.
    #[must_use]
    pub fn new(namespace: KeyringNamespace) -> Self {
        AzulKeyring {
            namespace,
            quiet: false,
            locks: None,
        }
    }

    /// The keyring of `namespace` that never shows a prompt: an op the system would have to
    /// ask the user for answers [`KeyringError::Denied`].
    #[must_use]
    pub fn quiet(namespace: KeyringNamespace) -> Self {
        AzulKeyring {
            namespace,
            quiet: true,
            locks: None,
        }
    }

    /// This keyring with `locks` as the folder its entries are held under: every app and window of
    /// the user that reaches the same entries takes turns at the same locks.
    #[must_use]
    pub fn with_locks(mut self, locks: LockDir) -> Self {
        self.locks = Some(locks);
        self
    }

    /// Whether this keyring never shows a prompt.
    #[must_use]
    pub fn is_quiet(&self) -> bool {
        self.quiet
    }

    /// The namespace this keyring reads and writes.
    #[must_use]
    pub fn namespace(&self) -> &KeyringNamespace {
        &self.namespace
    }
}

/// The words a build with the cargo feature `memory-keyring` carries in its binary: the Azlin apps'
/// keyring is then a map of the process and every drive key is gone at exit, so nothing signed and
/// shipped may be one - and cargo unifies features across one build, so `cargo build --release
/// --all-targets` can turn the feature on for a binary that ships (X-DELTA-STRUCT3-5). A `#[used]`
/// static that [`azlin_keyring`] and every op of the map read holds them. The prefix is the one
/// `scripts/sign-release.sh release` and `scripts/check_no_test_hooks.py` refuse, as they refuse
/// azul-appkit's `anchors::TEST_HOOKS_MARKER`. A build without the feature has no such constant and
/// no such static, so its binary does not hold the bytes.
#[cfg(feature = "memory-keyring")]
pub const MEMORY_KEYRING_MARKER: &str =
    "AZLIN-BUILD-MARKER:memory-keyring:the-azlin-keyring-of-this-build-is-a-map-of-the-process";

/// The keyring of the Azlin apps - AzDrive, AzMail, the bridge - in their shared namespace: the
/// ONE place such a keyring is built (the K5/K7 policy of what a keyring is, and the locks its
/// entries are changed under, are applied here and nowhere else). `asking` says whether it may
/// show the system's prompt: `Asking::User` for what the user asked for, `Asking::Never` for
/// background work. Its entries are held under the locks every process of this user shares
/// (`<config dir>/azlin/locks/<namespace>`) when the system names a configuration folder. In a
/// test build (cargo feature `memory-keyring`, see the module's doc) it is a map of this process,
/// held under this process's locks, and the binary holds `MEMORY_KEYRING_MARKER`.
#[must_use]
pub fn azlin_keyring(asking: Asking) -> AzulKeyring {
    // A binary that builds the Azlin keyring in a build with the feature keeps the marker.
    #[cfg(feature = "memory-keyring")]
    backing::keep_marker();
    let namespace = KeyringNamespace::azlin_shared();
    let keyring = match asking {
        Asking::User => AzulKeyring::new(namespace),
        Asking::Never => AzulKeyring::quiet(namespace),
    };
    backing::shared_locks(keyring)
}

/// azul's form of `namespace`, for a UI callback's `CallbackInfo::keyring_*_in`: the entries
/// it reads and writes are this crate's [`AzulKeyring`]'s of the same namespace.
#[must_use]
pub fn azul_namespace(namespace: &KeyringNamespace) -> AzulNamespace {
    AzulNamespace::create(namespace.as_str())
}

impl KeyringStore for AzulKeyring {
    fn get(&self, key: &str) -> Result<Option<String>, KeyringError> {
        backing::get(&self.namespace, self.quiet, key)
    }

    fn set(&self, key: &str, secret: &str) -> Result<(), KeyringError> {
        backing::set(&self.namespace, self.quiet, key, secret)
    }

    fn delete(&self, key: &str) -> Result<(), KeyringError> {
        backing::delete(&self.namespace, self.quiet, key)
    }

    fn protection(&self) -> Protection {
        backing::PROTECTION
    }

    fn hold(&self, key: &str) -> Result<HeldLock, KeyringError> {
        match &self.locks {
            Some(locks) => locks.lock(key, ENTRY_LOCK_WAIT),
            None => crate::lock::in_process(key, ENTRY_LOCK_WAIT),
        }
        .map_err(|e| KeyringError::Failed(e.to_string()))
    }
}

/// Where the ops of every [`AzulKeyring`] go: the system's keyring through azul. The one of the two
/// modules a build has (the other is the test build's, below), so a test build cannot reach this
/// one at all.
#[cfg(not(feature = "memory-keyring"))]
mod backing {
    use azul::{error::KeyringResult, misc::Keyring};

    use super::{azul_namespace, AzulKeyring};
    use crate::{
        keyring::{KeyringError, KeyringNamespace, Protection},
        lock::LockDir,
    };

    /// The operating system's keyring.
    pub(super) const PROTECTION: Protection = Protection::Os;

    /// `keyring` with the folder of the locks every process of this user shares: its entries are
    /// changed in several steps under them, so two windows or two apps take turns.
    pub(super) fn shared_locks(keyring: AzulKeyring) -> AzulKeyring {
        let locks = crate::lock::user_config_dir().map(|config| {
            LockDir::new(
                crate::lock::keyring_lock_root(&config)
                    .join(crate::lock::namespace_folder(keyring.namespace().as_str())),
            )
        });
        match locks {
            Some(locks) => keyring.with_locks(locks),
            None => keyring,
        }
    }

    /// What a keyring answer that is no success says.
    fn error_of(answer: &KeyringResult) -> KeyringError {
        match answer {
            KeyringResult::Denied => KeyringError::Denied,
            KeyringResult::Unavailable => KeyringError::Unavailable,
            KeyringResult::NotFound => KeyringError::Failed(String::from("the entry is not there")),
            KeyringResult::Error => {
                KeyringError::Failed(String::from("the keyring reported an error"))
            }
            _ => KeyringError::Failed(String::from("the keyring answered something else")),
        }
    }

    pub(super) fn get(
        namespace: &KeyringNamespace,
        quiet: bool,
        key: &str,
    ) -> Result<Option<String>, KeyringError> {
        let service = azul_namespace(namespace);
        let answer = if quiet {
            Keyring::get_quietly_in(service, key)
        } else {
            Keyring::get_blocking_in(service, key)
        };
        match answer {
            KeyringResult::Retrieved(secret) => Ok(Some(secret.as_str().to_string())),
            KeyringResult::NotFound => Ok(None),
            other => Err(error_of(&other)),
        }
    }

    pub(super) fn set(
        namespace: &KeyringNamespace,
        quiet: bool,
        key: &str,
        secret: &str,
    ) -> Result<(), KeyringError> {
        let service = azul_namespace(namespace);
        let answer = if quiet {
            Keyring::store_quietly_in(service, key, secret, false)
        } else {
            Keyring::store_blocking_in(service, key, secret, false)
        };
        match answer {
            KeyringResult::Stored => Ok(()),
            other => Err(error_of(&other)),
        }
    }

    pub(super) fn delete(
        namespace: &KeyringNamespace,
        quiet: bool,
        key: &str,
    ) -> Result<(), KeyringError> {
        let service = azul_namespace(namespace);
        let answer = if quiet {
            Keyring::delete_quietly_in(service, key)
        } else {
            Keyring::delete_blocking_in(service, key)
        };
        match answer {
            KeyringResult::Deleted | KeyringResult::NotFound => Ok(()),
            other => Err(error_of(&other)),
        }
    }
}

/// Where the ops of every [`AzulKeyring`] go in a test build (cargo feature `memory-keyring`): a map
/// of this process, one for every keyring that is built, the entries of each namespace apart.
/// Nothing here calls azul, the system or the disk.
#[cfg(feature = "memory-keyring")]
mod backing {
    use std::{
        collections::BTreeMap,
        sync::{Mutex, MutexGuard, PoisonError},
    };

    use super::AzulKeyring;
    use crate::keyring::{KeyringError, KeyringNamespace, Protection};

    /// Entries by (namespace, entry name).
    type Entries = BTreeMap<(String, String), String>;

    /// The process's keyring.
    static ENTRIES: Mutex<Entries> = Mutex::new(BTreeMap::new());

    /// The memory of this process: nothing is kept for the next run.
    pub(super) const PROTECTION: Protection = Protection::Memory;

    /// The marker in this build's binary.
    #[used]
    static MARKER_IN_THIS_BINARY: &str = super::MEMORY_KEYRING_MARKER;

    /// Keeps the marker in every binary that reaches the map (a linker that drops what nothing
    /// reads would drop it otherwise): [`entries`], the one door of every op, and
    /// [`azlin_keyring`](super::azlin_keyring) call it.
    pub(super) fn keep_marker() {
        std::hint::black_box(MARKER_IN_THIS_BINARY);
    }

    /// The map; a test that panicked while it held it leaves it as it was, for the tests after it.
    fn entries() -> MutexGuard<'static, Entries> {
        keep_marker();
        ENTRIES.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn at(namespace: &KeyringNamespace, key: &str) -> (String, String) {
        (namespace.as_str().to_string(), key.to_string())
    }

    /// `keyring` as it is: the map is this process's, so the locks that hold its entries are too
    /// (no lock folder in the user's configuration folder).
    pub(super) fn shared_locks(keyring: AzulKeyring) -> AzulKeyring {
        keyring
    }

    pub(super) fn get(
        namespace: &KeyringNamespace,
        _quiet: bool,
        key: &str,
    ) -> Result<Option<String>, KeyringError> {
        Ok(entries().get(&at(namespace, key)).cloned())
    }

    pub(super) fn set(
        namespace: &KeyringNamespace,
        _quiet: bool,
        key: &str,
        secret: &str,
    ) -> Result<(), KeyringError> {
        entries().insert(at(namespace, key), secret.to_string());
        Ok(())
    }

    pub(super) fn delete(
        namespace: &KeyringNamespace,
        _quiet: bool,
        key: &str,
    ) -> Result<(), KeyringError> {
        entries().remove(&at(namespace, key));
        Ok(())
    }
}

#[cfg(all(test, feature = "memory-keyring"))]
mod tests {
    use super::*;

    /// X-TESTFIX-AZUL2-4: what the Azlin apps' keyring is in a test build - a map of this process, whose
    /// entries read back, replace, and go with a delete (one that is not there is no error).
    #[test]
    fn the_azlin_keyring_of_a_test_build_is_a_map_that_reads_back_what_was_stored() {
        let keyring = azlin_keyring(Asking::User);
        assert_eq!(keyring.protection(), Protection::Memory);
        let entry = "azul-storage/tests/memory-keyring/round-trip";
        assert_eq!(keyring.get(entry), Ok(None));
        keyring.set(entry, "first").unwrap();
        assert_eq!(keyring.get(entry).unwrap().as_deref(), Some("first"));
        keyring.set(entry, "second").unwrap();
        assert_eq!(
            keyring.get(entry).unwrap().as_deref(),
            Some("second"),
            "a store replaces what was there"
        );
        keyring.delete(entry).unwrap();
        keyring.delete(entry).unwrap();
        assert_eq!(keyring.get(entry), Ok(None));
    }

    /// One map for the process: what a keyring that may ask stores, a quiet one reads (and the other
    /// way), as the system's two ops reach the same entries; the entries of two namespaces are
    /// apart.
    #[test]
    fn the_two_askings_share_one_map_and_two_namespaces_are_apart() {
        let entry = "azul-storage/tests/memory-keyring/shared";
        let user = azlin_keyring(Asking::User);
        let never = azlin_keyring(Asking::Never);
        assert!(!user.is_quiet() && never.is_quiet());
        user.set(entry, "by the user").unwrap();
        assert_eq!(never.get(entry).unwrap().as_deref(), Some("by the user"));
        never.set(entry, "in the background").unwrap();
        assert_eq!(
            user.get(entry).unwrap().as_deref(),
            Some("in the background")
        );
        let other = AzulKeyring::new(KeyringNamespace::named("org.example.memory-keyring-test"));
        assert_eq!(
            other.get(entry),
            Ok(None),
            "the entry is the Azlin namespace's, not another's"
        );
        other.set(entry, "elsewhere").unwrap();
        assert_eq!(
            user.get(entry).unwrap().as_deref(),
            Some("in the background")
        );
        other.delete(entry).unwrap();
        never.delete(entry).unwrap();
        assert_eq!(user.get(entry), Ok(None));
    }

    /// Whether the file at `path` holds `needle` (read in chunks; a needle cut by a chunk's end is
    /// found).
    fn file_holds(path: &std::path::Path, needle: &[u8]) -> std::io::Result<bool> {
        use std::io::Read;
        let keep = needle.len() - 1;
        let mut buf = vec![0_u8; (1 << 20) + keep];
        let mut carried = 0;
        let mut file = std::fs::File::open(path)?;
        loop {
            let read = file.read(&mut buf[carried..])?;
            if read == 0 {
                return Ok(false);
            }
            let end = carried + read;
            let mut from = 0;
            while let Some(at) = buf[from..end].iter().position(|b| *b == needle[0]) {
                let start = from + at;
                if buf[start..end].starts_with(needle) {
                    return Ok(true);
                }
                from = start + 1;
            }
            // What may be the start of a needle that the next read finishes: the last `keep` bytes.
            carried = keep.min(end);
            buf.copy_within(end - carried..end, 0);
        }
    }

    /// X-DELTA-STRUCT3-5: a build with the feature holds the marker the release script refuses, in its
    /// binary - this test binary is such a build, and the marker is looked for in the file the process
    /// runs from. The needle is the marker's prefix read backwards, so that the test holds no copy of the
    /// bytes it looks for and never names the constant (either would put them into this binary whatever
    /// the linker kept: the test would pass on a build whose static was dropped). The prefix is what
    /// `scripts/check_no_test_hooks.py` refuses; `scripts/test_sign_release.py` reads the constant's
    /// text from this file and sees both scripts refuse it.
    #[test]
    fn x_delta_struct3_5_a_build_with_the_feature_holds_the_marker_in_its_binary() {
        // What every binary that builds the Azlin keyring does; the map's own ops call it as well.
        let keyring = azlin_keyring(Asking::User);
        assert_eq!(keyring.protection(), Protection::Memory);
        let needle: Vec<u8> = ":gniryek-yromem:REKRAM-DLIUB-NILZA".bytes().rev().collect();
        let exe = std::env::current_exe().expect("the path of the test binary");
        assert!(
            file_holds(&exe, &needle).expect("the test binary is readable"),
            "{} is a build with the feature `memory-keyring` and holds no marker: \
             `scripts/check_no_test_hooks.py` could not tell a shipped binary that has the feature from \
             one that has not",
            exe.display()
        );
    }

    /// The reader's own cases: a needle at the start, the middle and the very end of a file, one cut
    /// by the end of a chunk, and none.
    #[test]
    fn the_marker_reader_finds_a_needle_wherever_it_is_and_none_where_there_is_none() {
        let dir = std::env::temp_dir().join(format!("azul-keyring-marker-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let needle = b"needle-bytes";
        let chunk = 1_usize << 20;
        let cases: [(&str, Vec<u8>, bool); 6] = [
            ("empty", Vec::new(), false),
            ("start", [&needle[..], b"tail"].concat(), true),
            ("end", [&b"head"[..], needle].concat(), true),
            (
                "cut",
                [vec![b'n'; chunk - 5], needle.to_vec()].concat(),
                true,
            ),
            (
                "cut-at-the-edge",
                [vec![b'x'; chunk - 1], needle.to_vec()].concat(),
                true,
            ),
            (
                "partial",
                [vec![b'x'; chunk + 9], needle[..needle.len() - 1].to_vec()].concat(),
                false,
            ),
        ];
        for (name, bytes, holds) in cases {
            let path = dir.join(name);
            std::fs::write(&path, bytes).unwrap();
            assert_eq!(file_holds(&path, needle).unwrap(), holds, "{name}");
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Held in this process only: the keyring `azlin_keyring` builds names no lock folder of the
    /// user's (nothing is written to the configuration folder), and an entry is held and let go.
    #[test]
    fn the_entries_of_a_test_build_are_held_in_process_and_no_lock_folder_is_named() {
        let keyring = azlin_keyring(Asking::User);
        assert!(keyring.locks.is_none(), "{keyring:?}");
        let held = keyring
            .hold("azul-storage/tests/memory-keyring/held")
            .unwrap();
        drop(held);
        let again = keyring
            .hold("azul-storage/tests/memory-keyring/held")
            .unwrap();
        drop(again);
    }
}

//! Gate: what a drive leaves on a device is named in one place, and a keyring says what it is
//! (ARCH28 RF53 / AR5b-4; AZ-K7, AZ-K6, LANE-AZ-2, X-AR5b-14).
//!
//! A drive has at least eight kinds of keyring entry; every flow listed "its" entries by hand and
//! a removed drive left the rest behind, readable by every program of the user. `KeyEntry`
//! (`azul-storage/src/footprint.rs`) is the one place that writes an entry's name, a golden test
//! pins the names (users' keys are in them), and `forget_drive` takes the drive off the device
//! through the stores. Five rules keep it so:
//!
//! - a string that starts with `azul-storage/` is the first half of an entry name written by hand:
//!   it belongs to `KeyEntry::name` (or `contact_key_entry`), nowhere else (one allowance, below:
//!   the label bound into a sealed small file's associated data, which names no entry);
//! - nor is the kind of an entry written as the END of a name built from another
//!   (`format!("{}/device-key", session_name)`): a string with a `/` before the word of a kind
//!   (`drive-key`, `vault-key`, `device-key`, ...) is the same hand-written name, one the first
//!   rule cannot see - the device key was named so and no removal found it
//!   (X-DELTA-AZDRIVE-KEYS-32; no allowance);
//! - every `impl KeyringStore for X` in shipped code has a `fn protection`: the keyring says whether
//!   it is the system's, a plain file (which takes no drive key unless the user allowed it) or
//!   memory. The default of the trait is `Memory` for a test's stand-in; a store that keeps entries
//!   on a disk must not be left with it by a copy of another store's impl (no allowance);
//! - `AzulKeyring::new(` and `::quiet(` are called in `azlin_keyring` only - the one place the
//!   Azlin apps' keyring is built, where its locks and its policy are applied (no other
//!   allowance: AzMail's five went over to it);
//! - `RemovalProof::from_checked_code(` is called in one place: `RecoveryState::removal_proof`,
//!   which asks whether the drive's recovery code was typed back right. The proof lets a removed
//!   drive's keys, paid months and open calls go; whoever made one by hand would make the evidence
//!   whatever they believed at that line.
//!
//! Comments and test items are not read.

use std::path::{Path, PathBuf};

use azul_srcgate::{calls, strings, Allow, Budget, Gate, Located, Orphans, Root, Rule};

const ROOTS: &[Root] = &[
    Root {
        dir: "examples/azul-storage/src",
        min_files: 55,
    },
    Root {
        dir: "examples/azcloud-kit/src",
        min_files: 55,
    },
    Root {
        dir: "examples/azul-drive/src",
        min_files: 65,
    },
    Root {
        dir: "examples/azul-appkit/src",
        min_files: 20,
    },
    Root {
        dir: "examples/azul-bridge/src",
        min_files: 15,
    },
    Root {
        dir: "examples/azcloud-api/src",
        min_files: 1,
    },
    Root {
        dir: "examples/azul-mail-core/src",
        min_files: 8,
    },
    Root {
        dir: "examples/azul-mail/src",
        min_files: 10,
    },
];

const BY_HAND: &str = "entry-name-written-by-hand";
const SUFFIXED: &str = "entry-name-suffixed-by-hand";

/// The words of the kinds of entry that hang below another's name or beside it
/// (`KeyEntry::kind`), as the last part of a name: not `s3` and `azlin`, which are also words of
/// the rest of the code.
const KIND_WORDS: &[&str] = &[
    "drive-key",
    "member-key",
    "drive-key-planned",
    "drive-key-previous",
    "drive-key-next",
    "recovery-request",
    "vault-key",
    "device-key",
];
const NO_PROTECTION: &str = "keyring-store-without-protection";
const BUILT_BY_HAND: &str = "azul-keyring-built-by-hand";
const PROOF_BY_HAND: &str = "removal-proof-made-by-hand";

/// Where an `AzulKeyring` is built by hand, and who changes it, where a proof is made, and the one
/// string that starts like an entry's name and is none. TO SHRINK.
const ALLOWED: &[Allow] = &[
    Allow::exactly(
        "examples/azul-storage/src/crypto/blob.rs",
        BY_HAND,
        1,
        "AZ-K13, `AAD_LABEL`: the domain label bound into the associated data of every sealed small \
         file (the period vault's). It is a format constant - respelled, no file sealed so far would \
         open - and no keyring entry's name: `KeyEntry` has no `blob` kind, and nothing reads a \
         keyring with it",
    ),
    Allow::exactly(
        "examples/azul-storage/src/azul_keyring.rs",
        BUILT_BY_HAND,
        2,
        "azlin_keyring: the one place (`new` for a keyring that may ask, `quiet` for one that never does)",
    ),
    Allow::exactly(
        "examples/azul-drive/src/recovery_health.rs",
        PROOF_BY_HAND,
        1,
        "RecoveryState::removal_proof: the one place a proof is made, from a recovery code that \
         was typed back right at a time that is not after now",
    ),
];

/// The index of the closer that matches the opener at `open`, or the end.
fn close_of(t: &[Located], open: usize) -> usize {
    let mut depth = 0usize;
    for (i, x) in t.iter().enumerate().skip(open) {
        if let azul_srcgate::Tok::Punct(c) = &x.tok {
            match c {
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return i;
                    }
                }
                _ => {}
            }
        }
    }
    t.len()
}

/// The lines of the `impl KeyringStore for X { .. }` blocks that have no `fn protection`.
fn stores_without_protection(t: &[Located]) -> Vec<usize> {
    let mut found = Vec::new();
    for (i, x) in t.iter().enumerate() {
        if !x.tok.is_ident("impl") {
            continue;
        }
        let Some(open) = (i..t.len()).find(|&k| t[k].tok.is_punct('{')) else {
            break;
        };
        let header: Vec<&str> = t[i + 1..open]
            .iter()
            .filter_map(|y| y.tok.ident())
            .collect();
        let is_store = header
            .iter()
            .position(|w| *w == "KeyringStore")
            .is_some_and(|p| header.get(p + 1) == Some(&"for"));
        if !is_store {
            continue;
        }
        let body = &t[open..close_of(t, open).min(t.len())];
        let says = body
            .windows(2)
            .any(|w| w[0].tok.is_ident("fn") && w[1].tok.is_ident("protection"));
        if !says {
            found.push(x.line);
        }
    }
    found
}

/// A string with a `/` before the word of a kind of entry at its end.
fn suffixed_by_hand(text: &str) -> bool {
    text.rsplit_once('/')
        .is_some_and(|(_, last)| KIND_WORDS.contains(&last))
}

fn rules() -> Vec<Rule> {
    vec![
        Rule::new(
            BY_HAND,
            |t| strings(t, |s| s.starts_with("azul-storage/")),
            &[
                "let name = format!(\"azul-storage/drive-key/{drive}\");",
                "let session = keyring.get(\"azul-storage/s3/d_1\");",
            ],
            &[
                "let service = \"azul-storage\";",
                "let file = dir.join(\"azul-storage\").join(\"drives.json\");",
                "let name = KeyEntry::DriveKey.name(drive);",
                "let other = \"azul-storage.drives\";",
            ],
        ),
        Rule::new(
            SUFFIXED,
            |t| strings(t, suffixed_by_hand),
            &[
                "let name = format!(\"{}/device-key\", session_name(drive));",
                "let key = base + \"/vault-key\";",
                "let n = join(\"x/y\", \"/drive-key-next\");",
            ],
            &[
                "let name = KeyEntry::DeviceKey.name(drive);",
                "let kind = \"device-key\";",
                "let url = \"https://example.test/api/keys\";",
                "let field = \"drive-key and the rest\";",
                "let path = \"docs/drive-keys\";",
            ],
        ),
        Rule::new(
            NO_PROTECTION,
            stores_without_protection,
            &[
                "impl KeyringStore for Plain { fn get(&self, k: &str) -> R { read(k) } }",
                "impl azul_storage::keyring::KeyringStore for Plain { fn set(&self, k: &str, s: &str) -> R { write(k, s) } }",
            ],
            &[
                "impl KeyringStore for Os { fn get(&self, k: &str) -> R { read(k) } fn protection(&self) -> Protection { Protection::Os } }",
                "impl Drive for Folder { fn get(&self, k: &str) -> R { read(k) } }",
                "impl Debug for KeyringStoreName { fn fmt(&self) {} }",
            ],
        ),
        Rule::new(
            BUILT_BY_HAND,
            |t| {
                let mut lines = calls(t, "AzulKeyring::new");
                lines.extend(calls(t, "AzulKeyring::quiet"));
                lines.sort_unstable();
                lines
            },
            &[
                "let k = AzulKeyring::new(KeyringNamespace::azlin_shared());",
                "let k = Arc::new(azul_storage::azul_keyring::AzulKeyring::quiet(ns));",
            ],
            &[
                "let k = azlin_keyring(Asking::User);",
                "let k = AzulKeyringStub::new();",
                "let k = Keyring::new(ns);",
            ],
        ),
        Rule::new(
            PROOF_BY_HAND,
            |t| calls(t, "RemovalProof::from_checked_code"),
            &[
                "let proof = RemovalProof::from_checked_code(drive);",
                "let proof = azul_storage::footprint::RemovalProof::from_checked_code(&id);",
            ],
            &[
                "let proof = state.removal_proof(now);",
                "pub fn from_checked_code(drive: &str) -> RemovalProof { x }",
                "let kept = RemovalProof::drive(&proof);",
            ],
        ),
    ]
}

fn gate(roots: &'static [Root], allowed: &'static [Allow]) -> Gate<'static> {
    Gate {
        name: "what a drive leaves on a device is named in one place",
        roots,
        rules: rules(),
        budget: Budget(allowed),
        orphans: Orphans::Ignore,
    }
}

#[test]
fn lane_az_2_an_entry_name_is_written_in_one_place_and_a_keyring_says_what_it_is() {
    gate(ROOTS, ALLOWED).run();
}

/// A tree on disk below a fresh base: (path below the base, text).
fn base_with(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let base = std::env::temp_dir().join(format!(
        "azul-srcgate-footprint-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&base);
    for (path, text) in files {
        let full = base.join(path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, text).unwrap();
    }
    base
}

const SMALL: &[Root] = &[Root {
    dir: "examples/azul-storage/src",
    min_files: 1,
}];

fn check(base: &Path) -> Result<usize, Vec<String>> {
    gate(SMALL, &[]).check(base).map(|hits| hits.len())
}

#[test]
fn lane_az_2_each_of_the_four_plants_is_caught_with_its_line() {
    let planted = "pub fn session(drive: &str) -> String {\n\
    format!(\"azul-storage/s3/{drive}\")\n\
}\n\
impl KeyringStore for Plain {\n\
    fn get(&self, key: &str) -> Result<Option<String>, KeyringError> {\n\
        read(key)\n\
    }\n\
}\n\
pub fn keyring() -> AzulKeyring {\n\
    AzulKeyring::new(KeyringNamespace::azlin_shared())\n\
}\n\
pub fn proof() -> RemovalProof {\n\
    RemovalProof::from_checked_code(\"d_1\")\n\
}\n";
    let base = base_with("planted", &[("examples/azul-storage/src/lib.rs", planted)]);
    let wrong = check(&base).expect_err("all four plants are findings");
    let _ = std::fs::remove_dir_all(&base);
    assert_eq!(wrong.len(), 4, "{wrong:?}");
    assert!(
        wrong
            .iter()
            .any(|w| w.contains("lib.rs:2") && w.contains(BY_HAND)),
        "{wrong:?}"
    );
    assert!(
        wrong
            .iter()
            .any(|w| w.contains("lib.rs:4") && w.contains(NO_PROTECTION)),
        "{wrong:?}"
    );
    assert!(
        wrong
            .iter()
            .any(|w| w.contains("lib.rs:10") && w.contains(BUILT_BY_HAND)),
        "{wrong:?}"
    );
    assert!(
        wrong
            .iter()
            .any(|w| w.contains("lib.rs:13") && w.contains(PROOF_BY_HAND)),
        "{wrong:?}"
    );
}

//! Gate: what a drive leaves on a device is named in one place, and a keyring says what it is
//! (ARCH28 RF53 / AR5b-4; AZ-K7, AZ-K6, LANE-AZ-2).
//!
//! A drive has at least eight kinds of keyring entry; every flow listed "its" entries by hand and
//! a removed drive left the rest behind, readable by every program of the user. `KeyEntry`
//! (`azul-storage/src/footprint.rs`) is the one place that writes an entry's name, a golden test
//! pins the names (users' keys are in them), and `forget_drive` takes the drive off the device
//! through the stores. Two rules keep it so, with no allowance at all:
//!
//! - a string that starts with `azul-storage/` is the first half of an entry name written by hand:
//!   it belongs to `KeyEntry::name` (or `contact_key_entry`), nowhere else;
//! - every `impl KeyringStore for X` in shipped code has a `fn protection`: the keyring says whether
//!   it is the system's, a plain file (which takes no drive key unless the user allowed it) or
//!   memory. The default of the trait is `Memory` for a test's stand-in; a store that keeps entries
//!   on a disk must not be left with it by a copy of another store's impl.
//!
//! Comments and test items are not read.

use std::path::{Path, PathBuf};

use azul_srcgate::{strings, Budget, Gate, Located, Orphans, Root, Rule};

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
const NO_PROTECTION: &str = "keyring-store-without-protection";

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
    ]
}

fn gate(roots: &'static [Root]) -> Gate<'static> {
    Gate {
        name: "what a drive leaves on a device is named in one place",
        roots,
        rules: rules(),
        budget: Budget(&[]),
        orphans: Orphans::Ignore,
    }
}

#[test]
fn lane_az_2_an_entry_name_is_written_in_one_place_and_a_keyring_says_what_it_is() {
    gate(ROOTS).run();
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
    gate(SMALL).check(base).map(|hits| hits.len())
}

#[test]
fn lane_az_2_a_name_written_by_hand_and_a_store_that_says_nothing_are_caught_with_their_lines() {
    let planted = "pub fn session(drive: &str) -> String {\n\
    format!(\"azul-storage/s3/{drive}\")\n\
}\n\
impl KeyringStore for Plain {\n\
    fn get(&self, key: &str) -> Result<Option<String>, KeyringError> {\n\
        read(key)\n\
    }\n\
}\n";
    let base = base_with("planted", &[("examples/azul-storage/src/lib.rs", planted)]);
    let wrong = check(&base).expect_err("both plants are findings");
    let _ = std::fs::remove_dir_all(&base);
    assert_eq!(wrong.len(), 2, "{wrong:?}");
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
}

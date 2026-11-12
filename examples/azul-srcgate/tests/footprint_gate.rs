//! Gate: what a drive leaves on a device is named in one place, and a keyring says what it is
//! (ARCH28 RF53 / AR5b-4; AZ-K7, AZ-K6, LANE-AZ-2, X-AR5b-14).
//!
//! A drive has at least eight kinds of keyring entry; every flow listed "its" entries by hand and
//! a removed drive left the rest behind, readable by every program of the user. `KeyEntry`
//! (`azul-storage/src/footprint.rs`) is the one place that writes an entry's name, a golden test
//! pins the names (users' keys are in them), and `forget_drive` takes the drive off the device
//! through the stores. Six rules keep it so:
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
//! - the witnesses of a removal are implemented in two places and made into evidence in the same two
//!   (two rules, X-DELTA-INTEGRITY3-1): `impl RecoveryRecord for X` (what the app that keeps the record says of the
//!   drive's recovery code) and `impl PaidMonths for X` (the drive's vault's answer), and the calls that make
//!   them evidence - `TokenFacts::of(` and `VaultFacts::of(` - and the proof - `RemovalProof::from_permit(` -
//!   all in azul-drive's `recovery_health.rs` and `encryption.rs` (the allowances below, each counted).
//!
//! Why the last two rules are here. A `RemovalProof` is made from a key permit's Permit of a removal
//! (`RemovalProof::from_permit`) and a Permit by the judge (`keystate::ask`) and nowhere else: the type stops a
//! proof made from a NAME (it was `from_checked_code(&str)`, protected by a count of its callers, which went with
//! the function). It does not stop a witness that lies: `impl RecoveryRecord for X { fn code_checked(..) ->
//! Read::Found(0) .. }`, `TokenFacts::of(&X)` and `ask(Op::Remove, ..)` give the judge a code that was "typed back
//! right", the judge permits and the keys and the paid months go. The old rule's purpose - one audited place
//! that turns "the code was checked" into the authority to erase a drive's keys - is what these rules count
//! now: a new removal path (AzMail, the kit) is a finding until its witness is read and listed.
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
const WITNESS_BY_HAND: &str = "removal-witness-implemented";
const EVIDENCE_BY_HAND: &str = "removal-evidence-made";

/// The traits a removal's evidence is read from (`keystate::RecoveryRecord`, `keystate::PaidMonths`).
const WITNESS_TRAITS: &[&str] = &["RecoveryRecord", "PaidMonths"];

/// Where an `AzulKeyring` is built by hand, where the witnesses of a removal are implemented and made into
/// evidence, and the one string that starts like an entry's name and is none. TO SHRINK.
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
        WITNESS_BY_HAND,
        1,
        "`impl RecoveryRecord for RecoveryState`: the one record of a drive's recovery code - AzDrive's own \
         settings, which say when the code was typed back right and whether the token server holds its key",
    ),
    Allow::exactly(
        "examples/azul-drive/src/encryption.rs",
        WITNESS_BY_HAND,
        1,
        "`impl PaidMonths for VaultCheck`: the one vault witness - it writes what the drive lacks, reads it \
         back and says whether months paid are only on this computer",
    ),
    Allow::exactly(
        "examples/azul-drive/src/recovery_health.rs",
        EVIDENCE_BY_HAND,
        1,
        "`token_facts`: the record's witness taken from the click - the one place the code's evidence is made \
         (`TokenFacts::no_record` makes none and is not counted)",
    ),
    Allow::exactly(
        "examples/azul-drive/src/encryption.rs",
        EVIDENCE_BY_HAND,
        2,
        "`removal_proof`: `VaultFacts::of` (the vault's evidence, asked only when it is the one fact missing) and \
         the one `RemovalProof::from_permit`, made from the Permit the judge gave for a removal",
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

/// The lines of the `impl RecoveryRecord for X` and `impl PaidMonths for X` headers (a trait named by a path
/// as well; a bound, `impl<T: PaidMonths> Show for T`, is no impl of it): a witness of a removal.
fn witnesses_implemented(t: &[Located]) -> Vec<usize> {
    let mut found = Vec::new();
    for (i, x) in t.iter().enumerate() {
        if !x.tok.is_ident("impl") {
            continue;
        }
        let Some(open) = (i..t.len()).find(|&k| t[k].tok.is_punct('{') || t[k].tok.is_punct(';'))
        else {
            break;
        };
        let header: Vec<&str> = t[i + 1..open]
            .iter()
            .filter_map(|y| y.tok.ident())
            .collect();
        let is_witness = header
            .iter()
            .enumerate()
            .any(|(p, word)| WITNESS_TRAITS.contains(word) && header.get(p + 1) == Some(&"for"));
        if is_witness {
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
            WITNESS_BY_HAND,
            witnesses_implemented,
            &[
                "impl RecoveryRecord for Liar { fn code_checked(&self) -> Read<u64> { Read::Found(0) } }",
                "impl azul_storage::crypto::keystate::PaidMonths for Rich { fn held_elsewhere(&self, d: &str) -> Read<bool> { Read::Found(true) } }",
            ],
            &[
                "impl Debug for RecoveryRecordName { fn fmt(&self) {} }",
                "impl<T: PaidMonths> Show for T { fn show(&self) {} }",
                "fn judge(record: &dyn RecoveryRecord) -> TokenFacts { x }",
                "impl Drop for PaidMonthsGuard { fn drop(&mut self) {} }",
            ],
        ),
        Rule::new(
            EVIDENCE_BY_HAND,
            |t| {
                let mut lines = calls(t, "TokenFacts::of");
                lines.extend(calls(t, "VaultFacts::of"));
                lines.extend(calls(t, "RemovalProof::from_permit"));
                lines.sort_unstable();
                lines
            },
            &[
                "e.token = TokenFacts::of(&record);",
                "e.vault = azul_storage::crypto::keystate::VaultFacts::of(&vault, drive);",
                "let proof = RemovalProof::from_permit(permit).ok();",
            ],
            &[
                "e.token = TokenFacts::no_record();",
                "e.vault = VaultFacts::no_vault();",
                "pub fn of(record: &dyn RecoveryRecord) -> TokenFacts { x }",
                "let facts = MyTokenFacts::of(&record);",
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
fn lane_az_2_each_of_the_three_plants_is_caught_with_its_line() {
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
}\n";
    let base = base_with("planted", &[("examples/azul-storage/src/lib.rs", planted)]);
    let wrong = check(&base).expect_err("all three plants are findings");
    let _ = std::fs::remove_dir_all(&base);
    assert_eq!(wrong.len(), 3, "{wrong:?}");
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
}

/// X-DELTA-INTEGRITY3-1: the type of `RemovalProof` stops a proof made from a name, not a witness that lies. A
/// `RecoveryRecord` that always says "checked", a vault that always says "kept elsewhere", the calls that make
/// them evidence and the call that makes the proof of the Permit they bought are each a finding with its line
/// - and the same text in a test module is not read.
#[test]
fn x_delta_integrity3_1_a_lying_witness_and_the_calls_that_turn_it_into_a_proof_are_caught() {
    let planted = "impl RecoveryRecord for Liar {\n\
    fn code_checked(&self) -> Read<u64> { Read::Found(0) }\n\
}\n\
impl azul_storage::crypto::keystate::PaidMonths for Rich {\n\
    fn held_elsewhere(&self, _drive: &str) -> Read<bool> { Read::Found(true) }\n\
}\n\
pub fn proof(permit: Permit) -> Option<RemovalProof> {\n\
    let mut e = Evidence::new(\"d_1\", 1);\n\
    e.token = TokenFacts::of(&Liar);\n\
    e.vault = VaultFacts::of(&Rich, \"d_1\");\n\
    RemovalProof::from_permit(permit).ok()\n\
}\n";
    let base = base_with("liar", &[("examples/azul-storage/src/lib.rs", planted)]);
    let wrong = check(&base).expect_err("the lying witness and its calls are findings");
    let _ = std::fs::remove_dir_all(&base);
    assert_eq!(wrong.len(), 5, "{wrong:?}");
    for (line, rule) in [
        (1, WITNESS_BY_HAND),
        (4, WITNESS_BY_HAND),
        (9, EVIDENCE_BY_HAND),
        (10, EVIDENCE_BY_HAND),
        (11, EVIDENCE_BY_HAND),
    ] {
        assert!(
            wrong
                .iter()
                .any(|w| w.contains(&format!("lib.rs:{line}")) && w.contains(rule)),
            "line {line} ({rule}): {wrong:?}"
        );
    }

    // The same text in a test module is a test's stand-in, not a witness of a removal: not read.
    let in_tests = format!("#[cfg(test)]\nmod tests {{\n{planted}}}\n");
    let base = base_with(
        "liar-in-tests",
        &[("examples/azul-storage/src/lib.rs", &in_tests)],
    );
    let checked = check(&base);
    let _ = std::fs::remove_dir_all(&base);
    assert_eq!(checked, Ok(0), "test items are not read");
}

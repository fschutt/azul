//! Gate: the bytes of an Azlin drive are named in azcloud-kit only, and an Azlin drive is opened in one
//! place (X-DELTA-AZDRIVE-KEYS-4, -9; ARCH29 C6d).
//!
//! `AzlinDrive` was the account (lockdown, the count of stored bytes, the session, the node list) AND the
//! bytes (`impl Drive`) in one handle, and 17 signatures of AzDrive took it: a job that held it could
//! `put` into the drive's bucket, below the encryption's gate - which is how a price notice reached an
//! encrypted drive's bucket in the clear (KEYS-4). The handle is two now: `AzlinAccount` (no `put`, no
//! `get`: what a job holds) and the bytes, `AzlinDrive` of `azcloud-kit/src/drive.rs`, a crate-private
//! type that `azcloud_kit::open_azlin` hands only to the app's gate. Two rules keep it so:
//!
//! - **`azlin-byte-store-named`**: the identifier `AzlinDrive` is written in azcloud-kit and nowhere else
//!   in shipped code (the compiler already refuses a path to a private type; this keeps a re-export, a
//!   same-named type or a doc that points a new caller at it out). Zero outside the crate.
//! - **`azlin-opened`**: every use of the name `open_azlin` - a call (`open_azlin(`,
//!   `azcloud_kit::open_azlin(`, `azcloud_kit::drive::open_azlin(`), an import under another name
//!   (`use .. open_azlin as ..`) and a function value (`.map(azcloud_kit::open_azlin)`) - but its
//!   definition: the one function that makes an account and its bytes, chooses the token server
//!   (`azlin_target`, KEYS-9) and puts the gate over the bytes. Each file that opens a drive is a counted
//!   line: a second opener is a second place the gate can be forgotten. It read the two path spellings
//!   `open_azlin(` and `azcloud_kit::open_azlin(` only (X-DELTA-STRUCT3-12).
//! - **`azlin-byte-store-made`**: every use of `AzlinDrive::over`, the one function that puts the bytes over
//!   an account: two places make them - `open_azlin` and `AzlinAccount::unlocked`, the vault's own handle -
//!   both in `azcloud-kit/src/drive.rs`. The kit was allowed by directory, uncounted: a new kit function
//!   `pub fn bytes(a: &Arc<AzlinAccount>) -> Arc<dyn Drive> { Arc::new(AzlinDrive::over(a.clone())) }` would
//!   have handed the bytes of an Azlin drive to every caller below the gate with every gate green
//!   (X-DELTA-STRUCT3-12).
//!
//! The kit's own files are listed one by one with their counts: `drive.rs` names the bytes and makes them, the
//! crate's `lib.rs` re-exports the opener. Comments, strings and test items are not read (a file only a test
//! build compiles is none of the gate's); the counts are exact and only go down.

use std::path::{Path, PathBuf};

use azul_srcgate::{idents, Allow, Budget, Gate, Located, Orphans, Root, Rule};

const ROOTS: &[Root] = &[Root {
    dir: "examples",
    min_files: 500,
}];

const BYTE_STORE: &str = "azlin-byte-store-named";
const OPENED: &str = "azlin-opened";
const MADE: &str = "azlin-byte-store-made";

/// Every use of `open_azlin` that is not its definition (`fn open_azlin`): whatever spelling reaches the
/// function - a bare call, a path call (`azcloud_kit::open_azlin(`, `azcloud_kit::drive::open_azlin(`), an
/// import under another name and a function value.
fn opened(t: &[Located]) -> Vec<usize> {
    t.iter()
        .enumerate()
        .filter(|(at, x)| {
            x.tok.is_ident("open_azlin") && !(*at > 0 && t[*at - 1].tok.is_ident("fn"))
        })
        .map(|(_, x)| x.line)
        .collect()
}

/// Every use of the path `AzlinDrive::over` (a call or a function value).
fn made(t: &[Located]) -> Vec<usize> {
    t.iter()
        .enumerate()
        .filter(|(at, x)| {
            x.tok.is_ident("over")
                && *at >= 3
                && t[*at - 1].tok.is_punct(':')
                && t[*at - 2].tok.is_punct(':')
                && t[*at - 3].tok.is_ident("AzlinDrive")
        })
        .map(|(_, x)| x.line)
        .collect()
}

fn rules() -> Vec<Rule> {
    vec![
        Rule::new(
            BYTE_STORE,
            |t| idents(t, "AzlinDrive"),
            &[
                "fn jobs(azlin: Option<Arc<azcloud_kit::AzlinDrive>>) {}",
                "let drive = AzlinDrive::new(&entry, session, url, keyring, transports, on_rotated);",
                "pub use drive::AzlinDrive;",
            ],
            &[
                "fn jobs(azlin: Option<Arc<azcloud_kit::AzlinAccount>>) {}",
                "let azlin_drive = 1;",
                "let (account, drive) = azcloud_kit::open_azlin(&entry, open, gate)?;",
                "let AzlinDrives = 1;",
            ],
        ),
        Rule::new(
            OPENED,
            opened,
            &[
                "let (account, drive) = azcloud_kit::open_azlin(&entry, open, gate)?;",
                "let opened = open_azlin(&entry, open, gate);",
                // The spellings the two call forms above missed (X-DELTA-STRUCT3-12): the path through the
                // module, an import under another name, a function value.
                "let (account, drive) = azcloud_kit::drive::open_azlin(&entry, open, gate)?;",
                "use azcloud_kit::open_azlin as open_drive;",
                "let all = entries.iter().map(azcloud_kit::open_azlin);",
                "let f: fn(&DriveEntry) = open_azlin;",
            ],
            &[
                "pub fn open_azlin(entry: &DriveEntry) {}",
                "let x = Slot::open_azlin_drive(1);",
                "let t = tool.open_azlin_x(1);",
                "let open_azlins = 1;",
            ],
        ),
        Rule::new(
            MADE,
            made,
            &[
                "let bytes: Arc<dyn Drive> = Arc::new(AzlinDrive::over(Arc::clone(&account)));",
                "let bytes = azcloud_kit::drive::AzlinDrive::over(account.clone());",
                "let make = AzlinDrive::over;",
            ],
            &[
                "let drive = Other::over(1);",
                "let over = 1;",
                "let t = AzlinDrive::overlay(1);",
                "let x = AzlinDrive.over(1);",
            ],
        ),
    ]
}

/// Where the rules may be broken, and why. TO SHRINK.
const ALLOWED: &[Allow] = &[
    Allow::exactly(
        "examples/azcloud-kit/src/drive.rs",
        BYTE_STORE,
        9,
        "the bytes of an Azlin drive are azcloud-kit's: `drive.rs` defines them (the struct, its impls, the macro call) and puts \
         them over an account (the two places the `azlin-byte-store-made` rule counts)",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/drive.rs",
        MADE,
        2,
        "`open_azlin` (the bytes it hands to the gate) and `AzlinAccount::unlocked` (the vault's own handle, \
         below the gate on purpose: its bytes go to `UnlockedDrive` and nothing else)",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/lib.rs",
        OPENED,
        1,
        "the re-export of `open_azlin` (`pub use drive::{..}`)",
    ),
    Allow::exactly(
        "examples/azul-drive/src/lib.rs",
        OPENED,
        1,
        "Slot::open_azlin_in: the one opener of an Azlin drive in the apps; its gate is `encryption::azlin_stack`, \
         the only function that makes the AutoEncrypted `open_azlin` takes (AzMail and the bridge open their own \
         drives through `mail_drive::wrap`, over a plain S3 bucket they build from a session)",
    ),
];

fn gate(roots: &'static [Root], allowed: &'static [Allow]) -> Gate<'static> {
    Gate {
        name: "the bytes of an Azlin drive are named in azcloud-kit only, and one function opens a drive",
        roots,
        rules: rules(),
        budget: Budget(allowed),
        orphans: Orphans::Ignore,
    }
}

#[test]
fn the_bytes_of_an_azlin_drive_are_named_in_the_kit_only_and_one_place_opens_a_drive() {
    gate(ROOTS, ALLOWED).run();
}

/// A tree on disk below a fresh base: (path below the base, text).
fn base_with(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let base = std::env::temp_dir().join(format!(
        "azul-srcgate-azlin-handle-{name}-{}",
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

const SMALL: &[Root] = &[
    Root {
        dir: "examples/azcloud-kit/src",
        min_files: 1,
    },
    Root {
        dir: "examples/azul-drive/src",
        min_files: 1,
    },
];

const LISTED: &[Allow] = &[
    Allow::exactly(
        "examples/azcloud-kit/src/drive.rs",
        BYTE_STORE,
        4,
        "the planted kit's drive.rs: the struct, its impl and the two places that put it over an account",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/drive.rs",
        MADE,
        2,
        "the planted kit's two places that make the bytes",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/lib.rs",
        OPENED,
        1,
        "the planted kit's re-export",
    ),
    Allow::exactly(
        "examples/azul-drive/src/lib.rs",
        OPENED,
        1,
        "the planted opener",
    ),
];

fn check(base: &Path, allowed: &'static [Allow]) -> Result<usize, Vec<String>> {
    gate(SMALL, allowed).check(base).map(|hits| hits.len())
}

/// The planted kit: the bytes are defined and made in `drive.rs` (twice: the opener and the vault's handle),
/// `lib.rs` re-exports the opener.
const KIT_LIB: &str = "mod drive;\npub use drive::open_azlin;\n";
const KIT_DRIVE: &str = "pub(crate) struct AzlinDrive {}\nimpl AzlinDrive { fn over() {} }\npub fn open_azlin(entry: &DriveEntry) {\n    let bytes = AzlinDrive::over(account);\n}\npub fn unlocked(account: A) {\n    let bytes = AzlinDrive::over(account);\n}\n";

/// A job that names the bytes of an Azlin drive - the shape of the 17 signatures - is a finding with its
/// line; the kit may name them in the files it lists; a listed opener is counted, a second one in the same file
/// is a new site, and an opener in a file nobody listed is a finding.
#[test]
fn a_planted_job_that_names_the_bytes_and_a_second_opener_are_caught() {
    let drive_lib = [
        "pub fn open(slot: &mut Slot) {",
        "    let _ = azcloud_kit::open_azlin(&slot.entry, open, gate);",
        "}",
        "",
    ]
    .join("\n");
    let jobs = [
        "pub struct Renew {",
        "    pub azlin: Option<Arc<azcloud_kit::AzlinDrive>>,",
        "}",
        "pub fn again(slot: &Slot) {",
        "    let _ = azcloud_kit::open_azlin(&slot.entry, open, gate);",
        "}",
        "",
    ]
    .join("\n");
    let base = base_with(
        "planted",
        &[
            ("examples/azcloud-kit/src/lib.rs", KIT_LIB),
            ("examples/azcloud-kit/src/drive.rs", KIT_DRIVE),
            (
                "examples/azul-drive/src/lib.rs",
                &format!("mod jobs;\n{drive_lib}"),
            ),
            ("examples/azul-drive/src/jobs.rs", jobs.as_str()),
        ],
    );
    let wrong = check(&base, LISTED).expect_err("the job names the bytes and opens a drive");
    assert_eq!(wrong.len(), 2, "{wrong:?}");
    assert!(
        wrong
            .iter()
            .any(|w| w.starts_with("examples/azul-drive/src/jobs.rs:2") && w.contains(BYTE_STORE)),
        "{wrong:?}"
    );
    assert!(
        wrong
            .iter()
            .any(|w| w.starts_with("examples/azul-drive/src/jobs.rs:5") && w.contains(OPENED)),
        "{wrong:?}"
    );
    // Without the job, the listed places are counted and nothing else is found.
    std::fs::write(
        base.join("examples/azul-drive/src/jobs.rs"),
        "pub fn clean() {}\n",
    )
    .unwrap();
    assert_eq!(check(&base, LISTED), Ok(8));
    // A second opener in the listed file is a new site: the count says so.
    std::fs::write(
        base.join("examples/azul-drive/src/lib.rs"),
        format!("mod jobs;\n{drive_lib}{drive_lib}"),
    )
    .unwrap();
    let wrong = check(&base, LISTED).expect_err("one more opener than the list says");
    assert!(
        wrong
            .iter()
            .any(|w| w.contains("holds 2 x azlin-opened where its allowance says 1")),
        "{wrong:?}"
    );
    let _ = std::fs::remove_dir_all(base);
}

/// X-DELTA-STRUCT3-12. The gate read two spellings of the opener (`open_azlin(` and `azcloud_kit::open_azlin(`) and
/// allowed the whole kit by directory, uncounted. Every spelling that reaches the function is a finding now: the
/// path through the module, an import under another name, a function value; and a new function of the kit that
/// puts the bytes over an account - the hole that handed the bytes of an Azlin drive to every caller below the
/// gate - is a finding in a file nobody listed and a new site in `drive.rs`. Each plant fails on the old gate (it
/// found none of them) and none is in the clean tree.
#[test]
fn x_delta_struct3_12_every_spelling_of_the_opener_and_a_new_kit_function_that_makes_the_bytes_are_caught(
) {
    let drive_lib = "pub fn open(slot: &mut Slot) {\n    let _ = azcloud_kit::open_azlin(&slot.entry, open, gate);\n}\n";
    let spellings = [
        "pub fn path_through_the_module(slot: &Slot) {",
        "    let _ = azcloud_kit::drive::open_azlin(&slot.entry, open, gate);",
        "}",
        "use azcloud_kit::open_azlin as open_drive;",
        "pub fn value(slots: &[Slot]) {",
        "    let _ = slots.iter().map(azcloud_kit::open_azlin);",
        "}",
        "",
    ]
    .join("\n");
    let kit_function = "pub fn bytes(a: &Arc<AzlinAccount>) -> Arc<dyn Drive> {\n    Arc::new(AzlinDrive::over(a.clone()))\n}\n";
    let base = base_with(
        "spellings",
        &[
            (
                "examples/azcloud-kit/src/lib.rs",
                &format!("mod bytes;\n{KIT_LIB}"),
            ),
            ("examples/azcloud-kit/src/bytes.rs", kit_function),
            ("examples/azcloud-kit/src/drive.rs", KIT_DRIVE),
            (
                "examples/azul-drive/src/lib.rs",
                &format!("mod jobs;\n{drive_lib}"),
            ),
            ("examples/azul-drive/src/jobs.rs", spellings.as_str()),
        ],
    );
    let wrong = check(&base, LISTED).expect_err("the spellings and the kit function are found");
    let at = |file: &str, line: usize, rule: &str| {
        wrong
            .iter()
            .any(|w| w.starts_with(&format!("{file}:{line}:")) && w.contains(rule))
    };
    assert!(
        at("examples/azul-drive/src/jobs.rs", 2, OPENED),
        "the path through the module: {wrong:?}"
    );
    assert!(
        at("examples/azul-drive/src/jobs.rs", 4, OPENED),
        "an import under another name: {wrong:?}"
    );
    assert!(
        at("examples/azul-drive/src/jobs.rs", 6, OPENED),
        "a function value: {wrong:?}"
    );
    assert!(
        at("examples/azcloud-kit/src/bytes.rs", 2, MADE),
        "a new kit function that makes the bytes, in a kit file nobody listed: {wrong:?}"
    );
    assert!(
        at("examples/azcloud-kit/src/bytes.rs", 2, BYTE_STORE),
        "and names them: {wrong:?}"
    );
    // A third place that makes the bytes in `drive.rs` is a new site there.
    std::fs::write(
        base.join("examples/azcloud-kit/src/drive.rs"),
        format!("{KIT_DRIVE}pub fn third(account: A) {{\n    let bytes = AzlinDrive::over(account);\n}}\n"),
    )
    .unwrap();
    std::fs::write(
        base.join("examples/azcloud-kit/src/bytes.rs"),
        "pub fn clean() {}\n",
    )
    .unwrap();
    std::fs::write(
        base.join("examples/azul-drive/src/jobs.rs"),
        "pub fn clean() {}\n",
    )
    .unwrap();
    let wrong = check(&base, LISTED).expect_err("one more place than the list says");
    assert!(
        wrong
            .iter()
            .any(|w| w.contains("holds 3 x azlin-byte-store-made where its allowance says 2")),
        "{wrong:?}"
    );
    let _ = std::fs::remove_dir_all(base);
}

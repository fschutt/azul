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
//! - **`azlin-opened`**: the calls of `open_azlin`, the one function that makes an account and its
//!   bytes - and with them chooses the token server (`azlin_target`, KEYS-9) and puts the gate over the
//!   bytes. Each file that opens a drive is a counted line: a second opener is a second place the gate
//!   can be forgotten.
//!
//! Comments, strings and test items are not read; the counts are exact and only go down.

use std::path::{Path, PathBuf};

use azul_srcgate::{calls, idents, Allow, Budget, Gate, Orphans, Root, Rule};

const ROOTS: &[Root] = &[Root {
    dir: "examples",
    min_files: 500,
}];

const BYTE_STORE: &str = "azlin-byte-store-named";
const OPENED: &str = "azlin-opened";

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
            |t| {
                let mut lines = calls(t, "azcloud_kit::open_azlin");
                lines.extend(calls(t, "open_azlin"));
                lines.sort_unstable();
                lines
            },
            &[
                "let (account, drive) = azcloud_kit::open_azlin(&entry, open, gate)?;",
                "let opened = open_azlin(&entry, open, gate);",
            ],
            &[
                "pub fn open_azlin(entry: &DriveEntry) {}",
                "let x = Slot::open_azlin_drive(1);",
                "let t = tool.open_azlin_x(1);",
            ],
        ),
    ]
}

/// Where the rules may be broken, and why. TO SHRINK.
const ALLOWED: &[Allow] = &[
    Allow::file(
        "examples/azcloud-kit/src/",
        "the bytes of an Azlin drive are azcloud-kit's: `drive.rs` makes them, `open_azlin` hands them to \
         the gate, and the crate's own tests drive them",
    ),
    Allow::exactly(
        "examples/azul-drive/src/lib.rs",
        OPENED,
        1,
        "Slot::open: the one opener of an Azlin drive in the apps; its gate is `encryption::wrap`, where \
         AutoEncrypted is built (AzMail and the bridge open their own drives through `mail_drive::wrap`, \
         over a plain S3 bucket they build from a session)",
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
    Allow::file(
        "examples/azcloud-kit/src/",
        "the bytes are the kit's (the planted tree's)",
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

/// A job that names the bytes of an Azlin drive - the shape of the 17 signatures - is a finding with its
/// line; the kit may name them; a listed opener is counted, a second one in the same file is a new site,
/// and an opener in a file nobody listed is a finding.
#[test]
fn a_planted_job_that_names_the_bytes_and_a_second_opener_are_caught() {
    let kit = [
        "pub(crate) struct AzlinDrive {}",
        "pub fn open_azlin(entry: &DriveEntry) {}",
        "",
    ]
    .join("\n");
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
            (
                "examples/azcloud-kit/src/lib.rs",
                &format!("mod drive;\n{kit}"),
            ),
            ("examples/azcloud-kit/src/drive.rs", ""),
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
    // Without the job, the one listed opener is counted and the kit's own names are none.
    std::fs::write(
        base.join("examples/azul-drive/src/jobs.rs"),
        "pub fn clean() {}\n",
    )
    .unwrap();
    assert_eq!(check(&base, LISTED), Ok(2));
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

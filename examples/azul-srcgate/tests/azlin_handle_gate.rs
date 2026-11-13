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
//! - **`azlin-gate-made`**: every use of `AutoEncrypted::new` (a call, a function value, any path before it) and
//!   every import of `AutoEncrypted` under another name: the places that make the encryption's gate over a bucket.
//!   An Azlin drive's is made in ONE place, `azcloud_kit::open_azlin`, over the account's own bytes under the
//!   account's own drive id - the closure it took from its caller could wrap any bytes under any id and type-checked
//!   (X-DELTA-STRUCT4-1: the type is `GateParts` now, which has no field for either, and this rule counts the
//!   places that could still name them). AzMail's and the bridge's own gate over a bucket they build from a session
//!   is the other counted line (`mail_drive::wrap_auto`); a third place is a finding.
//!
//! - **`azlin-plain-bucket-opened`**: every use of `open_bucket` but its definition (`AzlinSession::open_bucket` of
//!   azul-mail-core): where AzMail's and the bridge's code opens the plain BYTES of an Azlin drive from a session. It is
//!   a `PlainBucket`, which is no drive: it is wrapped by `mail_drive::wrap` (the encryption's gate) or handed to
//!   `pending_code::encrypt_new_drive`, and nothing reads or writes a file through it - but what it was, `open_drive`,
//!   returned the `S3Drive` itself, and a new function that did `bucket.put(key, mail)` compiled and passed every gate
//!   (X-DELTA-STRUCT4-6). Each file that opens one is a counted line.
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
const GATE_MADE: &str = "azlin-gate-made";
const PLAIN_OPENED: &str = "azlin-plain-bucket-opened";

/// Every use of the name `name` that is not a definition (`fn name`): whatever spelling reaches the function - a bare
/// call, a path call, a method call, an import under another name and a function value.
fn uses_of(t: &[Located], name: &str) -> Vec<usize> {
    t.iter()
        .enumerate()
        .filter(|(at, x)| x.tok.is_ident(name) && !(*at > 0 && t[*at - 1].tok.is_ident("fn")))
        .map(|(_, x)| x.line)
        .collect()
}

/// Every use of `open_azlin` that is not its definition (`fn open_azlin`): whatever spelling reaches the
/// function - a bare call, a path call (`azcloud_kit::open_azlin(`, `azcloud_kit::drive::open_azlin(`), an
/// import under another name and a function value.
fn opened(t: &[Located]) -> Vec<usize> {
    uses_of(t, "open_azlin")
}

/// Every use of `open_bucket` that is not its definition: where the plain bytes of an Azlin drive are opened from a
/// session (a method call, a path call, a function value).
fn bucket_opened(t: &[Located]) -> Vec<usize> {
    uses_of(t, "open_bucket")
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

/// Every use of the path `AutoEncrypted::new` (a call or a function value, whatever the path before it) and every
/// import of `AutoEncrypted` under another name (`use .. AutoEncrypted as Gate`): where the encryption's gate is made.
fn gate_made(t: &[Located]) -> Vec<usize> {
    t.iter()
        .enumerate()
        .filter(|(at, x)| {
            let path_new = x.tok.is_ident("new")
                && *at >= 3
                && t[*at - 1].tok.is_punct(':')
                && t[*at - 2].tok.is_punct(':')
                && t[*at - 3].tok.is_ident("AutoEncrypted");
            let renamed = x.tok.is_ident("AutoEncrypted")
                && t.get(*at + 1).is_some_and(|n| n.tok.is_ident("as"));
            path_new || renamed
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
            PLAIN_OPENED,
            bucket_opened,
            &[
                "let bucket = session.open_bucket(None, Box::new(transport.clone()))?;",
                "let bucket = azmail_core::azlin::AzlinSession::open_bucket(&session, None, transport)?;",
                "let open = AzlinSession::open_bucket;",
                "let bucket = account.session().open_bucket(endpoint, transport).map_err(|e| e.to_string())?;",
            ],
            &[
                "pub fn open_bucket(&self, endpoint: Option<&str>) -> Result<PlainBucket, DriveError> {}",
                "let bucket = session.open_drive();",
                "let t = reopen_bucket(1);",
                "let open_buckets = 1;",
            ],
        ),
        Rule::new(
            GATE_MADE,
            gate_made,
            &[
                "let gate = AutoEncrypted::new(bytes, \"d_other\", keyring, None);",
                "let gate = azul_storage::AutoEncrypted::new(bytes, id, keyring, provider);",
                "let gate = azul_storage::encrypted::AutoEncrypted::new(bytes, id, keyring, provider);",
                "let make = AutoEncrypted::new;",
                "use azul_storage::AutoEncrypted as Gate;",
            ],
            &[
                "let gate: Arc<AutoEncrypted> = opened.gate;",
                "let t = AutoEncrypted::newer(1);",
                "let t = Other::new(1);",
                "let AutoEncrypteds = 1;",
                "fn new() -> AutoEncrypted {}",
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
        "examples/azcloud-kit/src/drive.rs",
        GATE_MADE,
        1,
        "`gate_over`, called by `open_azlin`: THE one place that makes the encryption's gate over an Azlin drive's \
         bytes - over the account's own bytes, under the account's own drive id, with the entry's own record \
         (X-DELTA-STRUCT4-1)",
    ),
    Allow::exactly(
        "examples/azul-drive/src/e2e/",
        GATE_MADE,
        2,
        "the E2E commands' encrypted remotes (registered only when AZ_E2E or AZ_DEBUG is set): each device of a scenario \
         wraps the fake bucket of its remote - no Azlin account of the kit, no session - in the gate under the remote's \
         own name, the way two computers would; counted, so a new one there is seen",
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
        "Slot::open_azlin_in: the one opener of an Azlin drive in the apps; it gives the opener the parts of the gate \
         (`encryption::gate_parts`: no bytes, no id) and the opener makes the gate (AzMail and the bridge open their \
         own drives through `mail_drive::wrap`, over a `PlainBucket` they build from a session: no drive, counted by \
         `azlin-plain-bucket-opened`)",
    ),
    Allow::exactly(
        "examples/azul-mail-core/src/mail_drive.rs",
        GATE_MADE,
        1,
        "`mail_drive::wrap_auto`: AzMail's and the bridge's own gate over a bucket they build from a session - an \
         encrypted-only gate, no Azlin account of the kit (the bytes they are handed are a `PlainBucket`, which is no \
         drive: X-DELTA-STRUCT4-6)",
    ),
    Allow::exactly(
        "examples/azul-mail-core/src/mail_drive.rs",
        PLAIN_OPENED,
        2,
        "`receive_drops` (the drive's key and drop files, read in the plain bucket and filed into the encrypted drive) and \
         `open` (the bucket goes under the encryption's gate in the same statement)",
    ),
    Allow::exactly(
        "examples/azul-mail/src/lib.rs",
        PLAIN_OPENED,
        1,
        "the job that makes a new drive: the plain bucket goes to `pending_code::encrypt_new_drive`, which writes the \
         recovery wrap and the key files into it (no mail, no file of the drive) - the one place of the app that holds one",
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
        GATE_MADE,
        1,
        "the planted kit's one gate",
    ),
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

/// [`LISTED`] and the one plain bucket a planted job may open.
const LISTED_PLAIN: &[Allow] = &[
    Allow::exactly(
        "examples/azcloud-kit/src/drive.rs",
        GATE_MADE,
        1,
        "the planted kit's one gate",
    ),
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
    Allow::exactly(
        "examples/azul-drive/src/jobs.rs",
        PLAIN_OPENED,
        1,
        "the planted job's one plain bucket",
    ),
];

fn check(base: &Path, allowed: &'static [Allow]) -> Result<usize, Vec<String>> {
    gate(SMALL, allowed).check(base).map(|hits| hits.len())
}

/// The planted kit: the bytes are defined and made in `drive.rs` (twice: the opener and the vault's handle),
/// `lib.rs` re-exports the opener.
const KIT_LIB: &str = "mod drive;\npub use drive::open_azlin;\n";
const KIT_DRIVE: &str = "pub(crate) struct AzlinDrive {}\nimpl AzlinDrive { fn over() {} }\npub fn open_azlin(entry: &DriveEntry) {\n    let bytes = AzlinDrive::over(account);\n    let gate = AutoEncrypted::new(bytes, &drive_id, keyring, provider);\n}\npub fn unlocked(account: A) {\n    let bytes = AzlinDrive::over(account);\n}\n";

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
    assert_eq!(check(&base, LISTED), Ok(9));
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

/// X-DELTA-STRUCT4-1. The encryption's gate over an Azlin drive was made by a closure of the app (`azlin_stack`), and the
/// type of the closure's result said "an `AutoEncrypted`": any other place that wraps bytes in one - under another
/// drive's id - passed every gate. The places that make the gate are counted lines now, whatever the spelling: a path
/// call, a path through the module, an import under another name, a function value; the kit's one is the listed line and
/// a second one in the kit's own file is a new site. RED on the old gate: it read none of them.
#[test]
fn x_delta_struct4_1_every_place_that_makes_the_encryptions_gate_is_a_counted_line_whatever_its_spelling(
) {
    let jobs = [
        "pub fn wrap(bytes: Arc<dyn Drive>, keyring: Arc<dyn KeyringStore>) -> Arc<AutoEncrypted> {",
        "    Arc::new(AutoEncrypted::new(bytes, \"d_other\", keyring, None))",
        "}",
        "pub fn path(bytes: Arc<dyn Drive>, keyring: Arc<dyn KeyringStore>) {",
        "    let _ = azul_storage::encrypted::AutoEncrypted::new(bytes, \"d_other\", keyring, None);",
        "}",
        "use azul_storage::AutoEncrypted as Plain;",
        "pub fn value() { let _make = AutoEncrypted::new; }",
        "",
    ]
    .join("\n");
    let drive_lib = "mod jobs;\npub fn open(slot: &mut Slot) {\n    let _ = azcloud_kit::open_azlin(&slot.entry, open, gate);\n}\n";
    let base = base_with(
        "gate-made",
        &[
            ("examples/azcloud-kit/src/lib.rs", KIT_LIB),
            ("examples/azcloud-kit/src/drive.rs", KIT_DRIVE),
            ("examples/azul-drive/src/lib.rs", drive_lib),
            ("examples/azul-drive/src/jobs.rs", jobs.as_str()),
        ],
    );
    let wrong = check(&base, LISTED).expect_err("four more places make the gate");
    let at = |line: usize| {
        wrong.iter().any(|w| {
            w.starts_with(&format!("examples/azul-drive/src/jobs.rs:{line}:"))
                && w.contains(GATE_MADE)
        })
    };
    assert!(at(2), "the call: {wrong:?}");
    assert!(at(5), "a path through the module: {wrong:?}");
    assert!(at(7), "an import under another name: {wrong:?}");
    assert!(at(8), "a function value: {wrong:?}");
    // The kit's one gate is the listed line: without the others the gate holds, with a second one in its file it does not.
    std::fs::write(
        base.join("examples/azul-drive/src/jobs.rs"),
        "pub fn clean() {}\n",
    )
    .unwrap();
    assert_eq!(check(&base, LISTED), Ok(9));
    std::fs::write(
        base.join("examples/azcloud-kit/src/drive.rs"),
        format!("{KIT_DRIVE}pub fn second(b: Arc<dyn Drive>) {{\n    let _ = AutoEncrypted::new(b, \"d_other\", keyring, None);\n}}\n"),
    )
    .unwrap();
    let wrong = check(&base, LISTED).expect_err("a second gate in the kit's file");
    assert!(
        wrong
            .iter()
            .any(|w| w.contains("holds 2 x azlin-gate-made where its allowance says 1")),
        "{wrong:?}"
    );
    let _ = std::fs::remove_dir_all(base);
}

/// X-DELTA-STRUCT4-6. AzMail and the bridge open the plain bytes of an Azlin drive from a session by `open_bucket` (it was
/// `open_drive`, which returned the drive itself): every use of the name is a counted line - a method call, a path call, a
/// function value - so a new function that opens one is a finding with its line, whatever the spelling; a listed file's
/// count is exact (one more is a new site), and a definition of a function of that name is no use. RED on the old gate: it
/// read none of them.
#[test]
fn x_delta_struct4_6_every_place_that_opens_the_plain_bytes_of_a_drive_is_a_counted_line() {
    let jobs = [
        "pub fn leak(session: &AzlinSession, transport: Box<dyn Transport>) {",
        "    let bucket = session.open_bucket(None, transport).unwrap();",
        "}",
        "pub fn path(session: &AzlinSession, transport: Box<dyn Transport>) {",
        "    let _ = AzlinSession::open_bucket(session, None, transport);",
        "}",
        "pub fn value() { let _open = AzlinSession::open_bucket; }",
        "impl Other { pub fn open_bucket(&self) {} }",
        "",
    ]
    .join("\n");
    let drive_lib = "mod jobs;\npub fn open(slot: &mut Slot) {\n    let _ = azcloud_kit::open_azlin(&slot.entry, open, gate);\n}\n";
    let base = base_with(
        "plain-bucket",
        &[
            ("examples/azcloud-kit/src/lib.rs", KIT_LIB),
            ("examples/azcloud-kit/src/drive.rs", KIT_DRIVE),
            ("examples/azul-drive/src/lib.rs", drive_lib),
            ("examples/azul-drive/src/jobs.rs", jobs.as_str()),
        ],
    );
    // None of them is listed: each is a finding with its line, whatever the spelling.
    let wrong =
        check(&base, LISTED).expect_err("three places open the plain bytes and none is listed");
    let at = |line: usize| {
        wrong.iter().any(|w| {
            w.starts_with(&format!("examples/azul-drive/src/jobs.rs:{line}:"))
                && w.contains(PLAIN_OPENED)
        })
    };
    assert!(at(2), "the method call: {wrong:?}");
    assert!(at(5), "the path call: {wrong:?}");
    assert!(at(7), "the function value: {wrong:?}");
    assert!(!at(8), "a definition is no use: {wrong:?}");
    // The count of a listed file is exact: three where the list says one.
    let wrong = check(&base, LISTED_PLAIN).expect_err("three where the list says one");
    assert!(
        wrong
            .iter()
            .any(|w| w.contains("holds 3 x azlin-plain-bucket-opened where its allowance says 1")),
        "the count is exact: {wrong:?}"
    );
    // One call in the listed file holds; a second one is a new site.
    std::fs::write(
        base.join("examples/azul-drive/src/jobs.rs"),
        "pub fn one(s: &AzlinSession, t: Box<dyn Transport>) {\n    let _ = s.open_bucket(None, t);\n}\n",
    )
    .unwrap();
    assert_eq!(check(&base, LISTED_PLAIN), Ok(10));
    std::fs::write(
        base.join("examples/azul-drive/src/jobs.rs"),
        "pub fn two(s: &AzlinSession, t: Box<dyn Transport>, u: Box<dyn Transport>) {\n    let _ = s.open_bucket(None, t);\n    let _ = s.open_bucket(None, u);\n}\n",
    )
    .unwrap();
    let wrong = check(&base, LISTED_PLAIN).expect_err("a second plain bucket in the listed file");
    assert!(
        wrong
            .iter()
            .any(|w| w.contains("holds 2 x azlin-plain-bucket-opened where its allowance says 1")),
        "{wrong:?}"
    );
    // The same call in a file nobody listed is a finding.
    std::fs::write(
        base.join("examples/azul-drive/src/jobs.rs"),
        "pub fn clean() {}\n",
    )
    .unwrap();
    std::fs::write(
        base.join("examples/azul-drive/src/other.rs"),
        "pub fn leak(s: &AzlinSession, t: Box<dyn Transport>) {\n    let _ = s.open_bucket(None, t);\n}\n",
    )
    .unwrap();
    let wrong = check(&base, LISTED_PLAIN).expect_err("a file nobody listed opens one");
    assert!(
        wrong
            .iter()
            .any(|w| w.starts_with("examples/azul-drive/src/other.rs:2:")
                && w.contains(PLAIN_OPENED)),
        "{wrong:?}"
    );
    let _ = std::fs::remove_dir_all(base);
}

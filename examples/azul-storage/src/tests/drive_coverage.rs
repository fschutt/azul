//! X-AR5a-7: `LocalDrive` overrides most of the `Drive` methods; the others fall to the
//! trait's defaults. Which ones is a decision, written down here with its reason: a new `Drive`
//! method fails this test until it is either overridden by `LocalDrive` or listed with the reason
//! the default is right for a folder on disk, and `forward_drive!` must forward it.
//!
//! X-DELTA-AZDRIVE-FILES-2 (and -35): the same holds for EVERY wrapper of a drive. The app never
//! hands out a bare `LocalDrive`: Home and every added folder are a `RetryingDrive` over one,
//! and that wrapper forwarded neither `site()` nor `remove_empty_folder()`, so the guards that
//! rest on them never fired and every folder Move out of a local slot ended "failed". A wrapper
//! defines every method of the trait or says, in a `// inherits: a, b - why` comment inside its
//! impl, which ones it leaves to the default and why that is right; a shipped `impl Drive for`
//! that this file does not know is a failure until it is classified.
//!
//! ARCH29 C6e: a wrapper made with `delegate_drive!` forwards every method of `drive_methods!`
//! (THE list of the 27) unless it writes the method itself, so a new method of the trait is
//! forwarded without touching the wrapper - provided the list says the same as the trait, which
//! the first test below keeps true. `AzlinDrive`, the bytes of an Azlin drive (ARCH29 C6d), `Stops`
//! (the copy of a locked drive reads through it; X-DELTA-STRUCT-DRIVE-3: it wrote seven methods and
//! left twenty to the trait's defaults) and `Stoppable` (a sync download's drive over a reference;
//! X-DELTA-STRUCT3-6: the same seven methods, and the pinned read was lost) are made with it too: a request
//! goes through their `send` (the credentials renewed; the stop flag asked), and a method they do not write is
//! forwarded - or, where there is no drive below, refused - never answered by the trait's default.
//!
//! X-DELTA-STRUCT3-8: the tests above read NAMES. The last section of this file puts a probe (a drive that only
//! listens) below each wrapper that decides for itself what a call does and sends a pinned read through it; the
//! wrappers' own crates hold the same test over their own probes ([`BODY_TESTS`] names each and a test below
//! keeps the list whole). The wrappers that must write every method on
//! purpose (the encryption wrappers, `ScopedDrive`, the bridge's account drive) stay in `WRAPPERS`
//! with their per-method census: forwarding as it is would be the bug for a method nobody looked at.

use azul_srcgate::{
    lex::{item_end, test_items},
    lex_spans, Spanned, Tok,
};

/// The methods `LocalDrive` leaves to the trait's default, each with why that is right.
const LOCAL_DEFAULTS: &[(&str, &str)] = &[
    ("get_range_if", "refused as unsupported: a folder on disk is copied by its path (a file copy), never by ranges of one request each, and keeps no versions to ask (see conditional_writes)"),
    ("resume_folder", "a folder on disk has no multipart state: None"),
    ("put_file", "streams the file through LocalDrive::put_from"),
    ("put_file_checked", "opens the checked file once, streams it through LocalDrive::put_from"),
    ("conditional_writes", "false: a folder on disk keeps no versions; a copy into it is guarded by the rename that puts the file in place (durable::Expect)"),
    ("put_file_if", "refused at once, unread (a folder asks no condition, see conditional_writes): the engine guards a file it copies in with Expect"),
    ("put_many", "one LocalDrive::put after the other: a folder packs nothing"),
    ("delete_many", "one LocalDrive::delete after the other, a version checked by LocalDrive::head's tag"),
    ("put_from_many", "one LocalDrive::put_from after the other"),
    ("delete_for", "the reason is checked against the key, then LocalDrive::delete (ARCH30-1: the trait's composition over the drive's own delete)"),
    ("delete_many_for", "the reason is checked against every key of the batch, then LocalDrive::delete_many's default (one LocalDrive::delete after the other)"),
    ("delete_folder_for", "the reason is checked against the folder, then LocalDrive::delete_folder"),
];

/// The block that starts at `header` in `source`, and how far in its first line stands: from the header to the closing brace
/// that stands as far in as that line (column 0 for an impl at the start of a line, the module's indent for one inside a
/// module: a block is read by its own indentation, X-DELTA-DRIVEE-7).
fn block_of<'a>(source: &'a str, header: &str) -> (&'a str, usize) {
    let start = source
        .find(header)
        .unwrap_or_else(|| panic!("{header:?} not found"));
    let line_start = source[..start].rfind('\n').map_or(0, |at| at + 1);
    let indent = source[line_start..]
        .bytes()
        .take_while(|byte| *byte == b' ')
        .count();
    let body = &source[start..];
    let closing = format!("\n{}}}\n", " ".repeat(indent));
    let end = body.find(&closing).expect("end of the block");
    (&body[..end], indent)
}

fn methods_of(source: &str, header: &str) -> Vec<String> {
    let (block, indent) = block_of(source, header);
    let (one, two) = (
        format!("{}fn ", " ".repeat(indent + 4)),
        format!("{}fn ", " ".repeat(indent + 12)),
    );
    block
        .lines()
        .filter_map(|line| {
            let rest = line
                .strip_prefix(one.as_str())
                .or_else(|| line.strip_prefix(two.as_str()))?;
            Some(rest.split(['(', '<']).next()?.trim().to_string())
        })
        .collect()
}

#[test]
fn x_ar5a_7_every_drive_method_is_overridden_by_the_local_drive_or_defaulted_for_a_reason() {
    let lib = include_str!("../lib.rs");
    let local = include_str!("../local.rs");
    let all = methods_of(lib, "pub trait Drive: Send + Sync {");
    assert!(all.len() >= 22, "the trait was parsed: {all:?}");
    let own = methods_of(local, "impl Drive for LocalDrive {");
    for method in &all {
        let overridden = own.contains(method);
        let defaulted = LOCAL_DEFAULTS.iter().any(|(m, _)| m == method);
        assert!(
            overridden != defaulted,
            "Drive::{method}: LocalDrive must either override it or list it in LOCAL_DEFAULTS \
             with the reason the default is right (overridden: {overridden}, listed: {defaulted})"
        );
    }
    for (method, reason) in LOCAL_DEFAULTS {
        assert!(all.iter().any(|m| m == method), "{method} is no Drive method");
        assert!(!reason.is_empty());
    }
}

#[test]
fn x_ar5a_7_the_pointer_impls_forward_every_drive_method() {
    let lib = include_str!("../lib.rs");
    let all = methods_of(lib, "pub trait Drive: Send + Sync {");
    let start = lib.find("macro_rules! forward_drive").expect("the macro");
    let forwarded: Vec<String> = lib[start..]
        .lines()
        .skip(1)
        .take_while(|l| !l.starts_with("forward_drive!"))
        .filter_map(|line| {
            let rest = line.strip_prefix("            fn ")?;
            Some(rest.split(['(', '<']).next()?.trim().to_string())
        })
        .collect();
    for method in &all {
        assert!(
            forwarded.contains(method),
            "Drive::{method} is not forwarded by a Box, an Arc or a reference to a drive"
        );
    }
}

// ==== THE list of the methods, and the wrappers made from it (ARCH29 C6e) ====

/// The methods of the trait `Drive` in lib.rs, each with whether it has no default body.
fn trait_methods(lib: &str) -> Vec<(String, bool)> {
    let header = "pub trait Drive: Send + Sync {";
    let start = lib
        .find(header)
        .unwrap_or_else(|| panic!("{header:?} not found"));
    let body = &lib[start..];
    let end = body.find("\n}\n").expect("end of the trait");
    let lines: Vec<&str> = body[..end].lines().collect();
    let mut out = Vec::new();
    for (at, line) in lines.iter().enumerate() {
        let Some(rest) = line.strip_prefix("    fn ") else {
            continue;
        };
        let name = rest
            .split(['(', '<'])
            .next()
            .unwrap_or(rest)
            .trim()
            .to_string();
        // The signature ends at the first line that ends with `;` (no default) or `{` (a body).
        let required = lines[at..]
            .iter()
            .map(|l| l.trim_end())
            .find_map(|l| {
                if l.ends_with(';') {
                    Some(true)
                } else if l.ends_with('{') {
                    Some(false)
                } else {
                    None
                }
            })
            .unwrap_or_else(|| panic!("the signature of Drive::{name} ends nowhere"));
        out.push((name, required));
    }
    out
}

/// One entry of `drive_methods!`: `(kind, required|provided) fn name(..) -> ..;`.
struct Entry {
    kind: String,
    required: bool,
    name: String,
    /// The entry after `fn `: its name, arguments and return type.
    signature: String,
}

/// The entries of `drive_methods!` in lib.rs, one per line that starts with `(kind, flag) fn `.
fn list_entries(lib: &str) -> Vec<Entry> {
    let header = "macro_rules! drive_methods {";
    let start = lib
        .find(header)
        .unwrap_or_else(|| panic!("{header:?} not found"));
    let body = &lib[start..];
    let end = body.find("\n}\n").expect("end of the list");
    body[..end]
        .lines()
        .filter_map(|line| {
            let rest = line.trim_start().strip_prefix('(')?;
            let (flags, after) = rest.split_once(") fn ")?;
            let (kind, flag) = flags.split_once(", ")?;
            let required = match flag {
                "required" => true,
                "provided" => false,
                other => panic!("`{other}` is neither `required` nor `provided`: {line}"),
            };
            Some(Entry {
                kind: kind.to_string(),
                required,
                name: after.split('(').next()?.trim().to_string(),
                signature: after.to_string(),
            })
        })
        .collect()
}

/// ARCH29 C6e. THE list a macro-made wrapper forwards from says what the trait says: the same
/// methods, the same ones required. A method added to the trait only is not forwarded by a wrapper
/// made with `delegate_drive!` - the trait's default would answer for it, which is how
/// `RetryingDrive` lost `site()` (X-DELTA-AZDRIVE-FILES-2); a method added to the list only does not
/// compile there. Adds nothing to either: it fails when they differ.
#[test]
fn c6e_the_list_of_drive_methods_is_the_traits_list_name_for_name_and_required_for_required() {
    let lib = include_str!("../lib.rs");
    let in_trait = trait_methods(lib);
    let in_list = list_entries(lib);
    assert!(in_trait.len() >= 27, "the trait was parsed: {in_trait:?}");
    let not_in_list: Vec<&str> = in_trait
        .iter()
        .filter(|(name, _)| !in_list.iter().any(|e| &e.name == name))
        .map(|(name, _)| name.as_str())
        .collect();
    let not_in_trait: Vec<&str> = in_list
        .iter()
        .filter(|e| !in_trait.iter().any(|(name, _)| name == &e.name))
        .map(|e| e.name.as_str())
        .collect();
    assert!(
        not_in_list.is_empty() && not_in_trait.is_empty(),
        "drive_methods! (lib.rs) and `pub trait Drive` differ - in the trait only: {not_in_list:?}; \
         in the list only: {not_in_trait:?}. A wrapper made with delegate_drive! forwards what the \
         list names; a method missing there is answered by the trait's default"
    );
    assert_eq!(
        in_list.len(),
        in_trait.len(),
        "a method is listed twice in drive_methods!"
    );
    for entry in &in_list {
        let in_the_trait = in_trait.iter().find(|(name, _)| name == &entry.name);
        assert_eq!(
            in_the_trait.map(|(_, required)| *required),
            Some(entry.required),
            "Drive::{} is {} in the trait and {} in drive_methods!",
            entry.name,
            if entry.required {
                "provided"
            } else {
                "required"
            },
            if entry.required {
                "required"
            } else {
                "provided"
            },
        );
    }
}

/// ARCH29 C6e. What a wrapper retries or renews is the `send` entries: they must be requests that
/// can be sent again as they are. A call that carries a body that is read once (`dyn Read`), a stop
/// handle for a file's upload (`dyn Ctl`) or a batch (`Batch*`) is `pass`, so a wrapper that retries
/// by default never sends a half-read body twice, and a method that answers no request (`site`,
/// `local_path`) is never wrapped.
#[test]
fn c6e_a_send_method_is_a_request_that_can_be_sent_again_and_every_other_call_is_pass() {
    let in_list = list_entries(include_str!("../lib.rs"));
    assert!(in_list.len() >= 27, "the list was parsed");
    for entry in &in_list {
        assert!(
            ["send", "pass", "compose"].contains(&entry.kind.as_str()),
            "Drive::{}: the kind `{}` is neither `send`, `pass` nor `compose`",
            entry.name,
            entry.kind
        );
        let answers_a_request = entry.signature.contains("-> ::core::result::Result<");
        let carries_what_cannot_be_sent_twice = entry.signature.contains("::std::io::Read")
            || entry.signature.contains("$crate::Ctl")
            || entry.signature.contains("$crate::Batch");
        if entry.kind == "send" {
            assert!(
                answers_a_request && !carries_what_cannot_be_sent_twice,
                "Drive::{} is `send` and is no request that can be sent again (a body, a stop \
                 handle, a batch or no Result): {}",
                entry.name,
                entry.signature
            );
        }
    }
    // The two methods RetryingDrive forgot (FILES-2) are forwarded in different ways on purpose:
    // the place is a question (`pass`), the emptied folder a request that is safe to send again.
    let kind = |name: &str| {
        in_list
            .iter()
            .find(|e| e.name == name)
            .map(|e| e.kind.as_str())
    };
    assert_eq!(kind("site"), Some("pass"));
    assert_eq!(kind("remove_empty_folder"), Some("send"));
    assert_eq!(
        kind("put_from"),
        Some("pass"),
        "a body read once is never sent twice"
    );
    assert_eq!(kind("put_file"), Some("pass"));
    assert_eq!(kind("put_many"), Some("pass"));
}

/// ARCH30-1. A call that takes a reason to delete (`erase::Why`) is `compose`, and only such a call
/// is: a reason is spent by the call it is given to, so it cannot be `send` (a wrapper that retries
/// would send it twice) and a `pass` would forward it to the drive below - past the wrapper's own
/// retry, cache, credentials, stop flag, grant and key rewrite. The default of the trait checks it
/// against the key the caller names and calls the wrapper's OWN `delete`, `delete_many` or
/// `delete_folder`; the macro leaves such an entry out of `DriveDelegate` and of the `impl Drive` it
/// writes. A `compose` entry is provided (a leaf that must answer for itself is the later step) and
/// names the call it composes over (`delete_for` over `delete`).
#[test]
fn arch30_1_a_call_that_takes_a_reason_is_compose_and_nothing_else_is() {
    let in_list = list_entries(include_str!("../lib.rs"));
    let composed: Vec<&str> = in_list
        .iter()
        .filter(|e| e.kind == "compose")
        .map(|e| e.name.as_str())
        .collect();
    assert_eq!(
        composed,
        ["delete_for", "delete_many_for", "delete_folder_for"],
        "the reasons to delete, in the order of the list"
    );
    for entry in &in_list {
        let takes_a_reason = entry.signature.contains("$crate::erase::Why");
        assert_eq!(
            takes_a_reason,
            entry.kind == "compose",
            "Drive::{}: a call that takes a reason is `compose` and every `compose` call takes one \
             (kind `{}`): {}",
            entry.name,
            entry.kind,
            entry.signature
        );
        if entry.kind == "compose" {
            assert!(
                !entry.required,
                "Drive::{} is `compose` and required: the default is the composition",
                entry.name
            );
            let over = entry.name.trim_end_matches("_for");
            assert!(
                in_list.iter().any(|e| e.name == over),
                "Drive::{} composes over Drive::{over}, which the list does not name",
                entry.name
            );
        }
    }
}

/// A wrapper made with the macro, over a folder, that counts the requests it sends: it writes
/// `below` and `send` and nothing else.
struct Counting {
    folder: crate::LocalDrive,
    sent: std::sync::atomic::AtomicUsize,
}

impl crate::DriveDelegate for Counting {
    fn below(&self) -> &dyn crate::Drive {
        &self.folder
    }

    fn send<T>(
        &self,
        request: impl Fn(&dyn crate::Drive) -> Result<T, crate::DriveError>,
    ) -> Result<T, crate::DriveError> {
        self.sent.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        request(&self.folder)
    }
}

crate::delegate_drive!(Counting);

/// ARCH30-1, through the entry the production wrappers use (`delegate_drive!`, over a real folder):
/// the three calls that take a reason are composed over the wrapper's OWN delete path, so the
/// wrapper's `send` is asked once for the delete itself and a reason that is for another key is
/// refused BEFORE it is asked - nothing reaches the drive below (RED against a forwarded `pass`: the
/// wrapper would never hear of the delete, and `delete_for` would be answered by the folder below).
#[test]
fn arch30_1_a_macro_made_wrapper_composes_the_reasons_over_its_own_delete_path() {
    use crate::{
        erase::{Chose, Target, Why},
        BatchDelete, Drive, DriveError,
    };
    use std::sync::atomic::Ordering::SeqCst;
    let dir = crate::testing::TempDir::new("arch30-1-counting");
    let drive = Counting {
        folder: crate::LocalDrive::new(dir.path().to_path_buf()),
        sent: std::sync::atomic::AtomicUsize::new(0),
    };
    let sent = || drive.sent.load(SeqCst);
    for key in ["a/one.txt", "a/two.txt", "b/three.txt", "keep.txt"] {
        drive.put(key, b"x").unwrap();
    }
    let puts = sent();
    // One reason for one key: the delete goes through the wrapper's `send`, once.
    let key_one = Chose::job("delete one.txt", Target::Key(String::from("a/one.txt")));
    drive
        .delete_for("a/one.txt", Why::UserChose(&key_one))
        .unwrap();
    assert_eq!(sent(), puts + 1, "the wrapper's own delete path asked once");
    assert!(drive.head("a/one.txt").is_err());
    // A reason for another key is refused before the wrapper is asked, and nothing is deleted.
    let refused = drive.delete_for("keep.txt", Why::UserChose(&key_one));
    assert!(
        matches!(refused, Err(DriveError::Denied { .. })),
        "{refused:?}"
    );
    assert_eq!(sent(), puts + 2, "only the head above asked the wrapper");
    assert!(drive.head("keep.txt").is_ok());
    // A batch is refused whole when one key is not the reason's; else one send for the batch's items.
    let two_files = Chose::job(
        "delete two files",
        Target::Keys(vec![String::from("a/two.txt"), String::from("b/three.txt")]),
    );
    let stray = [BatchDelete::new("a/two.txt"), BatchDelete::new("keep.txt")];
    let answers = drive.delete_many_for(&stray, Why::UserChose(&two_files));
    assert!(answers
        .iter()
        .all(|a| matches!(a, Err(DriveError::Denied { .. }))));
    assert!(
        drive.head("a/two.txt").is_ok(),
        "nothing of a refused batch is deleted"
    );
    let both = [
        BatchDelete::new("a/two.txt"),
        BatchDelete::new("b/three.txt"),
    ];
    let answers = drive.delete_many_for(&both, Why::UserChose(&two_files));
    assert!(answers.iter().all(Result::is_ok), "{answers:?}");
    // A folder is removed only for a job that named a folder.
    let folder = Chose::job("delete the folder", Target::Folder(String::from("b/")));
    assert!(matches!(
        drive.delete_folder_for("b/", Why::UserChose(&two_files)),
        Err(DriveError::Denied { .. })
    ));
    drive
        .delete_folder_for("b/", Why::UserChose(&folder))
        .unwrap();
}

/// ARCH29 C6e, through the entry the production wrappers use (`delegate_drive!`, over a real
/// folder): a request goes through the wrapper's `send` once, and everything else - the place, the
/// path, a body that is read once - goes straight to the drive below, answered by the drive below
/// and not by the trait's defaults (`site()` is the folder's own place, the emptied folder is
/// removed with `rmdir`, as X-DELTA-AZDRIVE-FILES-2 needs).
#[test]
fn c6e_a_macro_made_wrapper_sends_its_requests_through_send_and_passes_the_rest_straight_on() {
    use crate::{Drive, ListRequest};
    use std::sync::atomic::Ordering::SeqCst;
    let dir = crate::testing::TempDir::new("c6e-counting");
    let drive = Counting {
        folder: crate::LocalDrive::new(dir.path().to_path_buf()),
        sent: std::sync::atomic::AtomicUsize::new(0),
    };
    let sent = || drive.sent.load(SeqCst);
    drive.put("a/b.txt", b"hi").unwrap();
    assert_eq!(drive.get("a/b.txt").unwrap(), b"hi");
    drive.head("a/b.txt").unwrap();
    drive.list(&ListRequest::folder("a/")).unwrap();
    assert_eq!(
        sent(),
        4,
        "put, get, head and list are requests: one send each"
    );
    // A question about the drive, and a body that is read once, go straight on.
    assert_eq!(drive.site(), drive.folder.site());
    assert!(matches!(drive.site(), crate::xfer::Site::Disk { .. }));
    assert_eq!(
        drive.local_path("a/b.txt"),
        drive.folder.local_path("a/b.txt")
    );
    assert_eq!(
        drive.conditional_writes(),
        drive.folder.conditional_writes()
    );
    let mut body: &[u8] = b"streamed";
    assert_eq!(drive.put_from("c.txt", &mut body), Ok(8));
    assert_eq!(drive.get("c.txt").unwrap(), b"streamed");
    assert_eq!(
        sent(),
        5,
        "the question and the body did not go through send; the get did"
    );
    // The emptied folder is the folder's to remove (a request: it is sent), not the default's.
    drive.create_folder("gone/").unwrap();
    assert_eq!(drive.remove_empty_folder("gone/"), Ok(true));
    assert!(!dir.path().join("gone").exists());
    assert_eq!(sent(), 7);
}

// ==== Every wrapper of a drive (X-DELTA-AZDRIVE-FILES-2) ====

/// The drives that sit on top of another drive and are made with `delegate_drive!` (ARCH29 C6e):
/// `(type, file below examples/)`. The macro forwards every method of THE list (`drive_methods!`,
/// kept equal to the trait by the first test of this file) through the wrapper's `DriveDelegate`,
/// which writes only what differs: nothing here needs an `// inherits:` comment.
const DELEGATING: &[(&str, &str)] = &[
    ("RetryingDrive", "azcloud-kit/src/sync/retry.rs"),
    ("ObjectCache", "azcloud-kit/src/sync/objects.rs"),
    ("AzlinDrive", "azcloud-kit/src/drive.rs"),
    ("Stops", "azul-drive/src/archive.rs"),
    // A download's drive, over a `&dyn Drive` (a wrapper with a lifetime: `delegate_drive!(Stoppable<'_>)`).
    ("Stoppable", "azcloud-kit/src/sync/drive_store.rs"),
];

/// The drives that sit on top of another drive and write every method themselves, on purpose (a
/// call forwarded as it is would be the bug: the encryption's `Op`, the grant's key rewrite, the
/// credentials): `(type, file below examples/, the impl's header)`. Each defines every method of the
/// trait or has an `// inherits:` comment for it.
const WRAPPERS: &[(&str, &str, &str)] = &[
    (
        "AutoEncrypted",
        "azul-storage/src/encrypted.rs",
        "impl Drive for AutoEncrypted {",
    ),
    (
        "EncryptedDrive",
        "azul-storage/src/encrypted.rs",
        "impl<D: Drive> Drive for EncryptedDrive<D> {",
    ),
    (
        "ScopedDrive",
        "azul-storage/src/scoped.rs",
        "impl<D: Drive> Drive for ScopedDrive<D> {",
    ),
    (
        "AccountDrive",
        "azul-bridge/src/account.rs",
        "impl Drive for AccountDrive {",
    ),
];

/// The shipped drives that are not wrappers, each with why nothing is required of them here: a
/// place of their own, or an adapter of the few calls one caller makes.
const OTHERS: &[(&str, &str)] = &[
    (
        "LocalDrive",
        "a folder on disk: checked above against LOCAL_DEFAULTS",
    ),
    (
        "S3Drive",
        "a bucket: its place is `Site::bucket`, the rest falls to defaults built on its own calls",
    ),
    (
        "OpendalDrive",
        "a data source nobody can look into: only provably the same place as itself",
    ),
    (
        "DatabaseDrive",
        "a database: only provably the same place as itself",
    ),
    (
        "MemoryDrive",
        "a drive in memory (the bridge's tests and demos): a place of its own",
    ),
    (
        "MemoryBucket",
        "a bucket in memory (the drive index's tests and tools): a place of its own",
    ),
    (
        "Probe",
        "the leaf of the body census (`azul_storage::testing`, feature `testing`): it only listens - the calls \
         that reach it, with their keys and conditions - and answers what a drive nobody wrote to answers: a \
         place of its own, and no drive below it",
    ),
    (
        "NoDriveBelow",
        "the drive below an Azlin drive's bytes (azcloud-kit's `AzlinDrive`): none - the bucket changes \
         with the credentials - so a method its store did not write is refused, by the six required \
         methods answering `Unsupported` (ARCH29 C6d)",
    ),
    (
        "NoEncryption",
        "the mail's stand-in in a build without the encryption: an Azlin drive is encrypted and \
         this build can open none, so every call answers a refusal - nothing is forwarded and no \
         place is claimed (X-DELTA-AZDRIVE-KEYS-23)",
    ),
];

/// The rows of the `// inherits: a, b - reason` comments inside `block`: the methods, the reason.
/// A comment may go on over the next `//` lines; the next `// inherits:` starts another row.
fn inherited(block: &str) -> Vec<(Vec<String>, String)> {
    let mut rows: Vec<String> = Vec::new();
    let mut open = false;
    for line in block.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("// inherits:") {
            rows.push(rest.trim().to_string());
            open = true;
        } else if open && line.starts_with("//") {
            if let Some(last) = rows.last_mut() {
                last.push(' ');
                last.push_str(line.trim_start_matches('/').trim());
            }
        } else {
            open = false;
        }
    }
    rows.into_iter()
        .map(|row| {
            let (names, why) = row.split_once(" - ").unwrap_or((row.as_str(), ""));
            (
                names.split(',').map(|n| n.trim().to_string()).collect(),
                why.trim().to_string(),
            )
        })
        .collect()
}

/// The text of the impl that starts with `header` in `source`, to its closing brace.
fn impl_block<'a>(source: &'a str, header: &str) -> &'a str {
    block_of(source, header).0
}

fn examples_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the examples folder")
        .to_path_buf()
}

#[test]
fn x_delta_azdrive_files_2_a_wrapper_defines_every_drive_method_or_says_why_it_inherits_it() {
    let lib = include_str!("../lib.rs");
    let all = methods_of(lib, "pub trait Drive: Send + Sync {");
    assert!(all.len() >= 25, "the trait was parsed: {all:?}");
    for (name, file, header) in WRAPPERS {
        let path = examples_dir().join(file);
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{name}: {} cannot be read ({e})", path.display()));
        let block = impl_block(&source, header);
        let defined = methods_of(&source, header);
        let rows = inherited(block);
        for (methods, why) in &rows {
            assert!(
                !why.is_empty(),
                "{name}: an `// inherits:` without a reason"
            );
            for method in methods {
                assert!(
                    all.contains(method),
                    "{name}: `// inherits: {method}` names no Drive method"
                );
                assert!(
                    !defined.contains(method),
                    "{name}: {method} is defined AND listed as inherited (the comment is stale)"
                );
            }
        }
        for method in &all {
            let inherits = rows.iter().any(|(methods, _)| methods.contains(method));
            assert!(
                defined.contains(method) || inherits,
                "{name} ({file}) neither defines Drive::{method} nor says in an `// inherits:` \
                 comment why the default is right for it: a wrapper that does not forward a \
                 call answers it for itself - `site()` as a place of its own, \
                 `remove_empty_folder()` as a listing and a delete of markers"
            );
        }
    }
}

/// ARCH29 C6e. A wrapper made with `delegate_drive!` calls the macro, has no `impl Drive` of its own
/// (that would not compile twice; the text check says why), and writes in its `DriveDelegate` only
/// `below`, `send` and methods of THE list: what it does not write is forwarded, whatever the trait
/// gets next.
#[test]
fn c6e_a_delegating_wrapper_calls_the_macro_and_writes_only_below_send_and_methods_of_the_list() {
    let lib = include_str!("../lib.rs");
    let listed: Vec<String> = list_entries(lib).into_iter().map(|e| e.name).collect();
    assert!(listed.len() >= 27, "the list was parsed: {listed:?}");
    for (name, file) in DELEGATING {
        let path = examples_dir().join(file);
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{name}: {} cannot be read ({e})", path.display()));
        // A wrapper over a reference has a lifetime, and the macro takes the type as it is written.
        let spelled = if source.contains(&format!("azul_storage::delegate_drive!({name}<'_>);")) {
            format!("{name}<'_>")
        } else {
            (*name).to_string()
        };
        assert!(
            source.contains(&format!("azul_storage::delegate_drive!({spelled});")),
            "{name} ({file}) is listed in DELEGATING and does not call delegate_drive!({name}): its \
             impl Drive is not made"
        );
        assert!(
            !source.contains(&format!("impl Drive for {spelled} {{")),
            "{name} ({file}) is made with delegate_drive! and writes an impl Drive of its own"
        );
        let header = format!("impl azul_storage::DriveDelegate for {spelled} {{");
        let written = methods_of(&source, &header);
        assert!(
            written.iter().any(|m| m == "below"),
            "{name} ({file}) does not say which drive is below"
        );
        for method in &written {
            assert!(
                method == "below" || method == "send" || listed.contains(method),
                "{name} ({file}): `{method}` is no method of DriveDelegate (drive_methods! lists \
                 {listed:?})"
            );
        }
    }
}

/// The shipped impls of `Drive` (`impl Drive for X`, `delegate_drive!(X)`) that `known` does not name, with their files.
fn unclassified<'a>(
    found: &'a [(String, std::path::PathBuf)],
    known: &[&str],
) -> Vec<&'a (String, std::path::PathBuf)> {
    found
        .iter()
        .filter(|(name, _)| !known.contains(&name.as_str()))
        .collect()
}

/// Every `impl Drive for X` of the shipped code - at the start of a line or inside a module, in a file that is no test's, and
/// no item a test build alone compiles - is a wrapper of the table or one of the others: a new drive is a decision, not a
/// surprise. It read the impls at the start of a line, so `mod mirror { impl<D: Drive> Drive for Mirror<D> { .. } }` was
/// classified nowhere and read by no census test (X-DELTA-DRIVEE-7; the finder is `drive_impls_in`, which has its planted cases).
#[test]
fn x_delta_azdrive_files_2_every_shipped_impl_of_drive_is_classified() {
    let found = shipped_impls();
    assert!(found.len() >= 10, "the impls were found: {found:?}");
    let known: Vec<&str> = WRAPPERS
        .iter()
        .map(|(name, _, _)| *name)
        .chain(DELEGATING.iter().map(|(name, _)| *name))
        .chain(OTHERS.iter().map(|(name, _)| *name))
        .collect();
    if let Some((name, path)) = unclassified(&found, &known).first() {
        panic!(
            "`impl Drive for {name}` in {} is in none of WRAPPERS, DELEGATING and OTHERS of drive_coverage.rs: \
             a drive that sits on another one forwards every call or says why not",
            path.display()
        );
    }
    for name in known {
        assert!(
            found.iter().any(|(found_name, _)| found_name == name),
            "{name} is listed here and no shipped file implements Drive for it"
        );
    }
    for (_, reason) in OTHERS {
        assert!(!reason.is_empty());
    }
}

// ==== What a wrapper DOES with a call: a probe below it (X-DELTA-STRUCT3-8) ====
//
// The census above reads NAMES: a wrapper that names `get_range_if` and answers it with `get_range` (the
// condition dropped) passes every check of this file, and the sync's ranged downloads, which rest on the
// pin (X-DELTA-SYNC2-3, X-DELTA-SYNC4-12), read an object that was replaced and put back as a splice. So each
// hand-written wrapper is put over a `Probe` - a drive that only listens - and ONE pinned read is sent through
// it: what the probe heard is what the wrapper did.

use std::sync::Arc;

use crate::erase::{Chose, Target, Why};
use crate::testing::{
    conditional_write_arrives, every_method, pinned_read_arrives, versioned_batches_arrive,
    ConditionalWrite, Heard, Probe, METHOD_NAMES,
};

/// `Box`, `Arc` and `&` of a drive are drives (`forward_drive!`): each sends a pinned read down with its
/// condition, as a read of the key it was given. RED against a pointer impl that answers `get_range_if` with
/// `get_range` (the probe hears a plain `get_range`).
#[test]
fn x_delta_struct3_8_the_pointer_impls_send_a_pinned_read_down_with_its_condition() {
    let probe = Probe::new();
    pinned_read_arrives(&Arc::new(probe.clone()), &probe, "object").expect("Arc<D>");
    pinned_read_arrives(&Box::new(probe.clone()), &probe, "object").expect("Box<D>");
    pinned_read_arrives(&&probe, &probe, "object").expect("&D");
    // ... and through all three at once.
    let all: Box<Arc<Probe>> = Box::new(Arc::new(probe.clone()));
    pinned_read_arrives(&all, &probe, "object").expect("Box<Arc<D>>");
    assert_eq!(
        probe
            .heard()
            .iter()
            .filter(|c| c.method == "get_range_if")
            .count(),
        4,
        "one pinned read per pointer, no more"
    );
}

/// A grant's drive rewrites the key of a pinned read - `p/` in front of it - and sends the condition as it is.
/// RED against a `get_range_if` that calls `get_range`, or one that sends the key unscoped.
#[test]
fn x_delta_struct3_8_a_scoped_drive_sends_a_pinned_read_with_its_condition_under_the_scope() {
    let probe = Probe::new();
    for can_write in [true, false] {
        let scoped = crate::scoped::ScopedDrive::new(probe.clone(), "p/", can_write).unwrap();
        pinned_read_arrives(&scoped, &probe, "p/object")
            .unwrap_or_else(|why| panic!("a grant that writes: {can_write}: {why:?}"));
    }
}

/// The encryption's gate sends a pinned read down as it is while the drive is new (nothing in it is encrypted: the
/// read is the bucket's own) - the key and the condition as given. RED against a `get_range_if` that called
/// `get_range`.
#[cfg(feature = "encryption")]
#[test]
fn x_delta_struct3_8_the_encryption_gate_sends_a_pinned_read_down_with_its_condition() {
    let probe = Probe::new();
    let auto = crate::AutoEncrypted::new(
        Arc::new(probe.clone()),
        "d_1",
        Arc::new(crate::keyring::MemoryKeyring::new()),
        None,
    );
    pinned_read_arrives(&auto, &probe, "object").expect("AutoEncrypted over a new drive");
}

/// The call of every method of [`crate::Drive`] (`testing::every_method`) is made from THE list: the names it
/// calls are `drive_methods!`'s, each once, in its order - a method the trait gets next that the call does not
/// make fails here, so a census built on it (a stop flag that must refuse every request, a drive that has
/// nothing below it) covers it at once.
#[test]
fn x_delta_struct3_8_the_call_of_every_method_is_the_list_and_nothing_else() {
    let lib = include_str!("../lib.rs");
    let listed: Vec<String> = list_entries(lib).into_iter().map(|e| e.name).collect();
    assert_eq!(METHOD_NAMES, listed.as_slice(), "the const is the list");
    let dir = crate::testing::TempDir::new("struct3-8-every-method");
    let probe = Probe::new();
    // The file and the reasons of the call are made here: the support module makes neither (the chose gate and
    // the durable gate read it), a test item may.
    let file = dir.path().join("one-byte.txt");
    std::fs::write(&file, b"x").unwrap();
    let keys = Chose::job(
        "a call of every method",
        Target::Keys(vec![String::from("k")]),
    );
    let folder = Chose::job("a call of every method", Target::Folder(String::from("f/")));
    let called = every_method(
        &probe,
        &file,
        || Why::UserChose(&keys),
        || Why::UserChose(&folder),
    );
    let names: Vec<&str> = called.iter().map(|c| c.name).collect();
    assert_eq!(
        names, METHOD_NAMES,
        "every method of the list is called, once, in the list's order"
    );
    assert!(
        called
            .iter()
            .any(|c| c.name == "get_range_if" && c.outcome.is_ok()),
        "the pinned read reached the probe, which answers it"
    );
    // The checked upload of the call arrived with the check of the file it names (X-DELTA-DRIVEE-7: the call asserted
    // nothing about its check).
    let heard = probe.heard();
    let checked: Vec<&Heard> = heard
        .iter()
        .filter(|call| call.method == "put_file_checked")
        .collect();
    assert_eq!(
        checked.len(),
        1,
        "the checked upload was heard once: {heard:?}"
    );
    assert_eq!(
        checked[0]
            .check
            .as_ref()
            .map(|check| (check.size, check.blake3.as_str())),
        Some((1, blake3::hash(b"x").to_hex().as_str())),
        "the check of the one-byte file"
    );
}

/// The shipped drives that sit on no drive: none of them holds a drive below it - a `dyn Drive`, an `Arc` of
/// one or a type parameter that is one. A wrapper is a wrapper, whatever it is called here: `Stoppable` held a
/// `&dyn Drive`, wrote seven methods and was filed under "an adapter of the few calls one caller makes", so the
/// pinned read was dropped by a wrapper no census read (X-DELTA-STRUCT3-6).
#[test]
fn x_delta_struct3_8_a_drive_filed_as_no_wrapper_holds_no_drive_below_it() {
    let impls = shipped_impls();
    for (name, _) in OTHERS {
        for (found, path) in &impls {
            if found != name {
                continue;
            }
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("{name}: {} cannot be read ({e})", path.display()));
            if let Some(held) = holds_a_drive(&text, name) {
                panic!(
                    "{name} ({}) is filed in OTHERS and holds a drive below it ({held}): it is a wrapper - \
                     delegate_drive! if it forwards every call, WRAPPERS with a body test if it does not",
                    path.display()
                );
            }
        }
    }
}

/// The tokens of `source` (comments left out, a string literal one token) with the byte range each came from, and whether the
/// shipped code has it: what only a test build compiles (`#[cfg(test)]`, `#[test]`, a file under `#![cfg(test)]`) is not
/// shipped. The census reads items through this and not through lines: an `impl` or a `struct` inside a module is found as it
/// is at the start of a line, and a test module's is none (X-DELTA-DRIVEE-7: the finders read the start of a line only).
fn shipped_tokens(source: &str) -> (Vec<Spanned>, Vec<bool>) {
    let (tokens, _) = lex_spans(source);
    let plain: Vec<Tok> = tokens.iter().map(|(token, _)| token.clone()).collect();
    let mut shipped = vec![true; tokens.len()];
    for range in test_items(&plain) {
        for at in range {
            if let Some(token) = shipped.get_mut(at) {
                *token = false;
            }
        }
    }
    (tokens, shipped)
}

/// The text of the struct `name` in `source`, from its `struct` keyword to the end of its fields (the closing brace, or the `;`
/// of a tuple or unit struct), when there is one - wherever it stands, at the start of a line or inside a module, and not
/// when only a test build compiles it.
fn struct_text<'a>(source: &'a str, name: &str) -> Option<&'a str> {
    let (tokens, shipped) = shipped_tokens(source);
    let at = (0..tokens.len().saturating_sub(1)).find(|&at| {
        shipped[at] && tokens[at].0.is_ident("struct") && tokens[at + 1].0.is_ident(name)
    })?;
    let plain: Vec<Tok> = tokens.iter().map(|(token, _)| token.clone()).collect();
    let end = item_end(&plain, at).max(at + 1);
    Some(&source[tokens[at].1.start..tokens[end - 1].1.end])
}

/// What shows that `source`'s struct `name` holds a drive below it: a `dyn .. Drive`, an `Arc`, `Box` or
/// reference of a drive, a type parameter bounded by `Drive` - in the header or in a `where` clause on any line, behind
/// any path (`D: Drive`, `D: Send + Drive`, `D: crate::Drive`, `D: azul_storage::Drive`). `None` for a leaf. It read
/// the struct's first line for `: Drive` only (X-DELTA-STRUCT4-4: a `where` clause, `crate::Drive` and `D: Send + Drive`
/// passed, and a new wrapper `struct Mirror<D> where D: Drive` was filed in OTHERS and never looked at); the type a
/// wrapper holds when it is a concrete one (`Arc<S3Drive>`) is the limit: the census cannot tell it from a leaf's field.
fn holds_a_drive(source: &str, name: &str) -> Option<String> {
    let text = struct_text(source, name)?;
    // Comments say what they like: a doc line that spells `D: Drive` is no bound.
    let code: String = text
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut rest = code.as_str();
    while let Some(at) = rest.find("dyn ") {
        rest = &rest[at + 4..];
        let path: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || matches!(c, '_' | ':'))
            .collect();
        if path == "Drive" || path.ends_with("::Drive") {
            return Some(format!("`dyn {path}`"));
        }
    }
    bounded_by_drive(&code)
}

/// Whether `code` (a struct's text, comments left out) bounds a type by the trait `Drive`: the word `Drive` - not
/// `DriveError`, not `Drives` - behind a `:` or a `+`, whatever path it is spelled with.
fn bounded_by_drive(code: &str) -> Option<String> {
    let bytes = code.as_bytes();
    let is_word = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    for (at, _) in code.match_indices("Drive") {
        let whole_word = (at == 0 || !is_word(bytes[at - 1]))
            && bytes.get(at + "Drive".len()).is_none_or(|b| !is_word(*b));
        if !whole_word {
            continue;
        }
        // Back over a path (`crate::`, `azul_storage::`).
        let mut start = at;
        while let Some(path) = code[..start].trim_end().strip_suffix("::") {
            let path = path.trim_end();
            let name = path.bytes().rev().take_while(|b| is_word(*b)).count();
            start = path.len() - name;
        }
        let before = code[..start].trim_end();
        if (before.ends_with(':') && !before.ends_with("::")) || before.ends_with('+') {
            return Some(String::from("a type parameter bounded by `Drive`"));
        }
    }
    None
}

/// The types `source` writes `Drive` for - `impl Drive for X`, `impl<D: Drive> Drive for X<D>`, `impl azul_storage::Drive for X`,
/// at the start of a line or inside a module - and the types `delegate_drive!(X)` makes it for, in the order they stand. What
/// only a test build compiles is left out, `SyncDrive` is no `Drive`, and the `$wrapper` of a macro is no name.
fn drive_impls_in(source: &str) -> Vec<String> {
    let (tokens, shipped) = shipped_tokens(source);
    let mut found = Vec::new();
    for (at, (token, _)) in tokens.iter().enumerate() {
        if !shipped[at] {
            continue;
        }
        if token.is_ident("impl") {
            // The header: up to the `{` of the impl - its generics, the trait, the type and a `where` clause.
            let end = tokens[at..]
                .iter()
                .position(|(token, _)| token.is_punct('{') || token.is_punct(';'))
                .map_or(tokens.len(), |len| at + len);
            let header = &tokens[at..end];
            for (n, (token, _)) in header.iter().enumerate() {
                let for_next = header
                    .get(n + 1)
                    .is_some_and(|(next, _)| next.is_ident("for"));
                if token.is_ident("Drive") && for_next {
                    if let Some(name) = header.get(n + 2).and_then(|(named, _)| named.ident()) {
                        found.push(name.to_string());
                    }
                }
            }
        } else if token.is_ident("delegate_drive") {
            // `delegate_drive!(X)`, behind any path: the type the macro makes the impl for.
            if let [(bang, _), (open, _), (named, _)] = &tokens[at + 1..(at + 4).min(tokens.len())]
            {
                if bang.is_punct('!') && open.is_punct('(') {
                    if let Some(name) = named.ident() {
                        found.push(name.to_string());
                    }
                }
            }
        }
    }
    found
}

/// Every `impl Drive for X` / `delegate_drive!(X)` of the shipped code, with its file: what the census above walks, as a list
/// (a file of a test build - a `tests` or `e2e` folder, a file named `*tests` or `*_test` - is none).
fn shipped_impls() -> Vec<(String, std::path::PathBuf)> {
    fn is_test_source(path: &std::path::Path) -> bool {
        path.components()
            .any(|c| matches!(c.as_os_str().to_str(), Some("tests" | "e2e" | "target")))
            || path
                .file_stem()
                .and_then(|s| s.to_str())
                .is_some_and(|stem| stem.ends_with("tests") || stem.ends_with("_test"))
    }
    fn walk(dir: &std::path::Path, found: &mut Vec<(String, std::path::PathBuf)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if is_test_source(&path) {
                continue;
            }
            if path.is_dir() {
                walk(&path, found);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                // The lexer reads only a file that can name one.
                if !text.contains("Drive") && !text.contains("delegate_drive") {
                    continue;
                }
                for name in drive_impls_in(&text) {
                    found.push((name, path.clone()));
                }
            }
        }
    }
    let mut found = Vec::new();
    let Ok(crates) = std::fs::read_dir(examples_dir()) else {
        panic!("the examples folder cannot be read");
    };
    for krate in crates.flatten() {
        walk(&krate.path().join("src"), &mut found);
    }
    found
}

/// What the body of a row's test must name when the row's label is no type a test can write: `&` is no name, and the drive below
/// the bytes of an Azlin drive is a private type of the kit that its tests reach through `DriveDelegate::below`. A label that
/// is a type is its own name (X-DELTA-DRIVEE-7: a row whose test names another wrapper proved nothing about this one).
const NAMED_AS: &[(&str, &[&str])] = &[
    ("Box, Arc and & of a drive", &["Arc", "Box"]),
    ("NoDriveBelow", &["below"]),
];

/// The names the test of the row `wrapper` must write in its body.
fn names_of_row(wrapper: &str) -> Vec<&str> {
    NAMED_AS
        .iter()
        .find(|(label, _)| *label == wrapper)
        .map_or_else(|| vec![wrapper], |(_, names)| names.to_vec())
}

/// Why the test a census row names proves nothing about the wrapper of the row, or `None` when it does: it is in `source`, it is
/// a `#[test]`, it is not ignored, its body NAMES the wrapper (a type: an identifier, not a word of a string or a comment) and
/// calls one of `words` (an identifier, or a string such as a header's name: a word in a comment is none). A row that points at
/// another wrapper's test, or at a test whose comment says the word, passed the old census: it asked that the test exist, that
/// it be a test and that its body hold a word, and the body of a test in a module ran to the end of the module.
fn row_problem(wrapper: &str, source: &str, test: &str, words: &[&str]) -> Option<String> {
    let (tokens, _) = lex_spans(source);
    let plain: Vec<Tok> = tokens.iter().map(|(token, _)| token.clone()).collect();
    let Some(at) = (0..plain.len().saturating_sub(2)).find(|&at| {
        plain[at].is_ident("fn")
            && plain[at + 1].is_ident(test)
            && (plain[at + 2].is_punct('(') || plain[at + 2].is_punct('<'))
    }) else {
        return Some(format!("the test `{test}` is not in the file"));
    };
    // Its attributes: the lines right above it, back to the blank line before the doc comment (a test in a module is
    // indented: the indent is no line).
    let above = source[..tokens[at].1.start].trim_end_matches([' ', '\t']);
    let attributes: Vec<&str> = above
        .lines()
        .rev()
        .take_while(|line| !line.trim().is_empty())
        .collect();
    if !attributes.iter().any(|line| line.trim() == "#[test]") {
        return Some(format!("`{test}` is no #[test] ({attributes:?})"));
    }
    if attributes.iter().any(|line| line.contains("ignore")) {
        return Some(format!("`{test}` is ignored"));
    }
    let end = item_end(&plain, at).max(at + 1);
    let body = &plain[at..end];
    let ident = |name: &str| body.iter().any(|token| token.is_ident(name));
    let string = |part: &str| {
        body.iter()
            .any(|token| matches!(token, Tok::Str(text) if text.contains(part)))
    };
    for name in names_of_row(wrapper) {
        if !ident(name) {
            return Some(format!(
                "the body of `{test}` does not name `{name}`: a row for a wrapper is the test of THAT wrapper"
            ));
        }
    }
    if !words.iter().any(|word| ident(word) || string(word)) {
        return Some(format!("the body of `{test}` calls none of {words:?}"));
    }
    None
}

/// The test that looks at what ARRIVES below each wrapper, one row per wrapper that decides for itself what to
/// do with a call: `(wrapper, file below examples/, the test)`. A pinned read sent through it over a probe (a
/// leaf that listens), or - for a wrapper whose probe is a fake bucket - at the transport; `NoDriveBelow` is
/// the drive that refuses everything, and its test calls every method of the list.
const BODY_TESTS: &[(&str, &str, &str)] = &[
    (
        "Box, Arc and & of a drive",
        "azul-storage/src/tests/drive_coverage.rs",
        "x_delta_struct3_8_the_pointer_impls_send_a_pinned_read_down_with_its_condition",
    ),
    (
        "ScopedDrive",
        "azul-storage/src/tests/drive_coverage.rs",
        "x_delta_struct3_8_a_scoped_drive_sends_a_pinned_read_with_its_condition_under_the_scope",
    ),
    (
        "AutoEncrypted",
        "azul-storage/src/tests/drive_coverage.rs",
        "x_delta_struct3_8_the_encryption_gate_sends_a_pinned_read_down_with_its_condition",
    ),
    (
        "EncryptedDrive",
        "azul-storage/src/tests/encrypted.rs",
        "x_delta_sync2_3_a_pinned_range_is_of_the_object_head_showed_and_of_no_other",
    ),
    (
        "AccountDrive",
        "azul-bridge/src/account.rs",
        "x_delta_struct3_8_the_bridges_account_drive_sends_a_pinned_read_with_its_if_match",
    ),
    (
        "Stops",
        "azul-drive/src/stop_jobs_tests.rs",
        "x_delta_struct3_8_stops_sends_a_pinned_read_down_with_its_condition_and_refuses_it_once_the_flag_is_up",
    ),
    (
        "Stoppable",
        "azcloud-kit/src/tests/sync_hostile.rs",
        "x_delta_struct3_8_the_downloads_drive_pins_a_read_and_refuses_every_request_once_its_pass_is_gone",
    ),
    (
        "NoDriveBelow",
        "azcloud-kit/src/tests/drive.rs",
        "x_delta_struct3_8_the_drive_below_the_bytes_refuses_every_request_and_answers_every_question_with_no",
    ),
];

/// The census learns BODIES: every wrapper that writes a method for itself (`WRAPPERS`, the pointer impls, the
/// delegating wrappers that decide what a call does - `Stops`, `Stoppable` - and the drive that refuses) has a
/// named test that exists, is not ignored and looks at what arrives below it. A wrapper that names `get_range_if`
/// and answers it with `get_range` passes every check of the names above; it fails the test named here.
#[test]
fn x_delta_struct3_8_every_wrapper_that_decides_for_itself_has_a_test_of_what_arrives_below_it() {
    for (name, _, _) in WRAPPERS {
        assert!(
            BODY_TESTS.iter().any(|(wrapper, _, _)| wrapper == name),
            "{name} writes every method for itself and has no row in BODY_TESTS: a test that sends a pinned \
             read through it over a probe"
        );
    }
    for must in [
        "Box, Arc and & of a drive",
        "Stops",
        "Stoppable",
        "NoDriveBelow",
    ] {
        assert!(
            BODY_TESTS.iter().any(|(wrapper, _, _)| *wrapper == must),
            "{must} has no row"
        );
    }
    for (name, file, test) in BODY_TESTS {
        let path = examples_dir().join(file);
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{name}: {} cannot be read ({e})", path.display()));
        // A row's test is the test of its wrapper (X-DELTA-DRIVEE-7: it named the test, not what the test is about).
        if let Some(problem) = row_problem(
            name,
            &source,
            test,
            &[
                "pinned_read_arrives",
                "get_range_if",
                "every_method",
                "every_method_of",
            ],
        ) {
            panic!("{name} ({file}): {problem} - a test that sends a pinned read through it or calls every method");
        }
    }
}

/// X-DELTA-STRUCT4-4. `holds_a_drive` read the first line of a struct for `: Drive`: a wrapper with its bound in a `where`
/// clause, behind `crate::`, or after another bound was filed in OTHERS and never looked at. Every spelling is found
/// now, and what is no bound is not: a doc line that spells one, `DriveError`, a field called `drives`.
#[test]
fn x_delta_struct4_4_a_drive_bound_is_found_wherever_the_struct_spells_it() {
    let found = |text: &str| holds_a_drive(text, "Mirror").is_some();
    for (what, text) in [
        (
            "the first line",
            "pub struct Mirror<D: Drive> {\n    inner: D,\n}\n",
        ),
        (
            "a where clause on the first line",
            "struct Mirror<D> where D: Drive {\n    inner: D,\n}\n",
        ),
        (
            "a where clause on its own lines",
            "pub struct Mirror<D>\nwhere\n    D: Send + Sync + Drive,\n{\n    inner: D,\n}\n",
        ),
        (
            "another bound before it",
            "struct Mirror<D: Send + Drive> {\n    inner: D,\n}\n",
        ),
        (
            "a path of the crate",
            "struct Mirror<D: crate::Drive> {\n    inner: D,\n}\n",
        ),
        (
            "a path of the storage crate",
            "struct Mirror<D>\nwhere\n    D: azul_storage::Drive,\n{\n    inner: D,\n}\n",
        ),
        (
            "a boxed trait object",
            "struct Mirror {\n    inner: Box<dyn Drive>,\n}\n",
        ),
        (
            "an arc of a trait object by path",
            "struct Mirror {\n    inner: Arc<dyn azul_storage::Drive>,\n}\n",
        ),
    ] {
        assert!(found(text), "{what}: not found in {text:?}");
    }
    for (what, text) in [
        ("no drive", "struct Mirror {\n    count: usize,\n}\n"),
        (
            "a doc line that spells a bound",
            "struct Mirror {\n    // D: Drive would make it a wrapper\n    count: usize,\n}\n",
        ),
        (
            "an error",
            "struct Mirror {\n    last: Option<DriveError>,\n}\n",
        ),
        (
            "a name that holds the word",
            "struct Mirror {\n    drives: usize,\n    my: MyDrive,\n}\n",
        ),
    ] {
        assert!(!found(text), "{what}: found in {text:?}");
    }
}

/// The one-byte file the uploads of a file read; the support module writes none.
fn one_byte_file(what: &str) -> (crate::testing::TempDir, std::path::PathBuf) {
    let dir = crate::testing::TempDir::new(what);
    let file = dir.path().join("one-byte.txt");
    std::fs::write(&file, b"x").unwrap();
    (dir, file)
}

/// X-DELTA-STRUCT4-4. `Box`, `Arc` and `&` of a drive send EVERY write that carries a condition down with it - `put_if`,
/// `put_from_if`, `put_file_if`, and an item of `put_many`, `put_from_many`, `delete_many`, `move_many` with a version - as
/// ONE write of the same method at the drive below. RED against a pointer impl that answers `put_if` with `put`: the
/// probe hears an unconditional write.
#[test]
fn x_delta_struct4_4_the_pointer_impls_send_every_conditional_write_down_with_its_condition() {
    let (_dir, file) = one_byte_file("struct4-4-pointers");
    let probe = Probe::new();
    let both = |wrapper: &dyn crate::Drive, what: &str| {
        conditional_write_arrives(wrapper, &probe, &file, "object")
            .unwrap_or_else(|why| panic!("{what}: {why:?}"));
        versioned_batches_arrive(wrapper, &probe, "object")
            .unwrap_or_else(|why| panic!("{what}: {why:?}"));
    };
    both(&Arc::new(probe.clone()), "Arc<D>");
    both(&Box::new(probe.clone()), "Box<D>");
    both(&&probe, "&D");
    both(&Box::new(Arc::new(probe.clone())), "Box<Arc<D>>");
    assert_eq!(
        probe
            .heard()
            .iter()
            .filter(|call| call.method == "put_if")
            .count(),
        4,
        "one conditional write per pointer, no more"
    );
}

/// X-DELTA-STRUCT4-4. A grant's drive rewrites the key of a conditional write - `p/` in front of it - and sends the
/// condition as it is. A read-only grant refuses the write and reaches nothing. RED against a `put_if` that calls `put`, a
/// batch that drops its version, or one that sends the key unscoped.
#[test]
fn x_delta_struct4_4_a_scoped_drive_sends_every_conditional_write_with_its_condition_under_the_scope(
) {
    let (_dir, file) = one_byte_file("struct4-4-scoped");
    let probe = Probe::new();
    let scoped = crate::scoped::ScopedDrive::new(probe.clone(), "p/", true).unwrap();
    conditional_write_arrives(&scoped, &probe, &file, "p/object")
        .unwrap_or_else(|why| panic!("a grant that writes: {why:?}"));
    let heard = probe.heard().len();
    let read_only = crate::scoped::ScopedDrive::new(probe.clone(), "p/", false).unwrap();
    assert!(
        matches!(
            conditional_write_arrives(&read_only, &probe, &file, "p/object"),
            Err(ConditionalWrite::Refused { .. })
        ),
        "a read-only grant takes no write"
    );
    assert_eq!(
        probe.heard().len(),
        heard,
        "and nothing reaches the drive below"
    );
}

/// X-DELTA-STRUCT4-4. The encryption's gate over a NEW drive takes no write: every conditional write is refused and no
/// write of any kind reaches the bucket below it. It is NOT the proof that the gate sends a condition down: a new drive
/// refuses before the condition is looked at, so a gate that dropped it passed (X-DELTA-DRIVEE-6, replayed from
/// `mutants/X-DELTA-DRIVEE-6.patch`). That is the other row of `AutoEncrypted`'s: the same writes through a gate over an
/// admitted drive, between two devices (`x_delta_drivee_6_*` of tests/encrypted.rs).
#[cfg(feature = "encryption")]
#[test]
fn x_delta_struct4_4_the_encryption_gate_lets_no_write_through_without_its_condition() {
    let (_dir, file) = one_byte_file("struct4-4-gate");
    let probe = Probe::new();
    let auto = crate::AutoEncrypted::new(
        Arc::new(probe.clone()),
        "d_1",
        Arc::new(crate::keyring::MemoryKeyring::new()),
        None,
    );
    assert!(
        matches!(
            conditional_write_arrives(&auto, &probe, &file, "object"),
            Err(ConditionalWrite::Refused { call: "put_if", .. })
        ),
        "a new drive takes no write"
    );
    let writes: Vec<_> = probe
        .heard()
        .into_iter()
        .filter(|call| {
            !matches!(
                call.method,
                "list" | "get" | "get_range" | "get_range_if" | "head"
            )
        })
        .collect();
    assert_eq!(writes, Vec::new(), "nothing was written below the gate");
}

/// The methods of `drive_methods!` that carry a condition, a version or a check - their arguments name `Precondition`,
/// `BatchPut`, `BatchDelete`, `BatchMove`, `BatchStream` or `FileCheck` - in the list's order: a wrapper that writes one of them
/// decides for itself what happens to it (X-DELTA-STRUCT4-4). Derived from the list, not written out: the seven names of the
/// hand list left out `put_file_checked` (X-DELTA-DRIVEE-7), and a method the trait gets next that carries one of them is a
/// method a wrapper can drop it from, so it is in the list at once.
fn conditional_methods(lib: &str) -> Vec<Entry> {
    const CARRIED: [&str; 6] = [
        "Precondition",
        "BatchPut",
        "BatchDelete",
        "BatchMove",
        "BatchStream",
        "FileCheck",
    ];
    list_entries(lib)
        .into_iter()
        .filter(|entry| {
            let arguments = entry
                .signature
                .split(" -> ")
                .next()
                .unwrap_or(&entry.signature);
            CARRIED.iter().any(|carried| arguments.contains(carried))
        })
        .collect()
}

/// What the test a CONDITION row names may call to be one of conditional writes: the helpers of `testing` (and the crates' own
/// `every_method_of`, which calls `every_method`), a header's name and every WRITE of [`conditional_methods`] (a pinned read, `get_range_if`, is BODY_TESTS's; a `compose` call is the trait's
/// default over `delete_many`).
fn condition_words(lib: &str) -> Vec<String> {
    let mut words: Vec<String> = [
        "conditional_write_arrives",
        "versioned_batches_arrive",
        "if-match",
        "every_method",
        "every_method_of",
    ]
    .iter()
    .map(|word| (*word).to_string())
    .collect();
    words.extend(
        conditional_methods(lib)
            .into_iter()
            .filter(|entry| entry.kind != "compose" && !entry.name.starts_with("get_"))
            .map(|entry| entry.name),
    );
    words
}

/// The test that looks at what a CONDITIONAL WRITE does below each wrapper that decides for itself what to do with one:
/// `(wrapper, file below examples/, the test)`. Over a probe (a leaf that listens; the key as it arrives, the condition
/// as it is) or - for a wrapper whose leaf is a fake bucket - at the transport; the two encryption wrappers rewrite the
/// write and their rows name the test of what the condition does to what is stored. A wrapper that retries has the
/// other half of it: the condition of the second try is the first's.
const CONDITION_TESTS: &[(&str, &str, &str)] = &[
    (
        "Box, Arc and & of a drive",
        "azul-storage/src/tests/drive_coverage.rs",
        "x_delta_struct4_4_the_pointer_impls_send_every_conditional_write_down_with_its_condition",
    ),
    (
        "ScopedDrive",
        "azul-storage/src/tests/drive_coverage.rs",
        "x_delta_struct4_4_a_scoped_drive_sends_every_conditional_write_with_its_condition_under_the_scope",
    ),
    (
        "AutoEncrypted",
        "azul-storage/src/tests/encrypted.rs",
        "x_delta_drivee_6_an_admitted_gate_writes_a_file_only_while_it_is_unchanged",
    ),
    (
        "AutoEncrypted",
        "azul-storage/src/tests/encrypted.rs",
        "x_delta_drivee_6_an_admitted_gate_writes_a_streamed_body_and_a_file_only_while_the_name_is_unchanged",
    ),
    (
        "AutoEncrypted",
        "azul-storage/src/tests/encrypted.rs",
        "x_delta_drivee_6_an_admitted_gate_holds_the_condition_and_the_version_of_every_item_of_a_batch",
    ),
    (
        "AutoEncrypted",
        "azul-storage/src/tests/encrypted.rs",
        "x_delta_drivee_6_an_admitted_gate_sends_a_checked_upload_with_its_check",
    ),
    (
        "AutoEncrypted",
        "azul-storage/src/tests/drive_coverage.rs",
        "x_delta_struct4_4_the_encryption_gate_lets_no_write_through_without_its_condition",
    ),
    (
        "EncryptedDrive",
        "azul-storage/src/tests/encrypted.rs",
        "a_conditional_write_holds_only_while_the_file_is_unchanged",
    ),
    (
        "AccountDrive",
        "azul-bridge/src/account.rs",
        "x_delta_struct4_4_the_bridges_account_drive_sends_every_conditional_write_with_its_if_match",
    ),
    (
        "Stops",
        "azul-drive/src/stop_jobs_tests.rs",
        "x_delta_struct4_4_stops_sends_every_conditional_write_down_with_its_condition_and_refuses_them_once_the_flag_is_up",
    ),
    (
        "Stoppable",
        "azcloud-kit/src/tests/sync_hostile.rs",
        "x_delta_struct4_4_the_downloads_drive_sends_every_conditional_write_down_with_its_condition",
    ),
    (
        "RetryingDrive",
        "azcloud-kit/src/tests/sync_retry.rs",
        "x_delta_struct4_4_a_retrying_drive_sends_every_conditional_write_with_its_condition_and_asks_the_same_condition_on_every_try",
    ),
    (
        "ObjectCache",
        "azcloud-kit/src/tests/sync_retry.rs",
        "x_delta_struct4_4_the_object_cache_sends_every_conditional_write_down_with_its_condition",
    ),
    (
        "AzlinDrive",
        "azcloud-kit/src/tests/drive.rs",
        "x_delta_struct4_4_an_azlin_drive_sends_its_conditional_writes_to_the_bucket_with_their_condition",
    ),
    (
        "NoDriveBelow",
        "azcloud-kit/src/tests/drive.rs",
        "x_delta_struct3_8_the_drive_below_the_bytes_refuses_every_request_and_answers_every_question_with_no",
    ),
];

/// X-DELTA-STRUCT4-4. The census learns CONDITIONS: every wrapper that writes one of [`conditional_methods`] for itself - the
/// hand-written ones (`WRAPPERS`), the pointer impls and every delegating wrapper whose `DriveDelegate` has one of them
/// (`RetryingDrive`, `ObjectCache`, `AzlinDrive`, `Stops`, `Stoppable`: read from the file, so a new one is found) - has a
/// named test that exists, is a test, is not ignored, NAMES the wrapper and sends a conditional write through it
/// ([`row_problem`]). A wrapper that answers `put_if` with `put`, or retries a write with `Precondition::Absent`, turns an
/// `If-Match` write into an overwrite of what another device just wrote (the AZ-M1 / SYNC4-1 class); the old census passed it,
/// and passed a row that pointed at another wrapper's test (X-DELTA-DRIVEE-7).
#[test]
fn x_delta_struct4_4_every_wrapper_that_writes_a_conditional_body_has_a_test_that_its_conditions_arrive(
) {
    let lib = include_str!("../lib.rs");
    let conditional: Vec<String> = conditional_methods(lib)
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    assert!(
        conditional.len() >= 8,
        "the conditional methods were derived: {conditional:?}"
    );
    let rows = |name: &str| {
        CONDITION_TESTS
            .iter()
            .any(|(wrapper, _, _)| *wrapper == name)
    };
    for (name, _, _) in WRAPPERS {
        assert!(
            rows(name),
            "{name} writes every method for itself and has no row in CONDITION_TESTS"
        );
    }
    for must in ["Box, Arc and & of a drive", "NoDriveBelow"] {
        assert!(rows(must), "{must} has no row in CONDITION_TESTS");
    }
    for (name, file) in DELEGATING {
        let path = examples_dir().join(file);
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{name}: {} cannot be read ({e})", path.display()));
        let spelled = if source.contains(&format!("azul_storage::delegate_drive!({name}<'_>);")) {
            format!("{name}<'_>")
        } else {
            (*name).to_string()
        };
        let written = methods_of(
            &source,
            &format!("impl azul_storage::DriveDelegate for {spelled} {{"),
        );
        let decides: Vec<&str> = written
            .iter()
            .map(String::as_str)
            .filter(|method| conditional.iter().any(|carried| carried == method))
            .collect();
        if !decides.is_empty() {
            assert!(
                rows(name),
                "{name} ({file}) writes {decides:?} for itself and has no row in CONDITION_TESTS: a test that sends a \
                 conditional write through it over a probe"
            );
        }
    }
    let words = condition_words(lib);
    let words: Vec<&str> = words.iter().map(String::as_str).collect();
    for (name, file, test) in CONDITION_TESTS {
        let path = examples_dir().join(file);
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{name}: {} cannot be read ({e})", path.display()));
        if let Some(problem) = row_problem(name, &source, test, &words) {
            panic!("{name} ({file}): {problem} - a test that sends a conditional write through it");
        }
    }
}

// ==== The census reads what it must read (X-DELTA-DRIVEE-7) ====
//
// The finders read the start of a line, a row was satisfied by a word of the test's body and `CONDITIONAL` was seven names
// written by hand: a wrapper in a module was classified nowhere, a row pointed at another wrapper's test, and a wrapper that
// answered a checked upload with `put_file` passed. Each has its planted case here: it fails, and the clean one passes.

/// An impl inside a module is found like one at the start of a line, and what only a test build compiles is not.
#[test]
fn x_delta_drivee_7_an_impl_inside_a_module_is_found_and_the_impls_of_a_test_build_are_not() {
    let planted = r#"
impl Drive for Plain {}
mod mirror {
    pub struct Mirror<D>(pub D);
    impl<D: Drive> Drive for Mirror<D> {
        fn put_if(&self) {}
    }
}
azul_storage::delegate_drive!(Wrapped);
impl crate::Drive
    for Wrapped2 {}
// impl Drive for InAComment {}
/// impl Drive for InADoc {}
impl SyncDrive for NotADrive {}
macro_rules! pointer {
    ($pointer:ty) => {
        impl<D: Drive + ?Sized> Drive for $pointer {}
    };
}
#[cfg(test)]
mod tests {
    impl Drive for InATestModule {}
}
#[cfg(test)]
impl Drive for ATestItem {}
"#;
    assert_eq!(
        drive_impls_in(planted),
        ["Plain", "Mirror", "Wrapped", "Wrapped2"],
        "the one at the start of a line, the one in the module, the macro's and the one that wraps its header"
    );
    // The census fails on the planted drive until it is classified, and passes once it is.
    let found: Vec<(String, std::path::PathBuf)> = drive_impls_in(planted)
        .into_iter()
        .map(|name| (name, std::path::PathBuf::from("planted.rs")))
        .collect();
    let unknown = unclassified(&found, &["Plain", "Wrapped", "Wrapped2"]);
    assert_eq!(
        unknown
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>(),
        ["Mirror"],
        "the wrapper in the module is classified nowhere"
    );
    assert!(unclassified(&found, &["Plain", "Mirror", "Wrapped", "Wrapped2"]).is_empty());
}

/// A struct inside a module is read to its own end, not to the end of the module: a leaf is no wrapper whatever follows it,
/// and a wrapper in a module is found.
#[test]
fn x_delta_drivee_7_a_struct_inside_a_module_is_read_to_its_own_end() {
    let planted = r#"
mod wrappers {
    pub struct Leaf {
        count: usize,
    }

    pub struct Boxed {
        inner: Box<dyn Drive>,
    }

    pub struct Tuple<D>(pub D)
    where
        D: Drive;

    #[cfg(test)]
    struct OnlyInTests {
        inner: Box<dyn Drive>,
    }
}
"#;
    assert_eq!(
        holds_a_drive(planted, "Leaf"),
        None,
        "a leaf is no wrapper, whatever follows it in its module"
    );
    assert!(
        holds_a_drive(planted, "Boxed").is_some(),
        "a boxed drive in a module"
    );
    assert!(
        holds_a_drive(planted, "Tuple").is_some(),
        "a tuple struct with a where clause"
    );
    assert!(
        struct_text(planted, "OnlyInTests").is_none(),
        "a struct that only a test build compiles is none"
    );
    // The block of an impl in a module ends at its own closing brace, and its methods are the ones at its own depth.
    let module = "mod mirror {\n    impl Drive for Mirror {\n        fn get(&self) {}\n        fn put(&self) {\n            let _ = |x| {};\n        }\n    }\n    impl Drive for Second {\n        fn list(&self) {}\n    }\n}\nimpl Drive for Other {\n    fn head(&self) {}\n}\n";
    assert_eq!(
        methods_of(module, "impl Drive for Mirror {"),
        ["get", "put"],
        "the methods of the first impl of a module, not the module's"
    );
    assert_eq!(methods_of(module, "impl Drive for Second {"), ["list"]);
    assert_eq!(methods_of(module, "impl Drive for Other {"), ["head"]);
}

/// A row is the test of its wrapper: one whose test names another wrapper, is no test, is ignored, or says the word in a
/// comment proves nothing; the clean row passes.
#[test]
fn x_delta_drivee_7_a_row_whose_test_names_another_wrapper_or_says_the_word_in_a_comment_proves_nothing(
) {
    let planted = r#"
mod tests {
    #[test]
    fn the_pointers_send_every_write() {
        let probe = Probe::new();
        conditional_write_arrives(&Box::new(Arc::new(probe.clone())), &probe, &file, "object").unwrap();
    }

    #[test]
    fn mirror_sends_every_write() {
        let probe = Probe::new();
        let mirror = Mirror(probe.clone());
        conditional_write_arrives(&mirror, &probe, &file, "object").unwrap();
    }

    #[test]
    #[ignore]
    fn mirror_is_ignored() {
        conditional_write_arrives(&Mirror(Probe::new()), &probe, &file, "object").unwrap();
    }

    fn mirror_is_no_test() {
        conditional_write_arrives(&Mirror(Probe::new()), &probe, &file, "object").unwrap();
    }

    #[test]
    fn mirror_says_it_in_a_comment() {
        // Mirror sends conditional_write_arrives
        let _ = "Mirror";
    }
}
"#;
    let words = ["conditional_write_arrives", "if-match"];
    assert_eq!(
        row_problem("Mirror", planted, "mirror_sends_every_write", &words),
        None,
        "the clean row"
    );
    for test in [
        "the_pointers_send_every_write",
        "mirror_is_ignored",
        "mirror_is_no_test",
        "mirror_says_it_in_a_comment",
        "no_such_test",
    ] {
        assert!(
            row_problem("Mirror", planted, test, &words).is_some(),
            "a row for Mirror that names `{test}` passed"
        );
    }
    // The pointers' own row names the pointers, and the drive below the bytes names the accessor that hands it out.
    assert_eq!(
        row_problem(
            "Box, Arc and & of a drive",
            planted,
            "the_pointers_send_every_write",
            &words
        ),
        None
    );
    assert_eq!(
        names_of_row("NoDriveBelow"),
        ["below"],
        "a private type of the kit is reached through the bytes' accessor"
    );
    assert_eq!(names_of_row("Mirror"), ["Mirror"]);
}

/// A wrapper made with the macro that forwards every method: the checked upload as well.
struct Faithful {
    below: Probe,
}

impl crate::DriveDelegate for Faithful {
    fn below(&self) -> &dyn crate::Drive {
        &self.below
    }
}

crate::delegate_drive!(Faithful);

/// The same, except that it answers a checked upload with `put_file`: the check of the file is dropped, and a file that
/// changed while it was uploaded is uploaded under the name of what it was.
struct Forgetful {
    below: Probe,
}

impl crate::DriveDelegate for Forgetful {
    fn below(&self) -> &dyn crate::Drive {
        &self.below
    }

    fn put_file_checked(
        &self,
        key: &str,
        path: &std::path::Path,
        _check: &crate::checked::FileCheck,
        ctl: &dyn crate::Ctl,
    ) -> Result<crate::checked::CheckedUpload, crate::DriveError> {
        crate::Drive::put_file(&self.below, key, path, ctl).map(crate::checked::CheckedUpload::Sent)
    }
}

crate::delegate_drive!(Forgetful);

/// The probe hears a checked upload with its check, and the census helpers send one: a wrapper that forwards everything passes,
/// and one that answers it with `put_file` fails - it reaches the probe as a plain `put`, with no check.
#[test]
fn x_delta_drivee_7_a_wrapper_that_drops_the_file_check_fails_the_census_and_a_faithful_one_passes()
{
    use crate::Drive;

    let (_dir, file) = one_byte_file("drivee7-check");
    // The probe hears it with its check.
    let probe = Probe::new();
    let check = crate::checked::FileCheck {
        size: 1,
        modified_ns: 7,
        blake3: String::from("not the hash of anything"),
        dev: 0,
        ino: 0,
    };
    let answer = probe.put_file_checked("k", &file, &check, &crate::ctl::Quiet);
    assert_eq!(answer, Ok(crate::checked::CheckedUpload::Sent(1)));
    assert_eq!(
        probe.heard(),
        [Heard {
            method: "put_file_checked",
            key: String::from("k"),
            condition: None,
            check: Some(check),
        }]
    );
    // The clean wrapper passes every write, the checked upload too.
    let probe = Probe::new();
    let faithful = Faithful {
        below: probe.clone(),
    };
    conditional_write_arrives(&faithful, &probe, &file, "object")
        .expect("a wrapper that forwards every method");
    // The one that drops the check passes the conditional writes and fails the checked upload.
    let probe = Probe::new();
    let forgetful = Forgetful {
        below: probe.clone(),
    };
    match conditional_write_arrives(&forgetful, &probe, &file, "object") {
        Err(ConditionalWrite::NotConditional {
            call: "put_file_checked",
            heard,
        }) => {
            assert!(
                heard.iter().all(|call| call.check.is_none()),
                "no check reached the probe: {heard:?}"
            );
            assert!(
                heard.iter().all(|call| call.method == "put"),
                "what reached it is a plain write: {heard:?}"
            );
        }
        other => panic!("a checked upload answered with put_file passed the census: {other:?}"),
    }
}

/// The conditional methods are the list's, not seven names written out: the entries of `drive_methods!` whose arguments carry a
/// condition, a version or a check - `put_file_checked` is one - and a method the trait gets next that carries one is in at once.
/// And the helpers of the census send every write of them: a derived method that neither helper sends fails here.
#[test]
fn x_delta_drivee_7_the_conditional_methods_are_derived_from_the_list_and_every_write_of_them_is_sent_by_the_helpers(
) {
    use std::collections::BTreeSet;

    let lib = include_str!("../lib.rs");
    let derived: Vec<String> = conditional_methods(lib)
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    for hand_written in [
        "put_if",
        "put_from_if",
        "put_file_if",
        "put_many",
        "put_from_many",
        "delete_many",
        "move_many",
    ] {
        assert!(
            derived.iter().any(|name| name == hand_written),
            "{hand_written} is in the derived list: {derived:?}"
        );
    }
    assert!(
        derived.iter().any(|name| name == "put_file_checked"),
        "the hand list left it out: {derived:?}"
    );
    // A method the list gets next is derived at once; one whose arguments carry none of them is not.
    let planted = "macro_rules! drive_methods {\n    ($callback:path ; $($context:tt)*) => {\n        $callback! {\n            (pass, provided) fn put_checked_if(key: &str, condition: &$crate::Precondition) -> ::core::result::Result<(), $crate::DriveError>;\n            (send, required) fn head(key: &str) -> ::core::result::Result<$crate::ObjectInfo, $crate::DriveError>;\n            (send, provided) fn count(key: &str) -> ::core::result::Result<$crate::Precondition, $crate::DriveError>;\n        }\n    };\n}\n";
    assert_eq!(
        conditional_methods(planted)
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>(),
        ["put_checked_if"]
    );
    // Both helpers over the probe itself: every write of the derived list is heard.
    let (_dir, file) = one_byte_file("drivee7-derived");
    let probe = Probe::new();
    conditional_write_arrives(&probe, &probe, &file, "object")
        .expect("the probe, through the helper");
    versioned_batches_arrive(&probe, &probe, "object").expect("the probe, through the helper");
    let heard: BTreeSet<&str> = probe.heard().iter().map(|call| call.method).collect();
    for entry in conditional_methods(lib) {
        if entry.kind == "compose" || entry.name.starts_with("get_") {
            continue;
        }
        assert!(
            heard.contains(entry.name.as_str()),
            "no helper of the census sends `{}`: a wrapper can drop what it carries and pass",
            entry.name
        );
    }
}

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
//! the first test below keeps true. The wrappers that must write every method on purpose (the
//! encryption wrappers, `ScopedDrive`, `AzlinDrive`, the bridge's account drive) stay in
//! `WRAPPERS` with their per-method census.

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
];

fn methods_of(source: &str, header: &str) -> Vec<String> {
    let start = source
        .find(header)
        .unwrap_or_else(|| panic!("{header:?} not found"));
    let body = &source[start..];
    // The impl or trait ends at the first closing brace in column 0.
    let end = body.find("\n}\n").expect("end of the block");
    body[..end]
        .lines()
        .filter_map(|line| {
            let rest = line
                .strip_prefix("    fn ")
                .or_else(|| line.strip_prefix("            fn "))?;
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
            entry.kind == "send" || entry.kind == "pass",
            "Drive::{}: the kind `{}` is neither `send` nor `pass`",
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
];

/// The drives that sit on top of another drive and write every method themselves, on purpose (a
/// call forwarded as it is would be the bug: the encryption's `Op`, the grant's key rewrite, the
/// credentials): `(type, file below examples/, the impl's header)`. Each defines every method of the
/// trait or has an `// inherits:` comment for it.
const WRAPPERS: &[(&str, &str, &str)] = &[
    (
        "AzlinDrive",
        "azcloud-kit/src/drive.rs",
        "impl Drive for AzlinDrive {",
    ),
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
        "Stoppable",
        "the six calls a download makes, failing once its process is gone: it forwards no more",
    ),
    (
        "Stops",
        "the seven calls the copy of a locked drive reads through, failing with Cancelled once the \
         drive's removal raised its flag: an adapter of the few calls one caller makes, it forwards \
         no more (X-DELTA-AZDRIVE-KEYS-32)",
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

/// The text of the impl that starts with `header` in `source`, to its closing brace in column 0.
fn impl_block<'a>(source: &'a str, header: &str) -> &'a str {
    let start = source
        .find(header)
        .unwrap_or_else(|| panic!("{header:?} not found"));
    let body = &source[start..];
    &body[..body.find("\n}\n").expect("end of the impl")]
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
        assert!(
            source.contains(&format!("azul_storage::delegate_drive!({name});")),
            "{name} ({file}) is listed in DELEGATING and does not call delegate_drive!({name}): its \
             impl Drive is not made"
        );
        assert!(
            !source.contains(&format!("impl Drive for {name} {{")),
            "{name} ({file}) is made with delegate_drive! and writes an impl Drive of its own"
        );
        let header = format!("impl azul_storage::DriveDelegate for {name} {{");
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

/// Every `impl Drive for X` of the shipped code (a column-0 impl in a file that is no test's) is
/// a wrapper of the table or one of the others: a new drive is a decision, not a surprise.
#[test]
fn x_delta_azdrive_files_2_every_shipped_impl_of_drive_is_classified() {
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
                for line in text.lines().filter(|l| l.starts_with("impl")) {
                    // `impl Drive for X {`, `impl<D: Drive> Drive for X<D> {`,
                    // `impl azul_storage::Drive for X {` - but not `impl SyncDrive for X`.
                    let Some(at) = line.find("Drive for ") else {
                        continue;
                    };
                    if line[..at]
                        .chars()
                        .next_back()
                        .is_some_and(|c| c.is_alphanumeric() || c == '_')
                    {
                        continue;
                    }
                    let name: String = line[at + "Drive for ".len()..]
                        .chars()
                        .take_while(|c| c.is_alphanumeric() || *c == '_')
                        .collect();
                    if !name.is_empty() {
                        found.push((name, path.clone()));
                    }
                }
                // `azul_storage::delegate_drive!(X);`: the impl is made by the macro (a comment or
                // a doc line that shows it starts with `//` and is none).
                for line in text.lines() {
                    let line = line.trim_start();
                    let Some(at) = line.find("delegate_drive!(") else {
                        continue;
                    };
                    if line.starts_with("//") || line[..at].contains(' ') {
                        continue;
                    }
                    let name: String = line[at + "delegate_drive!(".len()..]
                        .chars()
                        .take_while(|c| c.is_alphanumeric() || *c == '_')
                        .collect();
                    if !name.is_empty() {
                        found.push((name, path.clone()));
                    }
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
    assert!(found.len() >= 10, "the impls were found: {found:?}");
    let known: Vec<&str> = WRAPPERS
        .iter()
        .map(|(name, _, _)| *name)
        .chain(DELEGATING.iter().map(|(name, _)| *name))
        .chain(OTHERS.iter().map(|(name, _)| *name))
        .collect();
    for (name, path) in &found {
        assert!(
            known.contains(&name.as_str()),
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

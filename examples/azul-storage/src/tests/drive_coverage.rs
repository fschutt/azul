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

/// The methods `LocalDrive` leaves to the trait's default, each with why that is right.
const LOCAL_DEFAULTS: &[(&str, &str)] = &[
    ("resume_folder", "a folder on disk has no multipart state: None"),
    ("put_file", "streams the file through LocalDrive::put_from"),
    ("put_file_checked", "opens the checked file once, streams it through LocalDrive::put_from"),
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

// ==== Every wrapper of a drive (X-DELTA-AZDRIVE-FILES-2) ====

/// The drives that sit on top of another drive: `(type, file below examples/, the impl's header)`.
/// Each defines every method of the trait or has an `// inherits:` comment for it.
const WRAPPERS: &[(&str, &str, &str)] = &[
    (
        "RetryingDrive",
        "azcloud-kit/src/sync/retry.rs",
        "impl Drive for RetryingDrive {",
    ),
    (
        "ObjectCache",
        "azcloud-kit/src/sync/objects.rs",
        "impl Drive for ObjectCache {",
    ),
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
        .chain(OTHERS.iter().map(|(name, _)| *name))
        .collect();
    for (name, path) in &found {
        assert!(
            known.contains(&name.as_str()),
            "`impl Drive for {name}` in {} is in neither WRAPPERS nor OTHERS of drive_coverage.rs: \
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

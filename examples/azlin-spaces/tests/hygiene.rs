//! The crate's hygiene, the part a test can hold (the launch rules of the W1 lanes): the manifest lists
//! exactly the dependencies of the final manifest (and none of the forbidden ones), the crate root forbids
//! unsafe code and denies the panicking constructs, the library code names no panicking call, lenient
//! default, clock, file, network, thread, environment, database or random source, no container whose
//! iteration order is not deterministic (every replica must apply the same entry to the same state), every
//! public item names the plan section it implements and its caller or the later unit it is a seam of, every
//! source file is declared, every plan section a comment cites exists in `planning/architecture.md`
//! (COMMON-W1R R7), every mutation guard has a killer, and every test the brief names exists.
//!
//! `cargo clippy --all-targets -- -D warnings`, `rustfmt --check` and the builds with the features are the
//! lane's holds: cargo cannot run inside `cargo test`.
//!
//! Implements planning/architecture.md section 1.4 (engineering rules: no panics on input, I/O is a
//! parameter, every public item has a caller).

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The keys of a manifest section (`[dependencies]`, `[features]`): the text before ` =`.
fn section_keys(manifest: &str, section: &str) -> Vec<String> {
    let mut inside = false;
    let mut keys = Vec::new();
    for line in manifest.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            inside = t == section;
            continue;
        }
        if !inside || t.is_empty() || t.starts_with('#') {
            continue;
        }
        if let Some((key, _)) = t.split_once('=') {
            keys.push(key.trim().to_string());
        }
    }
    keys
}

/// The value text of one key of a manifest section (up to the end of the line).
fn section_value(manifest: &str, section: &str, key: &str) -> Option<String> {
    let mut inside = false;
    for line in manifest.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            inside = t == section;
            continue;
        }
        if inside {
            if let Some((k, v)) = t.split_once('=') {
                if k.trim() == key {
                    return Some(v.trim().to_string());
                }
            }
        }
    }
    None
}

/// The source files of `src/`, by file name (all flat: this crate has no module folder).
fn source_files() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for entry in fs::read_dir(root().join("src")).expect("src folder") {
        let entry = entry.expect("a directory entry");
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        assert!(
            name.ends_with(".rs"),
            "{name}: only .rs files live under src"
        );
        let text = fs::read_to_string(entry.path()).expect("source file");
        out.push((name, text));
    }
    out.sort();
    out
}

#[test]
fn crate_hygiene() {
    let manifest = fs::read_to_string(root().join("Cargo.toml")).expect("Cargo.toml");

    // The dependencies of the final manifest, exactly: nothing more, nothing less.
    let dependencies: BTreeSet<String> = section_keys(&manifest, "[dependencies]")
        .into_iter()
        .collect();
    let expected: BTreeSet<String> = [
        "azlin-seams",
        "azlin-chain",
        "azlin-policy",
        "azlin-keys",
        "blake3",
        "subtle",
        "thiserror",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    assert_eq!(dependencies, expected, "the dependencies of the crate");
    for forbidden in [
        "tokio",
        "rusqlite",
        "reqwest",
        "rand",
        "proptest",
        "serde",
        "serde_json",
        "azlin-cbor",
        "zeroize",
    ] {
        assert!(
            !dependencies.contains(forbidden),
            "{forbidden} is not a dependency of this crate"
        );
    }
    for section in ["[dev-dependencies]", "[build-dependencies]", "[features]"] {
        assert!(
            section_keys(&manifest, section).is_empty(),
            "{section} is empty (the feature `mutants` is azlin-seams's alone, R19)"
        );
    }
    assert!(!root().join("build.rs").exists());

    // Package facts.
    assert_eq!(
        section_value(&manifest, "[package]", "name").as_deref(),
        Some("\"azlin-spaces\"")
    );
    assert_eq!(
        section_value(&manifest, "[package]", "edition").as_deref(),
        Some("\"2021\"")
    );
    assert_eq!(
        section_value(&manifest, "[package]", "rust-version").as_deref(),
        Some("\"1.91\"")
    );
    assert_eq!(
        section_value(&manifest, "[package]", "publish").as_deref(),
        Some("false")
    );

    // The crate root.
    let lib = fs::read_to_string(root().join("src/lib.rs")).expect("src/lib.rs");
    assert!(lib.contains("#![forbid(unsafe_code)]"));
    for lint in [
        "clippy::unwrap_used",
        "clippy::expect_used",
        "clippy::panic",
        "clippy::unreachable",
        "clippy::todo",
        "clippy::unimplemented",
        "clippy::indexing_slicing",
    ] {
        assert!(lib.contains(lint), "the crate root does not deny {lint}");
    }
    assert!(lib.contains("#![warn(missing_docs)]"));
}

/// Every source file is declared by a `mod` line of `lib.rs`, and every `mod` line has its file.
#[test]
fn every_source_file_is_declared_and_every_module_has_its_file() {
    let lib = fs::read_to_string(root().join("src/lib.rs")).expect("src/lib.rs");
    let mut declared = BTreeSet::new();
    for line in lib.lines() {
        let t = line.trim();
        if t.starts_with("//") {
            continue;
        }
        let t = t.strip_prefix("pub ").unwrap_or(t);
        if let Some(rest) = t.strip_prefix("mod ") {
            if let Some(name) = rest.strip_suffix(';') {
                declared.insert(name.trim().to_string());
            }
        }
    }
    let on_disk: BTreeSet<String> = source_files()
        .into_iter()
        .map(|(name, _)| name.trim_end_matches(".rs").to_string())
        .filter(|name| name != "lib")
        .collect();
    assert_eq!(
        declared, on_disk,
        "lib.rs declares exactly the files of src/"
    );
    let expected: BTreeSet<String> = [
        "access", "admit", "apply", "charge", "epochs", "grant", "kind", "ops", "proposal", "role",
        "state",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    assert_eq!(declared, expected, "the modules of the brief (section 3)");
}

/// The tokens that do not belong in library code (QUALITY rules 4 and 5, launch rule 8).
const BANNED: &[&str] = &[
    "unsafe {",
    "unsafe fn",
    "unsafe impl",
    "unsafe trait",
    "std::fs",
    "std::net",
    "std::io",
    "std::time",
    "std::thread",
    "std::process",
    "std::env",
    "SystemTime",
    "Instant",
    "rand::",
    "tokio",
    "rusqlite",
    "serde",
    "HashMap",
    "HashSet",
    ".unwrap()",
    ".expect(",
    "panic!",
    "unreachable!",
    "todo!",
    "unimplemented!",
    "assert!(",
    "assert_eq!(",
    "debug_assert",
    "unwrap_or_default",
    "unwrap_unchecked",
    "get_unchecked",
    "mem::transmute",
    "println!",
    "eprintln!",
    "dbg!",
    ".ok()",
    "let _ =",
    "map_or(0",
    "unwrap_or(false)",
];

/// The library lines of `src/`: everything but comments.
fn library_lines() -> Vec<(String, usize, String)> {
    let mut out = Vec::new();
    for (name, text) in source_files() {
        // This crate keeps no unit tests in `src/`: the whole file is library code.
        assert!(
            !text.contains("#[cfg(test)]"),
            "{name}: tests live in tests/"
        );
        for (n, line) in text.lines().enumerate() {
            let t = line.trim_start();
            if t.starts_with("//") {
                continue;
            }
            out.push((name.clone(), n + 1, t.to_string()));
        }
    }
    out
}

#[test]
fn no_unwrap_in_non_test_code() {
    let lines = library_lines();
    for (name, n, t) in &lines {
        // A `const _: () = assert!(..)` is a compile-time check, not a runtime panic.
        if t.starts_with("const _: () = assert!(") {
            continue;
        }
        for banned in BANNED {
            assert!(
                !t.contains(banned),
                "src/{name}: `{banned}` in library code: {t} (line {n})"
            );
        }
    }
    assert!(
        lines.len() > 1_200,
        "the scan saw only {} lines: has the layout changed?",
        lines.len()
    );
}

/// The seam guard: no public function of the crate reads a clock or does I/O, and none takes or returns a
/// raw integer for a time or a size. Every function that needs a time takes the leader's `Stamp`; every
/// size is a `ByteCount`.
#[test]
fn public_api_exposes_no_clock_read_no_io() {
    let io_and_clock = [
        "SystemTime",
        "Instant",
        "std::time",
        "std::fs",
        "std::net",
        "std::io",
        "std::thread",
        "std::process",
        "std::env",
        "tokio",
        "rusqlite",
        "rand::",
        "getrandom",
    ];
    let raw_units = [
        "i64",
        "secs: u64",
        "seconds",
        "millis",
        "now: u64",
        "size: u64",
        "bytes: u64",
        "len: u64",
    ];
    let lines = library_lines();
    for (name, n, t) in &lines {
        for banned in io_and_clock {
            assert!(!t.contains(banned), "src/{name} line {n}: `{banned}`: {t}");
        }
        if t.starts_with("pub ") || t.starts_with("pub(crate) ") || t.contains("fn ") {
            for raw in raw_units {
                assert!(
                    !t.contains(raw),
                    "src/{name} line {n}: a raw unit `{raw}` in a signature: {t}"
                );
            }
        }
    }
    // The time of every operation that needs one is the caller's `Stamp`.
    let all: String = source_files().into_iter().map(|(_, text)| text).collect();
    assert!(
        all.contains("now: Stamp"),
        "the operations take the leader's stamp"
    );
    assert!(
        all.contains("at: Stamp"),
        "the directory is asked at a stamp"
    );
}

/// The doc block (the `///` lines) above line `i`, attributes skipped, joined by spaces.
fn doc_above(lines: &[&str], i: usize) -> String {
    let mut doc = Vec::new();
    let mut j = i;
    while j > 0 {
        j -= 1;
        let t = lines[j].trim();
        if let Some(rest) = t.strip_prefix("///") {
            doc.push(rest.trim().to_string());
        } else if t.starts_with("#[") || t.starts_with("#![") {
            continue;
        } else {
            break;
        }
    }
    doc.reverse();
    doc.join(" ")
}

/// A public item at the start of a line (`pub fn`, `pub struct`, `pub const`, ...). Methods inside `impl`
/// blocks are covered by their type.
fn is_public_item(line: &str) -> bool {
    line.starts_with("pub ") && !line.starts_with("pub use ") && !line.starts_with("pub mod ")
}

#[test]
fn public_items_cite_a_plan_section() {
    let mut checked = 0usize;
    for (name, text) in source_files() {
        let lines: Vec<&str> = text.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            if !is_public_item(line) {
                continue;
            }
            let doc = doc_above(&lines, i);
            assert!(
                doc.contains("planning/architecture.md section ")
                    || doc.contains("planning/architecture.md §"),
                "src/{name} line {}: `{}` does not cite a plan section (`planning/architecture.md section X.Y`)",
                i + 1,
                line.trim()
            );
            assert!(
                doc.contains("Caller:") || doc.contains("Seam:"),
                "src/{name} line {}: `{}` has no `Caller:` or `Seam:` doc line",
                i + 1,
                line.trim()
            );
            checked += 1;
        }
    }
    assert!(
        checked >= 50,
        "the scan found only {checked} public items: has the layout changed?"
    );
}

/// The numbers that follow the word `section`/`sections` in the comments of the source files: the plan
/// sections the code cites.
fn cited_sections() -> BTreeSet<String> {
    let mut cited = BTreeSet::new();
    for (_, text) in source_files() {
        let comments: String = text
            .lines()
            .filter_map(|l| {
                let t = l.trim_start();
                t.strip_prefix("//!")
                    .or_else(|| t.strip_prefix("///"))
                    .or_else(|| t.strip_prefix("//"))
            })
            .collect::<Vec<_>>()
            .join(" ");
        let tokens: Vec<&str> = comments.split_whitespace().collect();
        for (i, token) in tokens.iter().enumerate() {
            let word = token.trim_matches(|c: char| !c.is_ascii_alphabetic());
            if word != "section" && word != "sections" {
                continue;
            }
            // `section 5.9`, `sections 5.3, 5.10 and 7.2`.
            let mut k = i + 1;
            while let Some(next) = tokens.get(k) {
                // The number is the run of digits and dots that starts at the first digit.
                let from_digit = next.trim_start_matches(|c: char| !c.is_ascii_digit());
                let run: String = from_digit
                    .chars()
                    .take_while(|c| c.is_ascii_digit() || *c == '.')
                    .collect();
                let number = run.trim_end_matches('.');
                if number.is_empty() {
                    break;
                }
                cited.insert(number.to_string());
                // Continue only through a list ("5.3, 5.10 and 7.2").
                let more = next.ends_with(',') || tokens.get(k + 1) == Some(&"and");
                if !more {
                    break;
                }
                k += if tokens.get(k + 1) == Some(&"and") {
                    2
                } else {
                    1
                };
            }
        }
    }
    cited
}

/// The plan, by the rule R7: `AZLIN_PLAN_MD`; else (azul crate)
/// `CARGO_MANIFEST_DIR/../../../azul-apps/planning/architecture.md`.
fn plan_text() -> Option<String> {
    let from_env = std::env::var_os("AZLIN_PLAN_MD");
    let path = match &from_env {
        Some(path) => PathBuf::from(path),
        None => root().join("../../../azul-apps/planning/architecture.md"),
    };
    match fs::read_to_string(&path) {
        Ok(text) => Some(text),
        Err(error) => {
            // Absent: skipped when the variable is unset, a failure when it is set.
            assert!(
                from_env.is_none(),
                "AZLIN_PLAN_MD is set but {} cannot be read: {error}",
                path.display()
            );
            None
        }
    }
}

#[test]
fn every_cited_plan_section_exists_in_the_plan() {
    let cited = cited_sections();
    assert!(
        cited.len() >= 8,
        "the scan found only {cited:?}: has the comment style changed?"
    );
    let Some(plan) = plan_text() else {
        eprintln!("SKIPPED: plan not present");
        return;
    };
    let headings: Vec<&str> = plan
        .lines()
        .filter(|l| l.starts_with('#'))
        .map(|l| l.trim_start_matches('#').trim_start())
        .collect();
    for number in &cited {
        let found = headings.iter().any(|h| {
            h.strip_prefix(number.as_str())
                .is_some_and(|rest| rest.starts_with(' ') || rest.starts_with(". "))
        });
        assert!(
            found,
            "the code cites section {number}, which is no heading of the plan"
        );
    }
}

/// Each guard of the crate (`mutant!("id")`) with the test that must fail when it is turned off: the
/// mutation standard (QUALITY rule 12) kept as one table. The mutant hold runs every id with
/// `AZLIN_MUTANT=<id> cargo test -p azlin-spaces --features azlin-seams/mutants` and expects the red test.
const MUTANTS: &[(&str, &str)] = &[
    (
        "spaces_access_archived_writable",
        "an_archived_space_is_read_only_in_the_access_view",
    ),
    (
        "spaces_access_external_ignores_account",
        "an_external_member_with_an_internal_members_id_gets_no_internal_grant",
    ),
    (
        "spaces_access_groups_ignored",
        "effective_caps_union_direct_and_group_grants_through_nesting",
    ),
    (
        "spaces_access_ignores_revoked",
        "revoke_is_immediate_in_the_access_view",
    ),
    (
        "spaces_account_view_unchecked",
        "the_account_view_must_be_the_spaces_account",
    ),
    ("spaces_app_any_op", "app_principal_manager_may_rekey"),
    (
        "spaces_archived_unchecked",
        "an_archived_space_admits_nothing_and_applies_nothing",
    ),
    (
        "spaces_audit_space_unchecked",
        "the_audit_space_needs_an_admin_and_an_audit_level",
    ),
    (
        "spaces_capability_any_secret_accepted",
        "presented_secret_gets_the_role_and_a_wrong_secret_nothing",
    ),
    (
        "spaces_capability_manager_allowed",
        "a_capability_cannot_be_a_manager_and_is_paid_by_the_owner",
    ),
    (
        "spaces_capability_paid_by_grantee",
        "a_capability_cannot_be_a_manager_and_is_paid_by_the_owner",
    ),
    (
        "spaces_chain_unchecked",
        "space_chain_uses_azlin_chain_for_prev_seq_and_forks",
    ),
    (
        "spaces_charge_archived_unchecked",
        "a_write_by_a_principal_without_a_write_grant_or_into_an_archived_space_is_refused",
    ),
    (
        "spaces_charge_cap_off_by_one",
        "external_grantee_pays_its_own_quota_up_to_cap_bytes",
    ),
    ("spaces_charge_cap_unchecked", "charge_refuses_past_the_cap"),
    (
        "spaces_charge_payer_swapped",
        "owner_payer_charges_the_owner",
    ),
    (
        "spaces_charge_picks_least_room",
        "several_write_grants_use_the_one_with_the_most_room",
    ),
    (
        "spaces_cosig_unchecked",
        "admin_override_needs_the_admin_quorum",
    ),
    (
        "spaces_cosigner_role_unchecked",
        "admin_override_needs_the_admin_quorum",
    ),
    (
        "spaces_creation_role_unchecked",
        "member_creation_refused_when_it_does_not",
    ),
    (
        "spaces_device_active_unchecked",
        "suspended_or_removed_member_signs_nothing",
    ),
    (
        "spaces_epoch_any_key",
        "epoch_authorized_only_for_committed_key_ids",
    ),
    (
        "spaces_epoch_current_only",
        "old_epochs_stay_authorized_for_reading",
    ),
    (
        "spaces_epoch_planted_authorized",
        "planted_epoch_without_a_chain_entry_is_not_authorized",
    ),
    (
        "spaces_external_confused_with_member",
        "an_external_member_with_an_internal_members_id_gets_no_internal_grant",
    ),
    (
        "spaces_external_internal_unchecked",
        "an_external_grant_cannot_name_this_account_or_one_of_its_members",
    ),
    (
        "spaces_external_policy_unchecked",
        "external_grant_refused_when_policy_is_same_workspace_only",
    ),
    (
        "spaces_facts_device_unchecked",
        "the_facts_must_be_about_the_signing_device",
    ),
    (
        "spaces_first_manager_unchecked",
        "the_first_manager_must_be_an_active_member_of_the_account",
    ),
    (
        "spaces_forward_grant_no_rekey_due",
        "forward_space_grant_creates_a_rekey_due_obligation_and_a_rekey_clears_it",
    ),
    (
        "spaces_forward_style_grant_in_full",
        "full_space_grant_creates_none_and_the_joiner_gets_the_chain",
    ),
    (
        "spaces_grant_needs_no_manage",
        "grant_by_editor_viewer_uploader_refused",
    ),
    (
        "spaces_grant_wrap_missing",
        "grant_effect_names_the_wraps_a_new_member_needs",
    ),
    (
        "spaces_grantee_member_unchecked",
        "the_grantee_of_a_grant_must_exist",
    ),
    (
        "spaces_grants_limit_unchecked",
        "a_space_holds_at_most_max_grants_rows_and_max_capabilities_capabilities",
    ),
    (
        "spaces_group_unchecked",
        "the_grantee_of_a_grant_must_exist",
    ),
    (
        "spaces_hash_unchecked",
        "a_copy_whose_hash_is_not_its_preimages_is_corrupt",
    ),
    (
        "spaces_history_needs_no_manage",
        "grant_by_editor_viewer_uploader_refused",
    ),
    (
        "spaces_last_manager_check_off",
        "last_manager_cannot_be_revoked",
    ),
    (
        "spaces_leave_any_grantee",
        "revoke_by_the_grantee_is_leave_and_allowed",
    ),
    (
        "spaces_member_active_unchecked",
        "suspended_or_removed_member_signs_nothing",
    ),
    (
        "spaces_override_any_role",
        "admin_override_adds_managers_only_and_only_to_an_orphaned_space",
    ),
    (
        "spaces_override_for_external",
        "an_external_member_never_gets_the_admin_override",
    ),
    (
        "spaces_override_not_orphaned",
        "admin_override_adds_managers_only_and_only_to_an_orphaned_space",
    ),
    (
        "spaces_override_quorum_skipped",
        "admin_override_needs_the_admin_quorum",
    ),
    (
        "spaces_private_any_account",
        "private_member_space_only_for_family_accounts",
    ),
    (
        "spaces_rekey_admin_override",
        "an_admin_who_is_not_a_manager_cannot_rekey",
    ),
    (
        "spaces_rekey_chain_in_forward",
        "forward_space_grant_creates_a_rekey_due_obligation_and_a_rekey_clears_it",
    ),
    (
        "spaces_rekey_epoch_unchecked",
        "rekey_must_be_current_plus_one",
    ),
    (
        "spaces_rekey_keeps_due",
        "forward_space_grant_creates_a_rekey_due_obligation_and_a_rekey_clears_it",
    ),
    (
        "spaces_rekey_key_reuse_allowed",
        "rekey_key_id_reuse_refused",
    ),
    (
        "spaces_rekey_needs_no_manage",
        "grant_by_editor_viewer_uploader_refused",
    ),
    (
        "spaces_rekey_wraps_revoked",
        "a_rekey_wraps_only_the_remaining_readers_and_chains_in_full_mode",
    ),
    (
        "spaces_revoke_needs_no_authority",
        "revoke_by_the_grantee_is_leave_and_allowed",
    ),
    (
        "spaces_revoke_no_rekey_due",
        "a_removal_makes_a_rekey_due_in_either_mode",
    ),
    (
        "spaces_revoke_no_stop_serving",
        "revoke_effect_lists_the_wraps_to_stop_serving",
    ),
    ("spaces_root_anywhere", "root_kind_only_at_account_creation"),
    ("spaces_second_create_allowed", "second_create_refused"),
    (
        "spaces_signature_unchecked",
        "a_bad_unknown_or_unusable_signature_is_refused",
    ),
    (
        "spaces_signer_window_off_by_one",
        "is_authorized_signer_at_a_chain_position",
    ),
    (
        "spaces_signer_window_open_ended",
        "is_authorized_signer_at_a_chain_position",
    ),
    (
        "spaces_standing_ignores_groups",
        "a_manager_through_a_group_may_grant",
    ),
    (
        "spaces_standing_uses_revoked",
        "a_revoked_manager_signs_nothing",
    ),
    (
        "spaces_union_is_intersection",
        "union_of_viewer_and_uploader_reads_and_writes",
    ),
    ("spaces_uploader_reads", "uploader_cannot_read"),
    (
        "spaces_view_future_position_answered",
        "a_position_beyond_the_chain_is_unreadable_not_authorized",
    ),
    (
        "spaces_view_unreadable_as_none",
        "unreadable_chain_is_err_not_none",
    ),
    (
        "spaces_view_unreadable_as_unauthorized",
        "unreadable_chain_is_err_not_none",
    ),
    ("spaces_viewer_writes", "viewer_cannot_write"),
    (
        "spaces_visible_uses_current_mode",
        "history_switch_applies_to_later_joiners_only",
    ),
];

/// The text of every test source of the crate: `tests/*.rs` and `tests/common/mod.rs`.
fn test_sources() -> String {
    let mut out = String::new();
    let mut folders = vec![root().join("tests")];
    while let Some(folder) = folders.pop() {
        for entry in fs::read_dir(&folder).expect("tests folder") {
            let path = entry.expect("a directory entry").path();
            if path.is_dir() {
                folders.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push_str(&fs::read_to_string(&path).expect("test source"));
                out.push('\n');
            }
        }
    }
    out
}

/// The ids of the guards in `src/`, comments skipped.
fn guards_in_the_code() -> BTreeSet<String> {
    let mut in_code = BTreeSet::new();
    for (name, text) in source_files() {
        for line in text.lines() {
            let t = line.trim_start();
            if t.starts_with("//") {
                continue;
            }
            let mut rest = t;
            while let Some(at) = rest.find("mutant!(\"") {
                let after = &rest[at + "mutant!(\"".len()..];
                let end = after
                    .find('"')
                    .unwrap_or_else(|| panic!("src/{name}: an unterminated mutant id: {t}"));
                in_code.insert(after[..end].to_string());
                rest = &after[end..];
            }
        }
    }
    in_code
}

#[test]
fn mutant_guards_in_the_code_are_the_documented_set_and_each_has_a_killer() {
    let in_code = guards_in_the_code();
    let documented: BTreeSet<String> = MUTANTS.iter().map(|(id, _)| (*id).to_string()).collect();
    assert_eq!(documented.len(), MUTANTS.len(), "an id twice in the table");
    assert_eq!(
        in_code, documented,
        "the guards in src/ and the table of this test are one set"
    );
    // The list the crate publishes is the same set, in the same order.
    let published: Vec<String> = azlin_spaces::MUTANT_IDS
        .iter()
        .map(|id| (*id).to_string())
        .collect();
    let table: Vec<String> = MUTANTS.iter().map(|(id, _)| (*id).to_string()).collect();
    let mut sorted = table.clone();
    sorted.sort();
    assert_eq!(table, sorted, "the table is sorted by id");
    assert_eq!(published, table, "MUTANT_IDS is the table");
    let tests = test_sources();
    for (id, killer) in MUTANTS {
        assert!(
            id.starts_with("spaces_") && id.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'),
            "{id}: lower-case snake case under the prefix of the crate"
        );
        assert!(
            tests.contains(&format!("fn {killer}()")),
            "{id}: its killer `{killer}` is no test of this crate"
        );
    }
}

/// The names the brief's checks give (section 6): each is a `#[test]` of exactly that name somewhere in
/// `tests/`.
#[test]
fn every_named_check_of_the_brief_is_a_test() {
    let named = [
        // 1 chain
        "space_chain_uses_azlin_chain_for_prev_seq_and_forks",
        "space_create_commits_the_first_epoch_key_id",
        "second_create_refused",
        "root_kind_only_at_account_creation",
        // 2 who may sign
        "grant_by_a_manager_ok",
        "grant_by_editor_viewer_uploader_refused",
        "grant_by_an_account_admin_who_is_not_a_manager_refused_without_override",
        "admin_override_needs_the_admin_quorum",
        "revoke_by_the_grantee_is_leave_and_allowed",
        "last_manager_cannot_be_revoked",
        "suspended_or_removed_member_signs_nothing",
        "app_principal_manager_may_rekey",
        // 3 roles as capabilities
        "uploader_cannot_read",
        "viewer_cannot_write",
        "union_of_viewer_and_uploader_reads_and_writes",
        "effective_caps_union_direct_and_group_grants_through_nesting",
        // 4 external members and policy
        "external_grant_refused_when_policy_is_same_workspace_only",
        "external_grant_allowed_when_policy_allows",
        "external_grantee_pays_its_own_quota_up_to_cap_bytes",
        "charge_refuses_past_the_cap",
        "owner_payer_charges_the_owner",
        "external_member_cannot_grant_unless_manager",
        // 5 capability grants
        "capability_is_stored_only_as_a_hash",
        "presented_secret_gets_the_role_and_a_wrong_secret_nothing",
        "capability_comparison_is_constant_time_in_the_hash",
        "revoking_a_capability_stops_it_at_once",
        "capability_never_appears_in_effects_or_debug_output",
        // 6 epochs
        "planted_epoch_without_a_chain_entry_is_not_authorized",
        "rekey_must_be_current_plus_one",
        "rekey_by_a_removed_manager_refused",
        "rekey_key_id_reuse_refused",
        "old_epochs_stay_authorized_for_reading",
        "epoch_authorized_only_for_committed_key_ids",
        // 7 history
        "forward_space_grant_creates_a_rekey_due_obligation_and_a_rekey_clears_it",
        "full_space_grant_creates_none_and_the_joiner_gets_the_chain",
        "chain_visible_to_follows_history_mode_at_the_joiners_seq",
        "history_switch_applies_to_later_joiners_only",
        // 8 creation by members
        "member_creates_a_conversation_when_the_account_view_allows",
        "member_creation_refused_when_it_does_not",
        "private_member_space_only_for_family_accounts",
        // 9 the access view
        "revoke_is_immediate_in_the_access_view",
        "access_view_equals_a_naive_model_over_5000_seeded_random_chains",
        "replay_equals_incremental_state_hash",
        // 10 effects
        "grant_effect_names_the_wraps_a_new_member_needs",
        "rekey_effect_emits_key_epoch_notice",
        "revoke_effect_lists_the_wraps_to_stop_serving",
        // 11 planner P3's seam
        "space_chains_implement_azlin_keys_space_chain_view",
        "commitment_names_key_id_signer_seq_and_kind",
        "unreadable_chain_is_err_not_none",
        "is_authorized_signer_at_a_chain_position",
        // 12 golden
        "space_chain_entries_match_azlin_contract_fixture",
    ];
    let tests = test_sources();
    for name in named {
        assert!(
            tests.contains(&format!("fn {name}()")),
            "the brief's check `{name}` is no test of this crate"
        );
    }
    // The matrix has its own cells counted in its test.
    assert!(tests.contains("fn who_may_sign_matrix_covers_op_by_role_by_relation()"));
}

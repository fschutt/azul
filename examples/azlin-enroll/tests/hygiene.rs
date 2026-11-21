//! The crate's hygiene, the part a test can hold (the launch rules of the W1 lanes; check 10 of the
//! brief): the manifest lists exactly the dependencies of the final manifest (and none of the
//! forbidden ones), the crate root forbids unsafe code and denies the panicking constructs, no
//! function of the library reads a clock, the environment, a file, a network, a thread or a random
//! source, the library code names no panicking call and no lenient default, every public item names
//! the plan section it implements and its caller or the later unit it is a seam of, every source
//! file is declared, every plan section a comment cites exists in the plan (COMMON-W1R R7), the
//! witness types cannot be copied or made out of nothing, every test the brief names exists, and the
//! mutation guards of the source are exactly the ids of `MUTANT_IDS`, each with a killer that
//! exists.
//!
//! `cargo clippy --all-targets -- -D warnings`, `rustfmt --check` and the build for wasm32 are the
//! lane's holds: cargo cannot run inside `cargo test`.
//!
//! Implements planning/architecture.md section 1.4 (engineering rules: no panics on input, I/O is a
//! parameter, every public item has a caller) and section 7.5 (the guards of the holds).

mod common;

use std::collections::BTreeSet;
use std::fs;

use azlin_enroll::MUTANT_IDS;
use common::{plan_text, root};

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

/// The source files of `src/`, by file name (flat: this crate has no module folder).
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

/// The test files of `tests/` (and `tests/common/mod.rs`), by path relative to `tests/`.
fn test_files() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for entry in fs::read_dir(root().join("tests")).expect("tests folder") {
        let entry = entry.expect("a directory entry");
        let name = entry.file_name().to_string_lossy().to_string();
        if name.ends_with(".rs") {
            let text = fs::read_to_string(entry.path()).expect("test file");
            out.push((name, text));
        }
    }
    let common = fs::read_to_string(root().join("tests/common/mod.rs")).expect("common/mod.rs");
    out.push(("common/mod.rs".to_string(), common));
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
    let expected: BTreeSet<String> = ["azlin-seams", "azlin-policy", "blake3", "thiserror"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(dependencies, expected, "the dependencies of the crate");
    for forbidden in [
        "serde",
        "serde_json",
        "ciborium",
        "azlin-cbor",
        "azlin-teamlog",
        "azlin-keys",
        "azlin-chain",
        "tokio",
        "reqwest",
        "rand",
        "proptest",
        "chrono",
        "time",
    ] {
        assert!(
            !dependencies.contains(forbidden),
            "{forbidden} is not a dependency of this crate"
        );
    }
    for section in ["[dev-dependencies]", "[build-dependencies]", "[features]"] {
        assert!(
            section_keys(&manifest, section).is_empty(),
            "{section} is empty"
        );
    }
    assert!(!root().join("build.rs").exists());

    // Package facts.
    assert_eq!(
        section_value(&manifest, "[package]", "name").as_deref(),
        Some("\"azlin-enroll\"")
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
        "path",
        "request",
        "evidence",
        "hold",
        "cancel",
        "escrow_open",
        "phrase",
        "limits",
        "notices",
        "release",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    assert_eq!(declared, expected, "the modules of the brief (section 3)");
}

/// The tokens that do not belong in library code (QUALITY rules 4 and 5, launch rule 8): panics,
/// lenient defaults, unsafe code, and every way of reading the outside world.
const BANNED: &[&str] = &[
    "unsafe {",
    "unsafe fn",
    "unsafe impl",
    "unsafe trait",
    "std::fs",
    "std::net",
    "std::thread",
    "std::process",
    "std::io",
    "rand::",
    "getrandom",
    "serde",
    ".unwrap()",
    ".expect(",
    "panic!",
    "unreachable!",
    "todo!",
    "unimplemented!",
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

/// The tokens that read a clock or the environment: the crate takes the leader's `Stamp` as a
/// parameter and reads no clock of its own.
const CLOCK_AND_ENVIRONMENT: &[&str] = &[
    "std::time",
    "SystemTime",
    "Instant",
    "Clock",
    "now()",
    ".elapsed(",
    "std::env",
    "env::var",
    "env!(",
    "option_env!",
    "var_os",
    "current_dir",
    "args()",
];

/// The code lines of a source file: the lines that are not comments.
fn code_lines(text: &str) -> impl Iterator<Item = (usize, &str)> {
    text.lines()
        .enumerate()
        .map(|(n, line)| (n + 1, line.trim_start()))
        .filter(|(_, line)| !line.starts_with("//"))
}

#[test]
fn no_unwrap_in_non_test_code() {
    let mut scanned = 0usize;
    for (name, text) in source_files() {
        // This crate keeps no unit tests in `src/`: the whole file is library code.
        assert!(
            !text.contains("#[cfg(test)]"),
            "{name}: tests live in tests/"
        );
        for (n, line) in code_lines(&text) {
            scanned += 1;
            for banned in BANNED {
                assert!(
                    !line.contains(banned),
                    "src/{name}: `{banned}` in library code: {line} (line {n})"
                );
            }
        }
    }
    assert!(
        scanned > 800,
        "the scan saw only {scanned} lines: has the layout changed?"
    );
}

/// No function of the library reads a clock or the environment (the scan that the hold script's
/// `rg` repeats over the whole source).
#[test]
fn no_function_of_this_crate_reads_a_clock_or_the_environment() {
    let mut scanned = 0usize;
    for (name, text) in source_files() {
        for (n, line) in code_lines(&text) {
            scanned += 1;
            for banned in CLOCK_AND_ENVIRONMENT {
                assert!(
                    !line.contains(banned),
                    "src/{name}: `{banned}` in library code: {line} (line {n})"
                );
            }
        }
    }
    assert!(
        scanned > 800,
        "the scan saw only {scanned} lines: has the layout changed?"
    );
    // The one switch of the mutation guards is read inside azlin-seams, not here: the library
    // names the macro and nothing of the environment.
    let all: String = source_files().into_iter().map(|(_, text)| text).collect();
    assert!(all.contains("azlin_seams::mutant!"));
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

/// A public item at the start of a line (`pub fn`, `pub struct`, `pub const`, ...). Methods inside
/// `impl` blocks are covered by their type.
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
        checked >= 60,
        "the scan found only {checked} public items: has the layout changed?"
    );
}

/// The numbers that follow the word `section`/`sections` in the comments of the source files: the
/// plan sections the code cites.
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

#[test]
fn every_cited_plan_section_exists_in_the_plan() {
    let cited = cited_sections();
    assert!(
        cited.len() >= 8,
        "the scan found only {cited:?}: has the comment style changed?"
    );
    for expected in [
        "1.4", "5.3", "5.5", "5.9", "5.10", "5.11", "7.4", "7.5", "7.6", "12", "14",
    ] {
        assert!(
            cited.contains(expected),
            "the code does not cite section {expected}: {cited:?}"
        );
    }
    let Some(plan) = plan_text() else {
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

/// The witnesses of this crate cannot be copied, defaulted or made from another value: their
/// fields are private, they derive no `Clone`, `Copy` or `Default`, and nothing implements those
/// or `From` for them. (`Transcript` holds a `Secret`: it is not `Clone` either.)
#[test]
fn witness_types_have_no_clone_default_or_from() {
    for (file, name) in [
        ("release.rs", "ReleaseGrant"),
        ("release.rs", "Served"),
        ("phrase.rs", "JoinConfirmation"),
        ("phrase.rs", "Transcript"),
    ] {
        let text = fs::read_to_string(root().join("src").join(file)).expect("source file");
        let lines: Vec<&str> = text.lines().collect();
        let at = lines
            .iter()
            .position(|l| l.starts_with(&format!("pub struct {name}")))
            .unwrap_or_else(|| panic!("{file}: no `pub struct {name}`"));
        // The attributes right above the struct.
        let mut derives = String::new();
        let mut j = at;
        while j > 0 {
            j -= 1;
            let t = lines[j].trim();
            if t.starts_with("#[") {
                derives.push_str(t);
            } else if t.starts_with("///") {
                continue;
            } else {
                break;
            }
        }
        for banned in ["Clone", "Copy", "Default"] {
            assert!(
                !derives.contains(banned),
                "{name} derives {banned}: {derives}"
            );
        }
        // No private-looking field is public: the fields are private (only `Transcript`'s are
        // the caller's own inputs and public).
        if name != "Transcript" {
            let body: Vec<&str> = lines[at + 1..]
                .iter()
                .copied()
                .take_while(|l| !l.starts_with('}'))
                .collect();
            for field in body {
                assert!(
                    !field.trim_start().starts_with("pub "),
                    "{name}: a public field: {field}"
                );
            }
        }
        for line in &lines {
            let t = line.trim();
            if !t.starts_with("impl") {
                continue;
            }
            for trait_name in ["Clone", "Default", "From<"] {
                assert!(
                    !(t.contains(trait_name) && t.contains(&format!("for {name}"))),
                    "{name}: `{t}` makes a copy or a value out of another"
                );
            }
        }
    }
}

/// The ids of the guards in the source: the string of every `mutant!("...")`.
fn guards_in_source() -> Vec<String> {
    let mut ids = Vec::new();
    for (_, text) in source_files() {
        for (_, line) in code_lines(&text) {
            let mut rest = line;
            while let Some(at) = rest.find("azlin_seams::mutant!(\"") {
                let after = &rest[at + "azlin_seams::mutant!(\"".len()..];
                match after.find('"') {
                    Some(end) => {
                        ids.push(after[..end].to_string());
                        rest = &after[end..];
                    }
                    None => break,
                }
            }
        }
    }
    ids
}

#[test]
fn every_mutation_guard_of_the_source_is_listed_and_every_listed_id_is_a_guard() {
    let in_source = guards_in_source();
    let sites: BTreeSet<String> = in_source.iter().cloned().collect();
    assert_eq!(
        sites.len(),
        in_source.len(),
        "each guard has exactly one site in the source: {in_source:?}"
    );
    let listed: BTreeSet<String> = MUTANT_IDS.iter().map(|id| id.to_string()).collect();
    assert_eq!(
        sites, listed,
        "the guards of the source are the ids of MUTANT_IDS"
    );
    assert_eq!(listed.len(), MUTANT_IDS.len(), "the ids are unique");
    assert_eq!(MUTANT_IDS.len(), 6, "the six guards of the brief");
    for id in MUTANT_IDS {
        assert!(id.starts_with("enroll_"), "{id}");
        assert!(
            id.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'),
            "{id}"
        );
    }
    // Every guard has a killer test that exists (the table in the doc of `MUTANT_IDS`).
    let killers = [
        (
            "enroll_hold_skipped_on_recovery",
            "holds",
            "hold_withholds_release_before_hold_until_recovery_code",
        ),
        (
            "enroll_cancel_by_helpdesk",
            "veto",
            "helpdesk_cannot_cancel_or_approve_alone",
        ),
        (
            "enroll_sso_subject_unchecked",
            "evidence",
            "sso_subject_must_match_the_person",
        ),
        (
            "enroll_agent_counts_as_person",
            "escrow",
            "key_agent_share_never_stands_in_for_a_person",
        ),
        (
            "enroll_disputed_stamp_released",
            "clock",
            "disputed_stamp_never_releases",
        ),
        (
            "enroll_pending_device_may_cancel",
            "veto",
            "a_pending_device_cannot_cancel_the_request_it_belongs_to_or_anyone_elses",
        ),
    ];
    let named: BTreeSet<String> = killers.iter().map(|(id, _, _)| id.to_string()).collect();
    assert_eq!(named, listed, "every guard has a killer in this table");
    let lib = fs::read_to_string(root().join("src/lib.rs")).expect("src/lib.rs");
    for (id, file, test) in killers {
        let text =
            fs::read_to_string(root().join(format!("tests/{file}.rs"))).expect("a test file");
        assert!(
            has_test(&text, test),
            "the killer of {id}: tests/{file}.rs has no test {test}"
        );
        // And the doc table of MUTANT_IDS names it too.
        assert!(
            lib.contains(&format!("`{id}`")) && lib.contains(&format!("`{test}`")),
            "{id} {test}"
        );
    }
}

/// Whether the text defines the test `name`: a function, or a name handed to a macro that makes it.
fn has_test(text: &str, name: &str) -> bool {
    text.contains(&format!("fn {name}()")) || text.contains(&format!("{name},"))
}

#[test]
fn every_test_the_brief_names_exists() {
    let names = [
        // 1. holds
        "hold_withholds_release_before_hold_until_quorum_enrollment",
        "hold_withholds_release_before_hold_until_recovery_code",
        "hold_withholds_release_before_hold_until_contact_shares",
        "hold_withholds_release_before_hold_until_passkey_wrap",
        "hold_withholds_release_before_hold_until_escrow_recovery",
        "hold_withholds_release_before_hold_until_escrow_open",
        "release_one_second_before_is_withheld_and_at_hold_until_is_granted",
        "own_device_path_has_no_hold",
        "hold_length_is_the_policys_enroll_hold",
        "hold_is_fixed_at_open_and_a_later_policy_change_does_not_shorten_it",
        "escrow_open_waits_72h_for_an_active_subject",
        "escrow_open_has_no_wait_for_a_subject_offboarded_by_quorum",
        // 2. veto
        "any_admin_can_cancel",
        "any_active_device_of_the_person_can_cancel",
        "helpdesk_cannot_cancel_or_approve_alone",
        "a_pending_device_cannot_cancel_the_request_it_belongs_to_or_anyone_elses",
        "a_suspended_admin_cannot_cancel",
        "a_cancelled_request_never_releases",
        "cancel_after_release_is_refused",
        // 3. evidence
        "recovery_escrow_needs_fresh_sso_but_sso_alone_never_releases",
        "sso_subject_must_match_the_person",
        "stale_sso_attestation_refused",
        "mdm_certificate_is_device_evidence_tpm_attestation_is_not",
        "contact_shares_need_two_of_three",
        "passkey_path_refused_when_policy_passkey_recovery_is_off",
        "passkey_path_open_for_personal_and_family_by_default",
        "quorum_enrollment_refused_when_device_approval_is_self_only",
        "max_devices_per_member_refuses_the_extra_device",
        // 4. escrow
        "opening_needs_two_different_people",
        "asker_cannot_contribute_to_own_opening",
        "key_agent_share_never_stands_in_for_a_person",
        "key_agent_alone_never_opens",
        "lost_laptop_needs_the_agent_plus_one_admin_holder_other_than_the_asker",
        "key_agent_contributes_only_when_idp_token_mdm_and_policy_checks_all_pass",
        "single_admin_workspace_owner_kit_path_has_the_full_wait_and_notices",
        "private_member_has_no_escrow_path",
        // 5. clock
        "disputed_stamp_never_releases",
        "a_stamp_a_year_ahead_without_agreement_never_releases",
        "a_stamp_going_backwards_neither_extends_nor_cancels",
        // 6. notices and visibility
        "open_notices_every_admin_and_every_device_of_the_person",
        "a_lost_notice_changes_no_state_and_the_request_is_listed_by_pending_for_person",
        "cancel_notices_the_same_audience",
        // 7. limits
        "second_open_while_one_is_pending_refused",
        "daily_limit_per_person",
        "limits_are_per_person_not_per_device",
        // 8. phrase
        "phrase_is_the_same_on_both_devices_known_answer",
        "phrase_changes_when_any_input_bit_changes",
        "a_wrong_phrase_refuses_the_wrap",
        "invite_join_needs_the_inviters_confirmation_of_the_phrase",
        // 9. the property
        "release_is_never_before_hold_until_never_after_cancel_and_only_with_complete_evidence",
        // 11. golden
        "enrollment_request_matches_azlin_contract_fixture",
        // the rules of the second revision (the standing, the binding, the budgets, the life)
        "a_quorum_offboarding_is_two_admins_in_a_workspace_with_at_least_two",
        "a_request_lives_seven_days_after_its_hold_and_an_own_device_request_fifteen_minutes",
        "an_own_device_request_nobody_finished_expires_after_fifteen_minutes",
        "a_disputed_stamp_opens_nothing_and_adds_nothing",
        "a_request_releases_on_its_last_second_and_expires_on_the_next",
        "a_lower_kind_of_opener_never_uses_up_the_budget_of_a_higher_one",
        "the_daily_budget_of_a_kind_of_opener_is_its_own_too",
        "two_requests_of_one_device_in_one_second_are_two_requests_with_two_ids",
        "the_ledger_is_bounded_and_frees_itself_when_it_holds_only_the_idle",
        "two_kinds_of_opener_make_two_requests",
        "a_sole_admin_asks_alone_for_an_escrow_opening_and_for_nothing_else",
        "the_functions_that_take_facts_refuse_the_facts_of_another_person",
        "a_veto_is_not_refused_for_a_mistake_in_the_lists_of_the_facts",
        "the_key_agents_checks_are_for_this_person_and_this_device",
        "an_admin_or_the_person_is_no_key_agent",
        "a_share_of_an_admin_needs_an_active_admin",
        "a_member_listed_as_a_person_and_as_the_agent_counts_for_nobody",
        "the_helpdesk_member_who_opened_a_recovery_holds_no_share_of_it",
        "evidence_made_for_another_device_is_refused",
        "a_closed_request_takes_no_evidence",
        "the_release_notice_goes_to_the_audience_of_the_moment",
        "a_revoked_approving_device_does_not_finish_an_own_device_request",
        "admins_who_signed_and_were_suspended_do_not_finish_a_quorum_enrollment",
        "the_admin_who_confirmed_the_phrase_must_still_be_an_admin",
        "an_escrow_holder_who_is_no_admin_any_more_gives_no_share",
        "the_single_admin_exception_holds_only_while_the_workspace_has_one_admin",
        "the_single_admin_exception_is_for_a_request_opened_under_it",
        "a_grant_is_stale_after_the_life_of_the_request",
        "a_disputed_stamp_takes_no_grant",
        "an_asker_who_is_no_admin_any_more_is_no_person_when_the_key_goes_out",
        "recoveries_by_the_person_depend_on_no_admin_and_no_device",
    ];
    let files = test_files();
    for name in names {
        let found = files.iter().any(|(_, text)| has_test(text, name));
        assert!(found, "no test of tests/ is named {name}");
    }
    // The golden test is ignored until S5 lands the fixture, and says so.
    let golden = files
        .iter()
        .find(|(file, _)| file == "golden.rs")
        .map(|(_, text)| text.as_str())
        .expect("tests/golden.rs");
    assert!(golden.contains("#[ignore = \"S5 fixture not landed\"]"));
}

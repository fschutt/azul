//! The per-file problem list: its codes are the harness's, a problem stays until resolved,
//! names are shown escaped.

use azul_storage::{testing::TempDir, DriveError, ServiceError};

use crate::{
    error::CloudError,
    sync::{
        problems::{
            code_of_error, escaped, from_report, harness_json, Code, Problem, ProblemList,
            Severity,
        },
        MassDelete, SyncReport,
    },
};

#[test]
fn the_codes_are_written_as_the_sync_harness_spells_them() {
    assert_eq!(Code::SymlinkSkipped.as_str(), "symlink-skipped");
    assert_eq!(Code::LoopFrozen.as_str(), "loop-frozen");
    assert_eq!(Code::RemoteIndexUnreadable.as_str(), "remote-index-unreadable");
    assert_eq!(Code::DeletedElsewhereKept.as_str(), "deleted-elsewhere-kept");
    assert_eq!(Code::SignInNeeded.as_str(), "sign-in-needed");
    assert_eq!(Code::Transient.as_str(), "transient");
}

#[test]
fn a_problem_stays_listed_with_its_first_time_until_a_pass_no_longer_finds_it() {
    let mut list = ProblemList::default();
    list.publish(
        vec![
            Problem::new(Code::SymlinkSkipped, "a/link", "a link"),
            Problem::new(Code::Transient, "big.bin", "503"),
        ],
        true,
        100,
    );
    list.publish(
        vec![Problem::new(Code::SymlinkSkipped, "a/link", "a link")],
        true,
        200,
    );
    assert_eq!(list.problems.len(), 1, "big.bin went up: it is no problem any more");
    assert_eq!(list.problems[0].since, 100, "the link was listed first at 100");
    assert_eq!(list.updated_at, 200);
}

#[test]
fn a_pass_that_never_reached_the_files_keeps_their_problems() {
    let mut list = ProblemList::default();
    list.publish(
        vec![Problem::new(Code::SymlinkSkipped, "a/link", "a link")],
        true,
        100,
    );
    list.publish(
        vec![Problem::new(Code::RemoteOffline, "", "the drive cannot be reached")],
        false,
        200,
    );
    let codes: Vec<Code> = list.problems.iter().map(|p| p.code).collect();
    assert!(codes.contains(&Code::SymlinkSkipped), "{codes:?}");
    assert!(codes.contains(&Code::RemoteOffline), "{codes:?}");
    // The next full pass replaces the pass-wide problem.
    list.publish(
        vec![Problem::new(Code::SymlinkSkipped, "a/link", "a link")],
        true,
        300,
    );
    assert_eq!(list.problems.len(), 1);
}

#[test]
fn warnings_alone_leave_the_folder_up_to_date_and_errors_do_not() {
    let mut list = ProblemList::default();
    list.publish(vec![Problem::new(Code::Portability, "con.txt", "Windows")], true, 1);
    assert!(list.up_to_date());
    list.publish(vec![Problem::new(Code::LoopFrozen, "flap.txt", "loop")], true, 2);
    assert!(!list.up_to_date());
    assert_eq!(list.problems[0].severity, Severity::Error);
}

#[test]
fn a_reversed_extension_is_shown_with_its_code_point() {
    assert_eq!(escaped("invoice\u{202E}fdp.exe"), "invoice<U+202E>fdp.exe");
    assert_eq!(escaped("a\u{200B}b\u{7}c"), "a<U+200B>b<U+0007>c");
    assert_eq!(escaped("plain/name.txt"), "plain/name.txt");
}

#[test]
fn a_failure_is_classed_by_what_it_is_not_by_its_words() {
    let service = |status: u16| {
        CloudError::Drive(DriveError::Service(ServiceError {
            status,
            code: String::from("Error"),
            ..ServiceError::default()
        }))
    };
    assert_eq!(code_of_error(&service(429)), Code::RateLimited);
    assert_eq!(code_of_error(&service(503)), Code::Transient);
    assert_eq!(code_of_error(&service(507)), Code::QuotaExceeded);
    assert_eq!(
        code_of_error(&CloudError::Drive(DriveError::Corrupt {
            key: String::from("k"),
            reason: String::from("bad tag"),
        })),
        Code::HashMismatch
    );
    assert_eq!(
        code_of_error(&CloudError::Io(String::from("No space left on device (os error 28)"))),
        Code::DiskFull
    );
    assert_eq!(
        code_of_error(&service(503).context("upload big/stuck.bin")),
        Code::Transient,
        "the context in front changes nothing"
    );
}

#[test]
fn a_report_lists_its_structured_problems_first_and_reads_its_sentences_for_the_rest() {
    let report = SyncReport {
        skipped: vec![
            (String::from("ln"), String::from("a symbolic link: not followed")),
            (String::from("dup"), String::from("a symbolic link")),
        ],
        changed_during_sync: vec![String::from(
            "notes/a: b.txt: changed during the run; the next run takes it",
        )],
        mass_delete: Some(MassDelete {
            count: 12,
            of: 14,
            here: false,
            keys: Vec::new(),
        }),
        problems: vec![Problem::new(Code::ThroughLink, "dup", "under a link")],
        ..SyncReport::default()
    };
    let problems = from_report(&report);
    let find = |key: &str| problems.iter().filter(|p| p.key == key).map(|p| p.code).collect::<Vec<_>>();
    assert_eq!(find("ln"), vec![Code::SymlinkSkipped]);
    assert_eq!(find("dup"), vec![Code::ThroughLink], "the structured one wins");
    assert_eq!(find("notes/a: b.txt"), vec![Code::ChangedDuringSync]);
    assert_eq!(find(""), vec![Code::MassDeleteHeld]);
}

#[test]
fn the_list_survives_in_the_state_folder_and_the_harness_reads_escaped_keys() {
    let dir = TempDir::new("problems");
    let mut list = ProblemList::default();
    list.publish(
        vec![Problem::new(Code::DisplayRisk, "a\u{202E}txt.exe", "bidi")],
        true,
        5,
    );
    list.save(dir.path()).unwrap();
    let again = ProblemList::load(dir.path()).unwrap();
    assert_eq!(again, list);
    let json = harness_json(&again);
    assert_eq!(json[0]["code"], "display-risk");
    assert_eq!(json[0]["key"], "a<U+202E>txt.exe");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(dir.path().join("problems.json"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0, "the list names files: owner only");
    }
}

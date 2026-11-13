//! The per-file problem list: its codes are the harness's, a problem stays until resolved,
//! names are shown escaped.

use azul_storage::{testing::TempDir, DriveError, ServiceError};

use crate::{
    error::CloudError,
    sync::{
        problems::{
            code_of_error, code_of_sentence, escaped, from_report, harness_json, Code, Problem,
            ProblemList, Severity, LISTED_PER_CODE,
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

/// A device without the drive key (`joined: false` in AzDrive's scenarios, a forgotten
/// passphrase) failed its pass as `other`: the list said nothing the user could act on. It is
/// `sign-in-needed` (join again, or unlock with the recovery code), from the error or from its
/// sentence alone, also as the whole pass's problem.
#[test]
fn a_drive_this_device_holds_no_key_for_is_sign_in_needed() {
    let locked = CloudError::Drive(DriveError::Locked {
        drive: String::from("e2e-remote-r"),
    });
    assert_eq!(code_of_error(&locked), Code::SignInNeeded);
    assert_eq!(
        code_of_error(&CloudError::Drive(DriveError::Locked {
            drive: String::from("d"),
        })
        .context("list the drive")),
        Code::SignInNeeded
    );
    assert_eq!(code_of_sentence(&locked.to_string()), Code::SignInNeeded);
    assert_eq!(
        crate::sync::problems::of_failed_pass(&locked).code,
        Code::SignInNeeded
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

#[test]
fn not_backed_up_since_is_the_oldest_problem_that_keeps_a_change_back() {
    let mut list = ProblemList::default();
    assert_eq!(list.unsynced_since(), None);
    list.publish(
        vec![
            Problem::new(Code::Portability, "a:b.txt", "Windows cannot hold it"),
            Problem::new(Code::Transient, "work/day01.psd", "the service failed"),
        ],
        true,
        1_000,
    );
    list.publish(
        vec![
            Problem::new(Code::Portability, "a:b.txt", "Windows cannot hold it"),
            Problem::new(Code::Transient, "work/day01.psd", "the service failed"),
            Problem::new(Code::RemoteOffline, "", "offline"),
        ],
        false,
        5_000,
    );
    assert_eq!(list.unsynced_since(), Some(1_000), "kept since the first pass that listed it");
    list.publish(vec![Problem::new(Code::Settling, "x", "being written")], true, 9_000);
    assert_eq!(list.unsynced_since(), None, "a passing moment keeps nothing back");
}

#[test]
fn a_file_whose_content_the_drive_lost_is_an_unreadable_entry() {
    assert_eq!(
        code_of_sentence("the blob .azlin/blobs/ab/abcd is missing from the drive"),
        Code::RemoteEntryUnreadable
    );
}

/// X-DELTA-AZDRIVE-FILES-33: a check this device went on without (the disk would not say its room, so
/// the reserve was not enforced) is a problem of the pass - a warning that holds nothing back - and
/// no longer a note nothing shows.
#[test]
fn x_delta_azdrive_files_33_an_unchecked_check_is_a_warning_of_the_pass() {
    assert_eq!(Code::Unchecked.as_str(), "unchecked");
    assert_eq!(Code::Unchecked.severity(), Severity::Warning);
    assert!(!Code::Unchecked.holds_back());
    assert!(Code::Unchecked.is_pass_wide());
}

/// X-DELTA-EDGE-3 (E15a, T11). `ProblemList::publish` made the list one problem at a time and asked
/// the list so far whether the problem was in it already: a pass with 100,000 refused names
/// (symlinks pointing out, names past 4,096 bytes, a hostile index) took minutes and wrote a
/// `problems.json` of tens of megabytes. 200,000 problems and the same 200,000 told twice: one row
/// per (code, key), within two seconds, and a file the pass can write.
#[test]
fn publishing_two_hundred_thousand_problems_takes_linear_time() {
    const N: usize = 200_000;
    let pass: Vec<Problem> = (0..N)
        .chain(0..N)
        .map(|i| {
            Problem::new(
                Code::BadName,
                &format!("deep/{i:06}"),
                "not synced: a path longer than 4096 bytes",
            )
        })
        .collect();
    // On a thread of its own: the quadratic list takes minutes, and the test says so after the
    // limit instead of waiting for it.
    let (sent, done) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let started = std::time::Instant::now();
        let mut list = ProblemList::default();
        list.publish(pass, true, 100);
        let _ = sent.send((list, started.elapsed()));
    });
    let (list, took) = done
        .recv_timeout(std::time::Duration::from_secs(2))
        .expect("publishing 400,000 problems takes more than two seconds: the dedupe is a scan of the growing list");
    assert!(took < std::time::Duration::from_secs(2), "{took:?}");
    let mut seen = std::collections::HashSet::new();
    for p in &list.problems {
        assert!(
            seen.insert((p.code, p.key.clone())),
            "{:?} {:?} is listed twice",
            p.code,
            p.key
        );
    }
    let bytes = serde_json::to_vec(&list).expect("the list serialises");
    assert!(
        bytes.len() < 1 << 20,
        "problems.json is {} bytes: a list that long is written every pass",
        bytes.len()
    );
    // What is not listed is counted, not lost: the rows and the row that says "and N more" are the
    // 200,000 distinct problems, the 200,000 repeats told for nothing.
    let listed = list.problems.iter().filter(|p| p.more == 0).count();
    assert_eq!(listed, LISTED_PER_CODE, "the first thousand by key");
    let summary: Vec<&Problem> = list.problems.iter().filter(|p| p.more > 0).collect();
    assert_eq!(summary.len(), 1, "one row says the rest: {summary:?}");
    assert_eq!(summary[0].code, Code::BadName);
    assert_eq!(summary[0].key, "", "it names no file");
    assert_eq!(listed as u64 + summary[0].more, N as u64);
    assert_eq!(list.total(), N as u64);
    assert_eq!(summary[0].since, 100);
    assert!(
        !list.up_to_date(),
        "a row of errors left out is still errors"
    );
}

/// X-DELTA-EDGE-3 (E15a), the other writer of the list: a check outside a pass (the read-back, a
/// free-up, a disk too full) adds its problems one at a time to the list on disk, and each was a scan
/// of the list so far and a write of the whole file - a read-back that found a hundred thousand
/// damaged files wrote the list a hundred thousand times, each time longer. A problem told twice is
/// one row, the first thousand of a code are rows, and the rest are counted in one row that says so.
#[test]
fn problems_a_check_notes_between_passes_are_capped_like_a_pass_s() {
    let damaged = |i: usize| {
        Problem::new(
            Code::HashMismatch,
            &format!("big/{i:04}.bin"),
            "its bytes rotted",
        )
    };
    let mut list = ProblemList::default();
    for i in 0..1_200 {
        list.note(damaged(i), 500);
    }
    // A file told again that has its row is no second row.
    for i in 0..10 {
        list.note(damaged(i), 600);
    }
    list.note(
        Problem::new(Code::DiskReserve, "other.bin", "the disk is full"),
        700,
    );
    let rows = |code: Code| {
        list.problems
            .iter()
            .filter(|p| p.code == code && p.more == 0)
            .count()
    };
    assert_eq!(rows(Code::HashMismatch), LISTED_PER_CODE);
    assert_eq!(rows(Code::DiskReserve), 1, "another code has its own room");
    let more: Vec<&Problem> = list.problems.iter().filter(|p| p.more > 0).collect();
    assert_eq!(more.len(), 1, "{more:?}");
    assert_eq!(
        (more[0].code, more[0].more, more[0].key.as_str()),
        (Code::HashMismatch, 200, "")
    );
    assert_eq!(more[0].since, 500, "since the first one past the cap");
    assert_eq!(list.total(), 1_201);
}

/// The same through the function a check calls (`evict::note_problem`: the list is read from the state
/// folder, the problem noted, the list written): a list that holds a thousand rows of a code takes the
/// next problem of it into its "and N more" row, and the file is what the next look reads.
#[test]
fn evict_note_problem_counts_a_problem_past_the_cap_in_the_row_that_says_so() {
    use crate::sync::evict::note_problem;
    let dir = TempDir::new("problems-noted");
    let mut list = ProblemList::default();
    for i in 0..LISTED_PER_CODE {
        list.note(
            Problem::new(Code::HashMismatch, &format!("big/{i:04}.bin"), "rotted"),
            500,
        );
    }
    list.save(dir.path()).unwrap();
    for i in 0..3 {
        note_problem(
            dir.path(),
            Problem::new(Code::HashMismatch, &format!("more/{i}.bin"), "rotted"),
        );
    }
    let again = ProblemList::load(dir.path()).unwrap();
    assert_eq!(again.problems.len(), LISTED_PER_CODE + 1);
    assert_eq!(again.total(), LISTED_PER_CODE as u64 + 3);
    assert!(again
        .problems
        .iter()
        .any(|p| p.more == 3 && p.key.is_empty()));
}

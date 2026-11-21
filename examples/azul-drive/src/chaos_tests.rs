//! AzDrive's sync status under hostile conditions (the chaos E2E, scripts/azdrive_chaos_e2e.py):
//! a failed pass said in the errors table's words, the files that cannot sync counted, a full
//! disk named, and a drive this device was removed from no longer polled.

use std::path::Path;

use azcloud_kit::sync::session::{FileRecord, SyncProblem, SyncSetup, SyncStates};
use azul_appkit::l10n::Arg;

use crate::{sync_jobs, sync_view};

fn setup() -> SyncSetup {
    SyncSetup::new("d_photos", "", Path::new("/home/me/AzDrive/Photos"))
}

fn problem(code: &str) -> SyncProblem {
    SyncProblem {
        code: code.to_string(),
        request_id: Some(String::from("n1-9")),
        detail: String::from("the service's own words"),
    }
}

fn status_key(states: &SyncStates) -> String {
    sync_view::status_text(&setup(), states, None, true, false, false).key
}

#[test]
fn a_failed_pass_is_said_in_the_errors_tables_words() {
    let mut states = SyncStates {
        last_pass: Some(1),
        last_error: Some(String::from(
            "the storage service answered Forbidden (HTTP 403, x-azlin-error credentials_revoked)",
        )),
        last_problem: Some(problem("credentials_revoked")),
        ..SyncStates::default()
    };
    assert_eq!(status_key(&states), "azlin-error-credentials-revoked");
    states.last_problem = Some(problem("network"));
    assert_eq!(status_key(&states), "azlin-error-network");
    states.last_problem = Some(problem("quota_exceeded"));
    assert_eq!(status_key(&states), "azlin-error-quota-exceeded");
    // An error of AzDrive's own (the folder, a local disk) keeps its words.
    states.last_problem = None;
    assert_eq!(status_key(&states), "azdrive-sync-status-failed");
}

#[test]
fn up_to_date_counts_the_files_that_cannot_sync_and_names_a_full_disk() {
    let mut states = SyncStates {
        last_pass: Some(1),
        ..SyncStates::default()
    };
    states.skipped.insert(
        String::from("link.txt"),
        String::from("a symbolic link (never followed)"),
    );
    states.files.insert(
        String::from("Notes.txt"),
        FileRecord {
            error: Some(String::from("\"notes.txt\" here has the same name to this disk")),
            ..FileRecord::default()
        },
    );
    let said = sync_view::status_text(&setup(), &states, None, true, false, false);
    assert_eq!(said.key, "azdrive-sync-status-skipped");
    assert_eq!(said.get("count"), Some(&Arg::Int(2)));
    states.files.insert(
        String::from("big.bin"),
        FileRecord {
            error: Some(String::from(
                "write big.bin: file error: No space left on device (os error 28)",
            )),
            ..FileRecord::default()
        },
    );
    assert_eq!(status_key(&states), "azdrive-sync-status-disk-full");
}

#[test]
fn a_drive_this_device_was_removed_from_is_not_polled_until_asked() {
    let mut states = SyncStates::default();
    assert!(sync_jobs::polled(&states));
    states.last_problem = Some(problem("network"));
    assert!(sync_jobs::polled(&states), "offline: the next poll tries again");
    states.last_problem = Some(problem("quota_exceeded"));
    assert!(sync_jobs::polled(&states), "a full drive still brings changes down");
    states.last_problem = Some(problem("credentials_revoked"));
    assert!(!sync_jobs::polled(&states));
    states.last_problem = Some(problem("sign_in"));
    assert!(!sync_jobs::polled(&states));
}

#[test]
fn every_new_status_has_its_words_in_english_and_german() {
    let resources = [
        ("en", include_str!("../resources/en.ftl")),
        ("de", include_str!("../resources/de.ftl")),
    ];
    for key in ["azdrive-sync-status-skipped", "azdrive-sync-status-disk-full"] {
        for (lang, text) in resources {
            assert!(
                text.lines().any(|line| line.starts_with(&format!("{key} ="))),
                "{lang}.ftl lacks {key}"
            );
        }
    }
}

#[test]
fn the_node_lists_live_in_the_runs_cache_folder() {
    // A run with --cache-dir (an E2E, a second profile) keeps its drives' node lists there,
    // never in the user's own cache folder.
    let root = std::env::temp_dir().join("azdrive-chaos-tests-nodes");
    crate::set_nodes_root(root.join("nodes"));
    let kept = crate::nodes_file("d_test");
    assert!(kept.starts_with(&root), "{}", kept.display());
}

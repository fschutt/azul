//! What a hostile or broken server, network or disk does to a pass, as the engine names it: a
//! server that moved, a drive gone read-only, an object over the server's size cap, a file made
//! immutable, a folder that cannot be listed for a while, a crash inside a download's body, and
//! attributes that are not synced.

use azul_storage::{AzlinError, DriveError, ServiceError};

use crate::{
    error::CloudError,
    sync::problems::{code_of_error, of_failed_pass, Code, Severity},
};

fn service(status: u16, code: &str, azlin: Option<AzlinError>) -> CloudError {
    CloudError::Drive(DriveError::Service(ServiceError {
        status,
        code: code.to_string(),
        message: String::from("the service said no"),
        azlin_error: azlin,
        ..ServiceError::default()
    }))
}

#[test]
fn the_new_codes_are_written_as_the_sync_harness_spells_them() {
    assert_eq!(Code::ServerMoved.as_str(), "server-moved");
    assert_eq!(Code::ReadOnly.as_str(), "read-only");
    assert_eq!(Code::XattrsNotSynced.as_str(), "xattrs-not-synced");
    assert_eq!(Code::XattrsNotSynced.severity(), Severity::Warning);
    assert_eq!(Code::ServerMoved.severity(), Severity::Error);
    assert!(Code::ServerMoved.is_pass_wide());
    assert!(!Code::XattrsNotSynced.is_pass_wide());
}

#[test]
fn a_moved_server_is_its_own_problem_and_not_a_sign_in() {
    let moved = service(301, "PermanentRedirect", None);
    assert_eq!(code_of_error(&moved), Code::ServerMoved);
    assert_eq!(of_failed_pass(&moved).code, Code::ServerMoved);
    assert_eq!(code_of_error(&service(308, "PermanentRedirect", None)), Code::ServerMoved);
}

#[test]
fn a_read_only_drive_says_so_instead_of_asking_to_sign_in_again() {
    for azlin in [
        AzlinError::ReadOnlyUnpaid,
        AzlinError::ReadOnlyLockdown,
        AzlinError::ReadOnlyOperator,
        AzlinError::DriveBanned,
    ] {
        let e = service(403, "AccessDenied", Some(azlin));
        assert_eq!(code_of_error(&e), Code::ReadOnly, "{e}");
    }
    // Revoked keys still ask to sign in.
    assert_eq!(code_of_error(&service(403, "AccessDenied", None)), Code::SignInNeeded);
}

#[test]
fn an_object_over_the_servers_cap_is_too_large_and_a_full_drive_is_quota() {
    assert_eq!(code_of_error(&service(413, "EntityTooLarge", None)), Code::TooLarge);
    assert_eq!(
        code_of_error(&service(400, "EntityTooLarge", Some(AzlinError::ObjectTooLarge))),
        Code::TooLarge
    );
    assert_eq!(
        code_of_error(&service(403, "QuotaExceeded", Some(AzlinError::QuotaExceeded))),
        Code::QuotaExceeded
    );
}

#[test]
fn an_immutable_file_or_folder_is_locked_not_an_io_error() {
    let e = CloudError::Io(String::from("rename a.txt: Operation not permitted (os error 1)"));
    assert_eq!(code_of_error(&e), Code::Locked);
    let e = CloudError::Io(String::from("Permission denied (os error 13)"));
    assert_eq!(code_of_error(&e), Code::Unreadable);
}

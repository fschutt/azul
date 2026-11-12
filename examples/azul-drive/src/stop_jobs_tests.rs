//! X-DELTA-AZDRIVE-KEYS-32: a removal tells what runs for the drive to stop before it takes the
//! drive's credentials and files away. A job that goes on holds the credentials and writes what
//! the removal took (an index update that kept its folder, a transfer that kept its drive).

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use azul_storage::{cap::Cap, meta::MemoryBucket, testing::TempDir, Drive, DriveError};

use crate::{
    archive::{run_rescue, Stops},
    fileops::TransferKind,
    find::IndexInfo,
    raise_cancels_of, TransferJob,
};

fn job(source: &str, target: &str) -> TransferJob {
    let bucket: Arc<dyn Drive> = Arc::new(MemoryBucket::new());
    TransferJob {
        kind: TransferKind::Copy,
        source_id: source.to_string(),
        target_id: target.to_string(),
        source: Arc::clone(&bucket),
        target: bucket,
        items: Vec::new(),
        target_prefix: String::new(),
        plan: None,
        cancel: Arc::new(AtomicBool::new(false)),
        auto: None,
        action: 0,
    }
}

#[test]
fn a_removal_stops_the_index_update_and_the_transfers_to_and_from_the_drive_and_no_others() {
    let mut transfers = HashMap::new();
    transfers.insert(1, job("d_1", "d_2"));
    transfers.insert(2, job("d_3", "d_1"));
    transfers.insert(3, job("d_2", "d_3"));
    let mut indexes = HashMap::new();
    indexes.insert(String::from("d_1"), IndexInfo::default());
    indexes.insert(String::from("d_2"), IndexInfo::default());

    assert_eq!(raise_cancels_of(&indexes, &transfers, "d_1"), 3);
    let raised = |job: &TransferJob| job.cancel.load(Ordering::SeqCst);
    assert!(raised(&transfers[&1]), "a transfer from the drive");
    assert!(raised(&transfers[&2]), "a transfer to the drive");
    assert!(!raised(&transfers[&3]), "another drives' transfer runs on");
    assert!(
        indexes["d_1"].cancel.load(Ordering::SeqCst),
        "its index update"
    );
    assert!(
        !indexes["d_2"].cancel.load(Ordering::SeqCst),
        "another drive's"
    );
    // a drive with nothing running has nothing to stop
    assert_eq!(raise_cancels_of(&indexes, &transfers, "d_9"), 0);
}

/// The copy of a locked drive to this computer reads through a drive that stops answering once the
/// removal raised its flag: nothing more is asked of a drive that is gone, and a run that is told
/// at its start copies nothing and says so.
#[test]
fn a_rescue_copy_reads_through_a_drive_that_stops_when_the_drive_is_removed() {
    let inner: Arc<dyn Drive> = Arc::new(MemoryBucket::new());
    inner.put("docs/a.txt", b"alpha").unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let drive = Stops {
        inner,
        stop: Arc::clone(&stop),
    };
    assert_eq!(drive.get("docs/a.txt").unwrap(), b"alpha");
    assert_eq!(drive.head("docs/a.txt").unwrap().size, 5);

    stop.store(true, Ordering::SeqCst);
    assert_eq!(drive.get("docs/a.txt"), Err(DriveError::Cancelled));
    assert_eq!(drive.head("docs/a.txt").unwrap_err(), DriveError::Cancelled);
    assert!(matches!(
        drive.get_range("docs/a.txt", azul_storage::ByteRange::new(0, Some(1))),
        Err(DriveError::Cancelled)
    ));
    assert!(matches!(
        drive.list(&azul_storage::ListRequest::recursive("")),
        Err(DriveError::Cancelled)
    ));

    let dir = TempDir::new("stop-rescue");
    let folder = dir.path().join("copy");
    let report = run_rescue(
        &drive,
        &folder,
        Cap::Unknown("no probe"),
        None,
        &mut |_, _| {},
    );
    assert_eq!(report.copied, 0);
    assert!(!folder.join("docs").exists(), "nothing was copied");
}

/// X-DELTA-STRUCT-DRIVE-3: `Stops` wrote seven methods and left twenty to the trait's defaults, so its
/// `site()` was a place of its own, `local_path` was none and `metadata` was the default's - the FILES-2
/// shape again, and the census could not see it (it was classified as an adapter that "forwards no more").
/// It is the drive below for every call that is no request - the place, the path, the conditional
/// vocabulary - and a request asks the flag first and is answered by the drive below.
#[test]
fn x_delta_struct_drive_3_stops_is_the_drive_below_for_the_place_and_the_path_and_asks_the_flag_for_a_request(
) {
    let dir = TempDir::new("stops-forwards");
    let folder: Arc<dyn Drive> = Arc::new(azul_storage::LocalDrive::new(dir.path().to_path_buf()));
    folder.put("a.txt", b"alpha").unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let drive = Stops {
        inner: Arc::clone(&folder),
        stop: Arc::clone(&stop),
    };
    // The place and the path are the folder's: the overlap guards and the fast path of a copy rest on them.
    assert_eq!(drive.site(), folder.site());
    assert!(matches!(
        drive.site(),
        azul_storage::xfer::Site::Disk { .. }
    ));
    assert_eq!(drive.local_path("a.txt"), folder.local_path("a.txt"));
    assert!(drive.local_path("a.txt").is_some());
    assert_eq!(drive.conditional_writes(), folder.conditional_writes());
    // A request is the drive below's answer while the flag is down, and a refusal when it is raised -
    // `metadata` among them, which the old `Stops` left to the trait's default.
    assert_eq!(drive.get("a.txt").unwrap(), b"alpha");
    assert_eq!(drive.metadata("a.txt"), folder.metadata("a.txt"));
    stop.store(true, Ordering::SeqCst);
    assert_eq!(drive.get("a.txt"), Err(DriveError::Cancelled));
    assert_eq!(drive.metadata("a.txt"), Err(DriveError::Cancelled));
    assert_eq!(
        drive.site(),
        folder.site(),
        "a question about the drive waits for no flag"
    );
}

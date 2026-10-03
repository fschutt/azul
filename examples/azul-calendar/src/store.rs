//! Every durable write of AzCalendar - events, calendars, the settings, the To-Do bar's tasks,
//! exports - goes through the azul-storage `Drive` (a `LocalDrive` on the data folder today,
//! the user's `S3Drive` later), on an azul `Thread`, never from a callback (DEDUP_EDITORS B2).
//!
//! A callback queues the write in one of two write-behind queues (`azul_pim::write_queue`,
//! AzTasks' queue: one write per key, one batch in flight, failures kept for a retry): the
//! calendar's data folder and the task store's folder (the same folder when a data folder was
//! named). A short timer hands each queue's batch to a file thread
//! (`azul_appkit::ui::spawn_file_jobs`, the jobs below); the outcomes come back to the UI
//! thread, which finishes the batch. The main window does not close while a write waits.

use azul_appkit::files::{FileJob, FileOutcome};
use azul_pim::write_queue::Write;

/// The reply tag of a batch written into the calendar's data folder.
pub const TAG_DATA: u64 = 1;
/// The reply tag of a batch written into the task store.
pub const TAG_TASKS: u64 = 2;
/// The reply tag of an export file's write.
pub const TAG_EXPORT: u64 = 3;

/// The folder an export goes to, in the data folder (the data tree is what a sync sees).
pub const EXPORTS_DIR: &str = "exports";

/// What a close request does to the main window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainClose {
    /// Nothing waits (or the user was told what did not land): the window closes.
    Close,
    /// Writes wait or are on their way: the close is held, and the window closes once they
    /// landed.
    Wait,
    /// Writes did not land: the close is held once, and the user is told.
    Tell,
}

/// What a close request does to the main window: `waiting` writes are queued or on their way,
/// `failures` did not land, and the user was `told` about them already.
#[must_use]
pub fn main_close(waiting: bool, failures: usize, told: bool) -> MainClose {
    if waiting {
        MainClose::Wait
    } else if failures > 0 && !told {
        MainClose::Tell
    } else {
        MainClose::Close
    }
}

/// The file jobs that write `batch`, in order.
#[must_use]
pub fn jobs_of(batch: &[Write]) -> Vec<FileJob> {
    batch
        .iter()
        .map(|write| match write {
            Write::Put { key, bytes } => FileJob::Put {
                key: key.clone(),
                bytes: bytes.clone(),
            },
            Write::Delete { key } => FileJob::Delete { key: key.clone() },
        })
        .collect()
}

/// The writes of `batch` that `outcomes` say did not land, each with why. A write no outcome
/// answers for did not land either.
#[must_use]
pub fn failures_of(batch: &[Write], outcomes: &[FileOutcome]) -> Vec<(Write, String)> {
    batch
        .iter()
        .filter_map(|write| {
            let answer = outcomes.iter().find(|o| match o {
                FileOutcome::Put { key, .. } | FileOutcome::Deleted { key, .. } => {
                    key == write.key()
                }
                FileOutcome::Got { .. } | FileOutcome::GotAll { .. } => false,
            });
            match answer {
                Some(outcome) => outcome.error().map(|why| (write.clone(), why)),
                None => Some((
                    write.clone(),
                    String::from("the file thread did not answer for it"),
                )),
            }
        })
        .collect()
}

/// The key an export named `file_name` (`Work.ics`) is written to: its file name alone, in
/// [`EXPORTS_DIR`].
#[must_use]
pub fn export_key(file_name: &str) -> String {
    let name = file_name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .trim();
    let name = if name.is_empty() || name == "." || name == ".." {
        "AzCalendar.ics"
    } else {
        name
    };
    format!("{EXPORTS_DIR}/{name}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(key: &str, bytes: &str) -> Write {
        Write::Put {
            key: key.to_string(),
            bytes: bytes.as_bytes().to_vec(),
        }
    }

    #[test]
    fn a_batch_is_written_as_file_jobs_in_its_order() {
        let batch = vec![
            put("events/a.json", "{}"),
            Write::Delete {
                key: "events/b.json".to_string(),
            },
            put("settings.txt", "view=week\n"),
        ];
        assert_eq!(
            jobs_of(&batch),
            vec![
                FileJob::Put {
                    key: "events/a.json".to_string(),
                    bytes: b"{}".to_vec()
                },
                FileJob::Delete {
                    key: "events/b.json".to_string()
                },
                FileJob::Put {
                    key: "settings.txt".to_string(),
                    bytes: b"view=week\n".to_vec()
                },
            ]
        );
    }

    #[test]
    fn a_write_that_did_not_land_comes_back_for_a_retry_with_why() {
        let batch = vec![
            put("events/a.json", "{}"),
            Write::Delete {
                key: "events/b.json".to_string(),
            },
        ];
        let outcomes = vec![
            FileOutcome::Put {
                key: "events/a.json".to_string(),
                result: Err("the disk is full".to_string()),
            },
            FileOutcome::Deleted {
                key: "events/b.json".to_string(),
                result: Ok(()),
            },
        ];
        assert_eq!(
            failures_of(&batch, &outcomes),
            vec![(put("events/a.json", "{}"), "the disk is full".to_string())]
        );
        // A batch whose thread never answered for a write: that write did not land either.
        let short = vec![FileOutcome::Put {
            key: "events/a.json".to_string(),
            result: Ok(()),
        }];
        let failed = failures_of(&batch, &short);
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].0.key(), "events/b.json");
    }

    /// Closing the calendar loses no change: it waits for the writes on their way, and a write
    /// that did not land is said once before the window goes.
    #[test]
    fn the_main_window_closes_once_every_write_landed_or_the_user_was_told() {
        assert_eq!(main_close(false, 0, false), MainClose::Close);
        assert_eq!(main_close(true, 0, false), MainClose::Wait);
        assert_eq!(main_close(true, 2, true), MainClose::Wait, "new writes wait too");
        assert_eq!(main_close(false, 1, false), MainClose::Tell);
        assert_eq!(main_close(false, 1, true), MainClose::Close);
    }

    #[test]
    fn an_export_goes_into_the_exports_folder_of_the_data_tree() {
        assert_eq!(export_key("Work.ics"), "exports/Work.ics");
        // Only a file name: no folders of its own, no way out of the tree.
        assert_eq!(export_key("../../etc/Work.ics"), "exports/Work.ics");
        assert_eq!(export_key("C:\\Users\\me\\Work.ics"), "exports/Work.ics");
        assert_eq!(export_key(""), "exports/AzCalendar.ics");
    }
}

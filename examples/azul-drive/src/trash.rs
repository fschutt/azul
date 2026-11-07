//! The trash of a drive on this computer, and what becomes of it (AZ-U16, Q18).
//!
//! A delete on a local drive moves into `<root>/.azdrive-trash/<stamp>/<key>` ([`crate::fileops`],
//! Ctrl+Z brings it back), and nothing ever took it out again: deleting 100 GB to free space
//! freed nothing, ever. The trash is kept [`KEPT_DAYS`] days now (30, the sync trash's window)
//! and what is older goes - but never on the strength of the wall clock alone (rule 11): the
//! clock of a computer is set wrong, runs ahead after a dead battery, jumps with a time-zone
//! tool, and a purge that believed it would empty the whole trash of a user whose clock is a year
//! ahead. A delete goes only when ALL of these hold:
//!
//! - its stamp (the delete's own time, in its name) AND the folder's modification time (the file
//!   system's record) both say it is [`KEPT_DAYS`] old - a name altered, or a folder touched
//!   since, keeps it;
//! - an earlier pass found it so [`GRACE_DAYS`] days ago and wrote that down ([`Marks`]): a
//!   clock that is wrong for a moment marks, it purges nothing; it would have to stay wrong, and
//!   forward, for a week across two runs;
//! - the clock has not gone back since the last pass (`seen`, which moves forward by at most
//!   [`SEEN_STEP_DAYS`] a pass): a pass that finds it behind judges nothing.
//!
//! What it cannot read is kept: a stamp that is no stamp, a folder the file system cannot date.
//! The marks are a cache (`<cache>/trash/<drive>.json`): losing them delays a purge by a
//! week, it never brings one forward. The value that removes a delete is [`Expired`], made by
//! [`plan`] and nowhere else.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use azul::prelude::*;
use azul_storage::{
    config::DriveLocation,
    docs::{Doc, DocRead, Document},
    ops, Drive,
};
use serde::{Deserialize, Serialize};

use crate::{fileops::TRASH_FOLDER, jobs::Job, DriveState};

/// How long a delete stays in the trash.
pub(crate) const KEPT_DAYS: u64 = 30;

/// How long after a pass found a delete expired another pass may remove it.
pub(crate) const GRACE_DAYS: u64 = 7;

/// The most a pass moves the `seen` time forward: a clock that jumped a year ahead holds the
/// next passes back for this long once it is put right, not for a year.
pub(crate) const SEEN_STEP_DAYS: u64 = 60;

const DAY: u64 = 86_400;

/// What the passes wrote down about a drive's trash.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Marks {
    /// The latest time (seconds since 1970) a pass looked: a clock behind it has gone back.
    #[serde(default)]
    pub seen: u64,
    /// For each stamp, when a pass first found the delete expired.
    #[serde(default)]
    pub expired: BTreeMap<String, u64>,
    /// For each stamp in the trash, the time a delete's age counts from: its stamp - unless it
    /// appeared after a pass that is later than its stamp, which only a delete made by a clock that
    /// was behind can do; then the pass that first saw it (X-DELTA-AZDRIVE-FILES-34). An RTC reset
    /// to 2000 made a delete "old" the moment the clock was put right, and it was removed a week
    /// later instead of after its 30 days.
    #[serde(default)]
    pub first_seen: BTreeMap<String, u64>,
}

impl Document for Marks {}

/// One delete in the trash, as the two records of its age say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Delete {
    /// The folder's name below the trash: `20261001-031500-1`.
    pub stamp: String,
    /// The time the stamp names; `None`: a name that is no stamp.
    pub stamped: Option<u64>,
    /// When the file system says the folder last changed; `None`: it could not be told.
    pub modified: Option<u64>,
}

/// A delete that may go: the witness [`purge`] removes on. Made by [`plan`], which made it from
/// two ages and a mark a week old; it cannot be cloned or read from a file.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Expired {
    stamp: String,
}

impl Expired {
    /// The trash folder of the delete: `.azdrive-trash/<stamp>/`.
    pub(crate) fn folder(&self) -> String {
        format!("{TRASH_FOLDER}{}/", self.stamp)
    }
}

/// What a look at the trash decided.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Pass {
    /// The marks to write.
    pub marks: Marks,
    /// The deletes to remove.
    pub expired: Vec<Expired>,
    /// The clock was behind the last pass: nothing was judged.
    pub behind: bool,
}

/// The time a trash stamp (`20261001-031500-1`, [`crate::fileops::trash_stamp`]) names.
pub(crate) fn stamp_time(stamp: &str) -> Option<u64> {
    let (time, serial) = stamp.rsplit_once('-')?;
    serial.parse::<u32>().ok()?;
    let naive = chrono::NaiveDateTime::parse_from_str(time, "%Y%m%d-%H%M%S").ok()?;
    u64::try_from(naive.and_utc().timestamp()).ok()
}

/// Whether `then` is at least [`KEPT_DAYS`] days before `now`; a time in the future, or none, is
/// not old.
fn old(then: Option<u64>, now: u64) -> bool {
    then.is_some_and(|then| then <= now && now - then >= KEPT_DAYS * DAY)
}

/// The file that keeps what the trash purge has seen of the drive `drive_id`:
/// `<cache>/trash/<name>.json`; `None` for an id that names no file. The one place the name is
/// spelled; Remove drive asks it (`footprint.rs`).
pub(crate) fn marks_file(cache: &Path, drive_id: &str) -> Option<PathBuf> {
    let name = azul_storage::key::safe_file_name(drive_id)?;
    Some(cache.join("trash").join(format!("{name}.json")))
}

/// The start's look at the trash of every drive on this computer but the data tree's: one job
/// each for the drives that have a trash. Without a cache folder there is nowhere to write the
/// marks, and nothing is removed.
pub(crate) fn start(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    let now = crate::actions::now_secs();
    s.trash_looked = now;
    let Some(cache) = s.cache_dir.clone() else {
        return;
    };
    // Every local drive, the Azlin data tree among them: its deletes go to the same trash and
    // nothing else ever took them out (X-DELTA-AZDRIVE-FILES-34).
    let drives: Vec<(String, PathBuf)> = s
        .slots
        .iter()
        .filter_map(|slot| match &slot.entry.location {
            DriveLocation::Local { root } => Some((slot.entry.id.clone(), PathBuf::from(root))),
            _ => None,
        })
        .collect();
    for (drive_id, root) in drives {
        if !root.join(TRASH_FOLDER.trim_end_matches('/')).is_dir() {
            continue;
        }
        let Some(marks_file) = marks_file(&cache, &drive_id) else {
            continue;
        };
        crate::spawn(
            info,
            app,
            s,
            Job::PurgeTrash {
                drive_id,
                root,
                marks_file,
                now,
            },
        );
    }
}

/// The timer's look: the trash is looked at again once a day - an app that stays open for weeks
/// removed nothing, the look was the start's alone (X-DELTA-AZDRIVE-FILES-34).
pub(crate) fn look_if_due(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    if look_is_due(s.trash_looked, crate::actions::now_secs()) {
        start(info, app, s);
    }
}

/// Whether a look at `now` is due, the last having been at `looked` (0: none yet).
pub(crate) fn look_is_due(looked: u64, now: u64) -> bool {
    now >= looked.saturating_add(DAY)
}

/// What the passes so far and the clock now decide about `deletes`: mark the ones both records
/// call old, remove the ones marked [`GRACE_DAYS`] ago, unmark the ones that are not old, and
/// judge nothing when the clock is behind a pass that was.
pub(crate) fn plan(deletes: &[Delete], before: &Marks, now: u64) -> Pass {
    if now < before.seen {
        return Pass {
            marks: before.clone(),
            expired: Vec::new(),
            behind: true,
        };
    }
    // A first pass trusts the clock it has; later ones move `seen` on by a bounded step.
    let seen = if before.seen == 0 {
        now
    } else {
        now.min(before.seen.saturating_add(SEEN_STEP_DAYS * DAY))
    };
    let mut marks = Marks {
        seen,
        expired: BTreeMap::new(),
        first_seen: BTreeMap::new(),
    };
    let mut expired = Vec::new();
    for delete in deletes {
        // What the delete's age counts from: what an earlier pass wrote down; else its stamp -
        // unless an earlier pass is later than the stamp and did not know it (made by a clock that
        // was behind): then now.
        let counted_from = match before.first_seen.get(&delete.stamp) {
            Some(&then) if then <= now => then,
            _ => match delete.stamped {
                Some(stamped) if before.seen == 0 || stamped >= before.seen => stamped,
                _ => now,
            },
        };
        marks.first_seen.insert(delete.stamp.clone(), counted_from);
        if !(old(delete.stamped, now) && old(delete.modified, now) && old(Some(counted_from), now))
        {
            continue;
        }
        match before.expired.get(&delete.stamp) {
            Some(&found) if found <= now && now - found >= GRACE_DAYS * DAY => {
                // Kept in the marks until it is gone: a removal that fails is tried again.
                marks.expired.insert(delete.stamp.clone(), found);
                expired.push(Expired {
                    stamp: delete.stamp.clone(),
                });
            }
            Some(&found) if found <= now => {
                marks.expired.insert(delete.stamp.clone(), found);
            }
            // Not marked yet, or marked in the future of this clock: marked now.
            _ => {
                marks.expired.insert(delete.stamp.clone(), now);
            }
        }
    }
    Pass {
        marks,
        expired,
        behind: false,
    }
}

/// What a pass did.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Purged {
    /// Deletes removed.
    pub removed: usize,
    /// Deletes found expired for the first time (or still waiting out the grace).
    pub marked: usize,
    /// The clock was behind the last pass.
    pub behind: bool,
    /// The deletes that could not be removed, and why.
    pub failed: Vec<(String, String)>,
}

/// A look at the trash of the local drive `drive` rooted at `root` at time `now`, with the marks
/// in `marks_file`: marks first, then removes what [`plan`] let through, then forgets the marks of
/// what is gone. A drive without a trash costs one stat.
///
/// # Errors
///
/// The sentence for a trash that could not be listed, or marks of another format (left as they
/// are; nothing is removed then).
pub(crate) fn purge(
    drive: &dyn Drive,
    root: &Path,
    marks_file: &Path,
    now: u64,
) -> Result<Purged, String> {
    let trash = root.join(TRASH_FOLDER.trim_end_matches('/'));
    if !trash.is_dir() {
        return Ok(Purged::default());
    }
    let listed = ops::list_folder_all(drive, TRASH_FOLDER).map_err(|e| e.to_string())?;
    let deletes: Vec<Delete> = listed
        .folders
        .iter()
        .filter_map(|folder| {
            let stamp = azul_storage::key::last_segment(folder.trim_end_matches('/'));
            (!stamp.is_empty()).then(|| Delete {
                stamp: stamp.to_string(),
                stamped: stamp_time(stamp),
                modified: std::fs::metadata(trash.join(stamp))
                    .and_then(|meta| meta.modified())
                    .ok()
                    .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|since| since.as_secs()),
            })
        })
        .collect();
    let doc = Doc::<Marks>::new(marks_file);
    let before = match doc.read() {
        DocRead::Found(marks) => marks,
        DocRead::Absent | DocRead::Damaged(_) => Marks::default(),
        DocRead::Foreign(why) => return Err(why),
    };
    let pass = plan(&deletes, &before, now);
    let mut out = Purged {
        marked: pass.marks.expired.len().saturating_sub(pass.expired.len()),
        behind: pass.behind,
        ..Purged::default()
    };
    if pass.behind {
        return Ok(out);
    }
    // The intent first: a later pass removes only what an earlier one wrote down.
    doc.update(|marks| *marks = pass.marks.clone())
        .map_err(|e| e.to_string())?;
    let mut gone = Vec::new();
    for expired in &pass.expired {
        match drive.delete_folder(&expired.folder()) {
            Ok(()) => {
                out.removed += 1;
                gone.push(expired.stamp.clone());
            }
            Err(e) => out.failed.push((expired.stamp.clone(), e.to_string())),
        }
    }
    if !gone.is_empty() {
        doc.update(|marks| {
            for stamp in &gone {
                marks.expired.remove(stamp);
            }
        })
        .map_err(|e| e.to_string())?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use azul_storage::{testing::TempDir, LocalDrive};

    use super::*;
    use crate::fileops::{trash_key, trash_stamp};

    const NOW: u64 = 1_800_000_000;

    fn delete(stamp: &str, age_days: Option<u64>, folder_days: Option<u64>) -> Delete {
        Delete {
            stamp: stamp.to_string(),
            stamped: age_days.map(|d| NOW - d * DAY),
            modified: folder_days.map(|d| NOW - d * DAY),
        }
    }

    /// AZ-U16: a stamp reads back to the time it was made from; what is no stamp reads as none.
    #[test]
    fn az_u16_a_stamp_reads_back_to_the_time_it_was_made_from() {
        for (secs, serial) in [(1_759_300_000, 1), (1_800_000_000, 12), (86_400, 0)] {
            assert_eq!(stamp_time(&trash_stamp(secs, serial)), Some(secs));
        }
        for bad in [
            "",
            "x",
            "20261001-031500",
            "20261001-031500-x",
            "00000000-000000-1",
            "2026-10-01-1",
        ] {
            assert_eq!(stamp_time(bad), None, "{bad:?}");
        }
    }

    /// AZ-U16 / rule 11: the first look at an old delete only marks it; a second look a week later
    /// removes it; a delete that is young by either record is never touched and loses its mark.
    #[test]
    fn az_u16_an_old_delete_is_marked_first_and_removed_a_week_later() {
        let deletes = [
            delete("old", Some(40), Some(40)),
            delete("young", Some(5), Some(5)),
            delete("touched", Some(40), Some(2)),
            delete("renamed", Some(2), Some(40)),
        ];
        let first = plan(&deletes, &Marks::default(), NOW);
        assert!(first.expired.is_empty(), "nothing goes on the first look");
        assert_eq!(first.marks.seen, NOW);
        assert_eq!(first.marks.expired.keys().collect::<Vec<_>>(), vec!["old"]);

        // Three days later: still waiting.
        let early = plan(&deletes, &first.marks, NOW + 3 * DAY);
        assert!(early.expired.is_empty());
        assert_eq!(
            early.marks.expired.get("old"),
            Some(&NOW),
            "the first mark stands"
        );

        // A week later: it goes, and only it.
        let later = plan(&deletes, &first.marks, NOW + 7 * DAY);
        assert_eq!(
            later.expired,
            vec![Expired {
                stamp: String::from("old")
            }]
        );
        assert_eq!(later.expired[0].folder(), ".azdrive-trash/old/");
    }

    /// AZ-U16 / rule 11: a clock a year ahead (a dead battery) marks every delete and removes none
    /// of them in that pass; the same clock put right is behind the pass that saw the wrong time,
    /// so it judges nothing - for at most `SEEN_STEP_DAYS`, not for a year - and then marks the
    /// deletes again from its own time: the wrong clock's marks are in its future.
    #[test]
    fn az_u16_a_clock_that_jumped_ahead_removes_nothing_and_a_corrected_one_starts_over() {
        let deletes = [
            delete("yesterday", Some(1), Some(1)),
            delete("last-month", Some(25), Some(25)),
        ];
        // A pass yesterday, then the clock jumps ahead by more than a year.
        let history = Marks {
            seen: NOW - DAY,
            expired: BTreeMap::new(),
            // The pass yesterday knew both deletes.
            first_seen: BTreeMap::from([
                (String::from("yesterday"), NOW - DAY),
                (String::from("last-month"), NOW - 25 * DAY),
            ]),
        };
        let wrong = NOW + 400 * DAY;
        let pass = plan(&deletes, &history, wrong);
        assert!(pass.expired.is_empty(), "nothing goes on the first look");
        assert_eq!(
            pass.marks.expired.len(),
            2,
            "both look old to the wrong clock"
        );
        assert_eq!(
            pass.marks.seen,
            NOW - DAY + SEEN_STEP_DAYS * DAY,
            "the pass moves `seen` by a bounded step, not to the wrong time"
        );

        // The clock is put right: it is behind `seen`, so nothing is judged and the marks stay.
        let corrected = plan(&deletes, &pass.marks, NOW);
        assert!(corrected.behind);
        assert!(corrected.expired.is_empty());
        assert_eq!(corrected.marks, pass.marks);

        // Past `seen` it judges again: the marks of the wrong clock are in its future, so every
        // delete is marked afresh from the true time and none is removed.
        let after = plan(&deletes, &pass.marks, NOW + 62 * DAY);
        assert!(!after.behind);
        assert!(after.expired.is_empty(), "{after:?}");
        assert!(after.marks.expired.values().all(|&at| at == NOW + 62 * DAY));
    }

    /// X-DELTA-AZDRIVE-FILES-34: a delete made by a clock that was behind (an RTC reset to 2000)
    /// has a stamp and a folder time of 2000 - "old" the moment the clock is put right, removed a
    /// week later instead of after 30 days. A delete a pass did not know and whose stamp is before
    /// that pass counts its 30 days from the pass that first saw it.
    #[test]
    fn x_delta_azdrive_files_34_a_delete_made_by_a_clock_that_was_behind_lives_thirty_days_from_sight(
    ) {
        let ancient = Delete {
            stamp: String::from("ancient"),
            stamped: Some(946_684_800),
            modified: Some(946_684_800),
        };
        // A pass yesterday under the right clock; the delete was made after it.
        let history = Marks {
            seen: NOW - DAY,
            ..Marks::default()
        };
        let first = plan(&[ancient.clone()], &history, NOW);
        assert!(first.marks.expired.is_empty(), "{first:?}");
        assert_eq!(first.marks.first_seen.get("ancient"), Some(&NOW));
        let early = plan(&[ancient.clone()], &first.marks, NOW + 29 * DAY);
        assert!(early.marks.expired.is_empty(), "29 days are not 30");
        let marked = plan(&[ancient.clone()], &first.marks, NOW + 30 * DAY);
        assert_eq!(marked.marks.expired.len(), 1);
        assert!(marked.expired.is_empty(), "marked, not removed");
        let gone = plan(&[ancient], &marked.marks, NOW + 37 * DAY);
        assert_eq!(gone.expired.len(), 1, "{gone:?}");
    }

    /// The first pass ever judges a trash that was there before it by the stamps (the trash of the
    /// time before the purge), and a delete a pass knew keeps the time it was first seen.
    #[test]
    fn x_delta_azdrive_files_34_the_first_pass_trusts_the_stamps_and_a_known_delete_keeps_its_time()
    {
        let first = plan(&[delete("old", Some(40), Some(40))], &Marks::default(), NOW);
        assert_eq!(first.marks.first_seen.get("old"), Some(&(NOW - 40 * DAY)));
        assert!(first.marks.expired.contains_key("old"));
        // A delete a pass saw is not counted from the pass after it.
        let known = Marks {
            seen: NOW,
            first_seen: BTreeMap::from([(String::from("seen"), NOW - 3 * DAY)]),
            ..Marks::default()
        };
        let again = plan(&[delete("seen", Some(40), Some(40))], &known, NOW + DAY);
        assert_eq!(again.marks.first_seen.get("seen"), Some(&(NOW - 3 * DAY)));
        assert!(again.marks.expired.is_empty(), "seen 4 days ago: not 30");
    }

    /// The look at the trash is once a day; the start's was the only one.
    #[test]
    fn x_delta_azdrive_files_34_the_trash_is_looked_at_once_a_day() {
        assert!(look_is_due(0, NOW), "never looked");
        assert!(!look_is_due(NOW, NOW + DAY - 1));
        assert!(look_is_due(NOW, NOW + DAY));
        // A clock put back is no reason to look again at once.
        assert!(!look_is_due(NOW, NOW - 5));
    }

    /// What cannot be read is kept: a name that is no stamp, a folder nobody could date, a stamp
    /// in the future - and one such delete does not stop the others being judged.
    #[test]
    fn az_u16_what_cannot_be_read_is_kept_and_does_not_stop_the_others() {
        let deletes = [
            delete("nonsense", None, Some(90)),
            delete("undated", Some(90), None),
            Delete {
                stamp: String::from("future"),
                stamped: Some(NOW + 10 * DAY),
                modified: Some(NOW + 10 * DAY),
            },
            delete("old", Some(90), Some(90)),
        ];
        let pass = plan(&deletes, &Marks::default(), NOW);
        assert_eq!(pass.marks.expired.keys().collect::<Vec<_>>(), vec!["old"]);
        let second = plan(&deletes, &pass.marks, NOW + 8 * DAY);
        assert_eq!(second.expired.len(), 1);
        assert_eq!(second.expired[0].stamp, "old");
    }

    /// A trash with one old delete and one from today, on a real folder: the first pass writes the
    /// mark and removes nothing, the pass a week later removes the old delete's folder with its
    /// files and clears its mark; today's delete is there throughout.
    #[cfg(unix)]
    #[test]
    fn az_u16_the_pass_marks_first_and_removes_the_old_delete_a_week_later() {
        let tmp = TempDir::new("azdrive-trash-pass");
        let root = tmp.path().join("home");
        let marks_file = tmp.path().join("cache").join("trash").join("home.json");
        std::fs::create_dir_all(&root).unwrap();
        let drive = LocalDrive::without_manifest(root.clone());
        let old_stamp = trash_stamp(NOW - 40 * DAY, 1);
        let new_stamp = trash_stamp(NOW - 60, 2);
        drive
            .put(&trash_key("docs/a.txt", &old_stamp), b"old")
            .unwrap();
        drive.put(&trash_key("b.txt", &new_stamp), b"new").unwrap();
        // The file system's own record of the old delete's folder: 40 days ago.
        let old_folder = root.join(".azdrive-trash").join(&old_stamp);
        let then = std::time::UNIX_EPOCH + std::time::Duration::from_secs(NOW - 40 * DAY);
        std::fs::File::open(&old_folder)
            .unwrap()
            .set_modified(then)
            .unwrap();
        let new_folder = root.join(".azdrive-trash").join(&new_stamp);
        let near = std::time::UNIX_EPOCH + std::time::Duration::from_secs(NOW - 60);
        std::fs::File::open(&new_folder)
            .unwrap()
            .set_modified(near)
            .unwrap();

        let first = purge(&drive, &root, &marks_file, NOW).unwrap();
        assert_eq!((first.removed, first.marked), (0, 1), "{first:?}");
        assert!(old_folder.exists(), "the first pass only marks");
        let written = std::fs::read_to_string(&marks_file).unwrap();
        assert!(
            written.contains(&old_stamp) && !written.contains(&new_stamp),
            "{written}"
        );

        let early = purge(&drive, &root, &marks_file, NOW + 3 * DAY).unwrap();
        assert_eq!(early.removed, 0, "{early:?}");
        assert!(old_folder.exists());

        let later = purge(&drive, &root, &marks_file, NOW + 8 * DAY).unwrap();
        assert_eq!(later.removed, 1, "{later:?}");
        assert!(!old_folder.exists(), "the old delete went with its files");
        assert!(new_folder.join("b.txt").exists(), "today's delete is there");
        let after = std::fs::read_to_string(&marks_file).unwrap();
        assert!(
            !after.contains(&old_stamp),
            "its mark went with it: {after}"
        );
    }

    /// A drive that never deleted anything has no trash: no listing, no marks file.
    #[test]
    fn az_u16_a_drive_without_a_trash_is_one_stat_and_writes_nothing() {
        let tmp = TempDir::new("azdrive-trash-none");
        let root = tmp.path().join("home");
        std::fs::create_dir_all(&root).unwrap();
        let marks_file = tmp.path().join("marks.json");
        let drive = LocalDrive::without_manifest(root.clone());
        assert_eq!(
            purge(&drive, &root, &marks_file, NOW).unwrap(),
            Purged::default()
        );
        assert!(!marks_file.exists());
    }

    /// Marks of another format or a newer version are never written over, and then nothing is
    /// removed.
    #[cfg(unix)]
    #[test]
    fn az_u16_marks_of_another_format_stop_the_pass_and_stay_as_they_are() {
        let tmp = TempDir::new("azdrive-trash-foreign");
        let root = tmp.path().join("home");
        std::fs::create_dir_all(&root).unwrap();
        let drive = LocalDrive::without_manifest(root.clone());
        let stamp = trash_stamp(NOW - 90 * DAY, 1);
        drive.put(&trash_key("a.txt", &stamp), b"x").unwrap();
        let folder = root.join(".azdrive-trash").join(&stamp);
        let then = std::time::UNIX_EPOCH + std::time::Duration::from_secs(NOW - 90 * DAY);
        std::fs::File::open(&folder)
            .unwrap()
            .set_modified(then)
            .unwrap();
        let marks_file = tmp.path().join("marks.json");
        std::fs::write(&marks_file, "[1, 2, 3]").unwrap();
        let refused = purge(&drive, &root, &marks_file, NOW + 30 * DAY);
        assert!(refused.is_err(), "{refused:?}");
        assert!(folder.exists());
        assert_eq!(std::fs::read_to_string(&marks_file).unwrap(), "[1, 2, 3]");
    }
}

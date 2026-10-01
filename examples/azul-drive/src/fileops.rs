//! AzDrive's file operations on any drive, as Explorer does them: copy and
//! move (within a drive or across two), with a plan made first (every file
//! under a folder, every name that is taken at the target) so the user
//! decides each conflict - replace, skip, keep both - before a byte moves;
//! delete to a trash folder (local drives) or for good; the names Explorer
//! makes ("a - Copy.txt", "a (2).txt", "New folder (2)"). Everything here
//! blocks: the app runs it on an azul `Thread`. No azul types.

#[cfg(test)]
mod tests {
    use std::{
        path::{Path, PathBuf},
        sync::atomic::{AtomicBool, AtomicU32, Ordering},
    };

    use azul_storage::{Drive, LocalDrive};

    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(what: &str) -> Self {
            static COUNTER: AtomicU32 = AtomicU32::new(0);
            let dir = std::env::temp_dir().join(format!(
                "azdrive-{what}-{}-{}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::SeqCst)
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn seeded(tmp: &TempDir) -> LocalDrive {
        let drive = LocalDrive::new(tmp.path().join("home"));
        drive.put("docs/a.txt", b"alpha").unwrap();
        drive.put("docs/sub/b.txt", b"beta").unwrap();
        drive.put("readme.txt", b"hello").unwrap();
        drive.create_folder("empty/").unwrap();
        drive
    }

    fn item(key: &str) -> SourceItem {
        SourceItem {
            key: key.to_string(),
            is_folder: key.ends_with('/'),
            size: None,
        }
    }

    fn never_cancel() -> AtomicBool {
        AtomicBool::new(false)
    }

    #[test]
    fn explorer_names_a_copy_in_the_same_folder_and_a_kept_duplicate() {
        let taken = |n: &str| ["a.txt", "a - Copy.txt", "b (2).txt", "New folder"].contains(&n);
        assert_eq!(copy_name("a.txt", &taken), "a - Copy (2).txt");
        assert_eq!(copy_name("c.txt", &taken), "c - Copy.txt");
        assert_eq!(copy_name("dir", &|_| false), "dir - Copy");
        assert_eq!(keep_both_name("b.txt", &taken), "b (3).txt");
        assert_eq!(keep_both_name("a.txt", &taken), "a (2).txt");
        assert_eq!(new_name("New folder", &taken), "New folder (2)");
        assert_eq!(
            new_name("New Text Document.txt", &taken),
            "New Text Document.txt"
        );
        assert_eq!(split_extension("archive.tar.gz"), ("archive.tar", ".gz"));
        assert_eq!(split_extension(".env"), (".env", ""));
        assert_eq!(split_extension("noext"), ("noext", ""));
    }

    #[test]
    fn a_name_explorer_refuses_is_refused_with_the_reason() {
        assert!(check_name("notes.txt").is_ok());
        assert!(check_name("").is_err());
        assert!(check_name("   ").is_err());
        assert!(check_name("a/b").is_err());
        assert!(check_name("a:b").is_err());
        assert!(check_name("what?").is_err());
        assert!(check_name("..").is_err());
        let reason = check_name("a|b").unwrap_err();
        assert!(reason.contains('|'), "{reason}");
    }

    #[test]
    fn a_plan_lists_every_file_under_a_folder_and_finds_the_taken_names() {
        let tmp = TempDir::new("plan");
        let home = seeded(&tmp);
        let target = LocalDrive::new(tmp.path().join("other"));
        target.put("in/docs/a.txt", b"old").unwrap();
        let plan = plan_transfer(
            &home,
            &[item("docs/"), item("readme.txt"), item("empty/")],
            &target,
            "in/",
            false,
            TransferKind::Copy,
        )
        .unwrap();
        let targets: Vec<&str> = plan.files.iter().map(|f| f.target_key.as_str()).collect();
        assert_eq!(
            targets,
            vec!["in/docs/a.txt", "in/docs/sub/b.txt", "in/readme.txt"]
        );
        assert!(plan.folders.contains(&"in/docs/".to_string()));
        assert!(
            plan.folders.contains(&"in/empty/".to_string()),
            "{:?}",
            plan.folders
        );
        assert_eq!(plan.conflicts(), vec![0], "in/docs/a.txt is there");
        assert_eq!(plan.total_bytes(), 5 + 4 + 5);
    }

    #[test]
    fn a_copy_into_the_same_folder_is_named_like_explorers_copy() {
        let tmp = TempDir::new("same");
        let home = seeded(&tmp);
        let plan = plan_transfer(
            &home,
            &[item("readme.txt")],
            &home,
            "",
            true,
            TransferKind::Copy,
        )
        .unwrap();
        assert_eq!(plan.files[0].target_key, "readme - Copy.txt");
        assert!(plan.conflicts().is_empty());
        // Moving into the folder it is in does nothing; a folder never goes into itself.
        let noop = plan_transfer(
            &home,
            &[item("readme.txt")],
            &home,
            "",
            true,
            TransferKind::Move,
        )
        .unwrap();
        assert!(noop.files.is_empty() && noop.folders.is_empty());
        assert!(plan_transfer(
            &home,
            &[item("docs/")],
            &home,
            "docs/sub/",
            true,
            TransferKind::Move
        )
        .is_err());
    }

    #[test]
    fn running_a_copy_with_choices_replaces_skips_or_keeps_both() {
        let tmp = TempDir::new("run");
        let home = seeded(&tmp);
        let target = LocalDrive::new(tmp.path().join("other"));
        target.put("docs/a.txt", b"old").unwrap();
        target.put("readme.txt", b"old").unwrap();
        let mut plan = plan_transfer(
            &home,
            &[item("docs/"), item("readme.txt")],
            &target,
            "",
            false,
            TransferKind::Copy,
        )
        .unwrap();
        assert_eq!(plan.conflicts().len(), 2);
        let first = plan.conflicts()[0];
        let second = plan.conflicts()[1];
        plan.choose(first, ConflictChoice::KeepBoth);
        plan.choose(second, ConflictChoice::Skip);
        assert!(plan.unresolved().is_none());
        let mut seen = Vec::new();
        let report = run_transfer(
            &plan,
            &home,
            &target,
            TransferKind::Copy,
            &never_cancel(),
            &mut |p| seen.push(p.clone()),
        );
        assert!(report.failed.is_empty(), "{:?}", report.failed);
        assert_eq!(
            report.done, 2,
            "docs/a.txt kept both, docs/sub/b.txt copied"
        );
        assert_eq!(report.skipped, 1);
        assert_eq!(target.get("docs/a.txt").unwrap(), b"old");
        assert_eq!(target.get("docs/a (2).txt").unwrap(), b"alpha");
        assert_eq!(target.get("docs/sub/b.txt").unwrap(), b"beta");
        assert_eq!(target.get("readme.txt").unwrap(), b"old", "skipped");
        let last = seen.last().expect("progress was reported");
        assert_eq!(last.files_done, 3);
        assert_eq!(
            home.get("docs/a.txt").unwrap(),
            b"alpha",
            "a copy keeps the source"
        );
    }

    #[test]
    fn a_move_removes_the_sources_and_replace_overwrites() {
        let tmp = TempDir::new("move");
        let home = seeded(&tmp);
        let target = LocalDrive::new(tmp.path().join("other"));
        target.put("readme.txt", b"old").unwrap();
        let mut plan = plan_transfer(
            &home,
            &[item("docs/"), item("readme.txt")],
            &target,
            "",
            false,
            TransferKind::Move,
        )
        .unwrap();
        for i in plan.conflicts() {
            plan.choose(i, ConflictChoice::Replace);
        }
        let report = run_transfer(
            &plan,
            &home,
            &target,
            TransferKind::Move,
            &never_cancel(),
            &mut |_| {},
        );
        assert!(report.failed.is_empty(), "{:?}", report.failed);
        assert_eq!(target.get("readme.txt").unwrap(), b"hello");
        assert_eq!(target.get("docs/sub/b.txt").unwrap(), b"beta");
        assert!(home.get("readme.txt").is_err());
        assert!(
            home.local_path("docs/").map_or(true, |p| !p.exists()),
            "the folder moved"
        );
    }

    #[test]
    fn a_move_within_one_drive_renames_and_a_cancel_stops_before_the_next_file() {
        let tmp = TempDir::new("rename");
        let home = seeded(&tmp);
        home.create_folder("archive/").unwrap();
        let plan = plan_transfer(
            &home,
            &[item("docs/")],
            &home,
            "archive/",
            true,
            TransferKind::Move,
        )
        .unwrap();
        let report = run_transfer(
            &plan,
            &home,
            &home,
            TransferKind::Move,
            &never_cancel(),
            &mut |_| {},
        );
        assert!(report.failed.is_empty(), "{:?}", report.failed);
        assert_eq!(home.get("archive/docs/sub/b.txt").unwrap(), b"beta");

        let plan = plan_transfer(
            &home,
            &[item("archive/")],
            &home,
            "",
            true,
            TransferKind::Copy,
        )
        .unwrap();
        let cancelled = AtomicBool::new(true);
        let report = run_transfer(
            &plan,
            &home,
            &home,
            TransferKind::Copy,
            &cancelled,
            &mut |_| {},
        );
        assert!(report.cancelled);
        assert_eq!(report.done, 0);
    }

    #[test]
    fn delete_goes_to_the_trash_folder_and_comes_back_with_undo() {
        let tmp = TempDir::new("trash");
        let home = seeded(&tmp);
        let stamp = trash_stamp(1_759_300_000, 1);
        let moved =
            delete_items(&home, &[item("docs/"), item("readme.txt")], Some(&stamp)).unwrap();
        assert_eq!(moved.len(), 2);
        assert!(home.get("readme.txt").is_err());
        assert_eq!(moved[1].1, trash_key("readme.txt", &stamp));
        assert!(moved[1].1.starts_with(TRASH_FOLDER));
        assert_eq!(
            home.get(&trash_key("docs/sub/b.txt", &stamp)).unwrap(),
            b"beta"
        );
        assert!(is_in_trash(&moved[0].1));
        // Undo: every item back where it was.
        restore_items(&home, &moved).unwrap();
        assert_eq!(home.get("readme.txt").unwrap(), b"hello");
        assert_eq!(home.get("docs/sub/b.txt").unwrap(), b"beta");
        // For good: nothing left anywhere.
        delete_items(&home, &[item("docs/")], None).unwrap();
        assert!(home.local_path("docs/").map_or(true, |p| !p.exists()));
    }

    #[test]
    fn a_transfer_queue_runs_one_job_at_a_time_and_sums_the_progress() {
        let mut queue = TransferQueue::default();
        let a = queue.push("Copying 3 items".to_string());
        let b = queue.push("Uploading photo.jpg".to_string());
        assert_eq!(queue.next_to_start(), Some(a));
        queue.start(a);
        assert_eq!(queue.next_to_start(), None, "one at a time");
        queue.progress(
            a,
            &Progress {
                files_done: 1,
                files_total: 3,
                bytes_done: 50,
                bytes_total: 200,
                current: "a.txt".to_string(),
            },
        );
        assert_eq!(queue.percent(), Some(25.0));
        assert!(
            queue.status_text().contains("Copying 3 items"),
            "{}",
            queue.status_text()
        );
        queue.finish(a, None);
        assert_eq!(queue.next_to_start(), Some(b));
        queue.start(b);
        queue.finish(b, Some("no answer".to_string()));
        assert!(queue.is_idle());
        assert_eq!(queue.failed().len(), 1);
        queue.clear_finished();
        assert!(queue.jobs().is_empty());
    }
}

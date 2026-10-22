//! A folder of N files is read in a few batched system calls, not with a stat for each file,
//! and a listing page stats at most its own entries.

use std::{collections::BTreeMap, fs, ops::ControlFlow, time::UNIX_EPOCH};

use super::TempDir;
use crate::{
    dir_read::{self, calls_on_this_thread, DirItem, ItemKind, ReadCalls},
    Drive, ListRequest, LocalDrive,
};

/// `files` files `f00000.txt`... (file `i` holds `i % 7` bytes), a folder and a link to a
/// file.
fn folder_of(tmp: &TempDir, files: usize) {
    for i in 0..files {
        fs::write(tmp.path().join(format!("f{i:05}.txt")), vec![b'x'; i % 7]).unwrap();
    }
    fs::create_dir(tmp.path().join("sub")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(tmp.path().join("f00001.txt"), tmp.path().join("link.txt")).unwrap();
}

fn calls_of(read: impl FnOnce()) -> ReadCalls {
    let before = calls_on_this_thread();
    read();
    let after = calls_on_this_thread();
    ReadCalls {
        batch_reads: after.batch_reads - before.batch_reads,
        stats: after.stats - before.stats,
    }
}

#[test]
fn a_folder_of_3000_files_is_read_in_batches_without_a_stat_for_each() {
    let tmp = TempDir::new("dir-read-batches");
    folder_of(&tmp, 3000);
    let mut batches = 0;
    let mut items: Vec<DirItem> = Vec::new();
    let calls = calls_of(|| {
        dir_read::read_dir_items(tmp.path(), &mut |batch| {
            batches += 1;
            items.extend(batch);
            ControlFlow::Continue(())
        })
        .unwrap();
    });
    assert_eq!(items.len(), 3002, "the files, the folder and the link");
    assert!(batches >= 2, "the entries arrive in batches, not all at the end");
    // Only the link is stat'ed (to see what it points at).
    assert_eq!(calls.stats, if cfg!(unix) { 1 } else { 0 });
    if cfg!(target_os = "macos") {
        assert!(
            calls.batch_reads <= 3000 / 100 + 2,
            "one getattrlistbulk call reads a batch of entries: {calls:?}"
        );
        assert!(items.iter().all(DirItem::has_stat), "names, sizes and dates in one read");
    }
}

#[test]
fn what_the_batched_read_says_is_what_a_stat_says() {
    let tmp = TempDir::new("dir-read-agrees");
    folder_of(&tmp, 40);
    let items: BTreeMap<String, DirItem> = dir_read::read_dir_all(tmp.path())
        .unwrap()
        .into_iter()
        .map(|i| (i.name.clone(), i))
        .collect();
    for entry in fs::read_dir(tmp.path()).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().into_string().unwrap();
        let meta = fs::metadata(entry.path()).unwrap();
        let item = &items[&name];
        assert_eq!(item.kind == ItemKind::Folder, meta.is_dir(), "{name}");
        if let Some(size) = item.size {
            assert_eq!(size, meta.len(), "{name}");
        }
        if let Some(modified) = item.modified {
            let secs = meta.modified().unwrap().duration_since(UNIX_EPOCH).unwrap().as_secs();
            assert_eq!(modified, secs, "{name}");
        }
        #[cfg(unix)]
        if let Some(inode) = item.inode {
            // A link's inode is its own; its kind, size and date are what it points at.
            let own = fs::symlink_metadata(entry.path()).unwrap();
            assert_eq!(inode, std::os::unix::fs::MetadataExt::ino(&own), "{name}");
        }
    }
    assert_eq!(items.len(), 42);
    assert_eq!(items["sub"].size, None, "a folder has no size");
    #[cfg(unix)]
    assert_eq!(items["link.txt"].size, Some(1), "a link reads as the file it points at");
}

#[test]
fn a_read_stops_when_asked() {
    let tmp = TempDir::new("dir-read-stop");
    folder_of(&tmp, 3000);
    let mut batches = 0;
    dir_read::read_dir_items(tmp.path(), &mut |_| {
        batches += 1;
        ControlFlow::Break(())
    })
    .unwrap();
    assert_eq!(batches, 1);
}

#[test]
fn a_listing_page_of_a_large_folder_stats_at_most_its_own_entries() {
    let tmp = TempDir::new("dir-read-pages");
    folder_of(&tmp, 3000);
    let drive = LocalDrive::without_manifest(tmp.path().to_path_buf());
    let mut request = ListRequest::folder("").with_max_keys(500);
    let mut listed = 0;
    let mut pages = 0;
    loop {
        let mut page = None;
        let calls = calls_of(|| page = Some(drive.list(&request).unwrap()));
        let page = page.unwrap();
        pages += 1;
        listed += page.objects.len() + page.folders.len();
        assert!(
            calls.stats <= page.objects.len() + 1,
            "page {pages}: {} stats for {} entries",
            calls.stats,
            page.objects.len()
        );
        if cfg!(target_os = "macos") {
            assert!(calls.stats <= 1, "the batched read carries the sizes: {calls:?}");
        }
        match page.next {
            Some(next) => request = request.with_continuation(next),
            None => break,
        }
    }
    assert_eq!(listed, 3002);
    assert_eq!(pages, 7);
}

/// The numbers for the notes (`--ignored --nocapture`): a folder of 10,000 files read the old
/// way (read_dir and a stat per entry, what every listing page did) and the batched way.
#[test]
#[ignore = "a measurement, not a law"]
fn measure_a_folder_of_10000_files() {
    let tmp = TempDir::new("dir-read-measure");
    folder_of(&tmp, 10_000);
    let started = std::time::Instant::now();
    let mut stats = 0;
    for entry in fs::read_dir(tmp.path()).unwrap() {
        let _ = fs::metadata(entry.unwrap().path());
        stats += 1;
    }
    let old = started.elapsed();
    let started = std::time::Instant::now();
    let mut first = None;
    let mut items = 0;
    let calls = calls_of(|| {
        dir_read::read_dir_items(tmp.path(), &mut |batch| {
            first.get_or_insert_with(|| started.elapsed());
            items += batch.len();
            ControlFlow::Continue(())
        })
        .unwrap();
    });
    let new = started.elapsed();
    println!(
        "DIR_READ_10000 old: read_dir + {stats} stats {old:?}; batched: {items} items in \
         {new:?} (first batch after {first:?}), {calls:?}"
    );
}

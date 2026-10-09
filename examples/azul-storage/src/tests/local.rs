use super::TempDir;
use crate::{ByteRange, Drive, DriveError, ListRequest, LocalDrive, Precondition};

fn keys(page: &crate::ListPage) -> Vec<&str> {
    page.objects.iter().map(|o| o.key.as_str()).collect()
}

fn seeded(tmp: &TempDir) -> LocalDrive {
    let drive = LocalDrive::new(tmp.path());
    drive.put("mail/inbox/0001.eml", b"first").unwrap();
    drive.put("mail/inbox/0002.eml", b"second!").unwrap();
    drive.put("mail/sent/0003.eml", b"third").unwrap();
    drive.put("readme.txt", b"hello").unwrap();
    drive
}

#[test]
fn put_then_get_round_trips_bytes_under_the_root() {
    let tmp = TempDir::new("local-roundtrip");
    let drive = LocalDrive::new(tmp.path());
    drive.put("docs/a.txt", b"azul").unwrap();
    assert_eq!(drive.get("docs/a.txt").unwrap(), b"azul");
    assert_eq!(
        std::fs::read(tmp.path().join("docs").join("a.txt")).unwrap(),
        b"azul"
    );
    // A second put replaces the object.
    drive.put("docs/a.txt", b"storage").unwrap();
    assert_eq!(drive.get("docs/a.txt").unwrap(), b"storage");
}

#[test]
fn list_with_a_delimiter_shows_the_folders_and_files_of_one_level() {
    let tmp = TempDir::new("local-list");
    let drive = seeded(&tmp);

    let root = drive.list(&ListRequest::folder("")).unwrap();
    assert_eq!(root.folders, vec!["mail/".to_string()]);
    assert_eq!(keys(&root), vec!["readme.txt"]);
    assert_eq!(root.objects[0].size, 5);
    assert!(root.objects[0].modified.is_some());
    assert_eq!(root.next, None);

    let mail = drive.list(&ListRequest::folder("mail/")).unwrap();
    assert_eq!(
        mail.folders,
        vec!["mail/inbox/".to_string(), "mail/sent/".to_string()]
    );
    assert!(mail.objects.is_empty());

    let inbox = drive.list(&ListRequest::folder("mail/inbox/")).unwrap();
    assert!(inbox.folders.is_empty());
    assert_eq!(
        keys(&inbox),
        vec!["mail/inbox/0001.eml", "mail/inbox/0002.eml"]
    );
    assert_eq!(inbox.objects[1].size, 7);
}

#[test]
fn a_partial_name_prefix_lists_only_the_matching_entries() {
    let tmp = TempDir::new("local-partial");
    let drive = seeded(&tmp);
    let page = drive.list(&ListRequest::folder("mail/inbox/0002")).unwrap();
    assert_eq!(keys(&page), vec!["mail/inbox/0002.eml"]);
}

#[test]
fn list_pages_with_a_continuation_token() {
    let tmp = TempDir::new("local-pages");
    let drive = LocalDrive::new(tmp.path());
    for i in 0..5 {
        drive.put(&format!("bulk/{i}.txt"), b"x").unwrap();
    }
    let mut seen = Vec::new();
    let mut request = ListRequest::folder("bulk/").with_max_keys(2);
    let mut pages = 0;
    loop {
        let page = drive.list(&request).unwrap();
        pages += 1;
        assert!(page.objects.len() <= 2);
        seen.extend(page.objects.iter().map(|o| o.key.clone()));
        match page.next {
            Some(token) => request = request.with_continuation(token),
            None => break,
        }
    }
    assert_eq!(pages, 3);
    assert_eq!(
        seen,
        vec![
            "bulk/0.txt",
            "bulk/1.txt",
            "bulk/2.txt",
            "bulk/3.txt",
            "bulk/4.txt"
        ]
    );
}

#[test]
fn list_without_a_delimiter_walks_the_whole_prefix() {
    let tmp = TempDir::new("local-recursive");
    let drive = seeded(&tmp);
    let page = drive.list(&ListRequest::recursive("mail/")).unwrap();
    assert!(page.folders.is_empty());
    assert_eq!(
        keys(&page),
        vec![
            "mail/inbox/0001.eml",
            "mail/inbox/0002.eml",
            "mail/sent/0003.eml"
        ]
    );
}

#[test]
fn a_missing_folder_lists_as_empty() {
    let tmp = TempDir::new("local-missing");
    let drive = LocalDrive::new(tmp.path());
    let page = drive.list(&ListRequest::folder("nothing/here/")).unwrap();
    assert!(page.folders.is_empty() && page.objects.is_empty() && page.next.is_none());
}

#[test]
fn get_range_returns_the_inclusive_byte_range() {
    let tmp = TempDir::new("local-range");
    let drive = LocalDrive::new(tmp.path());
    drive.put("digits.txt", b"0123456789").unwrap();
    assert_eq!(
        drive
            .get_range("digits.txt", ByteRange::new(2, Some(4)))
            .unwrap(),
        b"234"
    );
    assert_eq!(
        drive
            .get_range("digits.txt", ByteRange::new(7, None))
            .unwrap(),
        b"789"
    );
    assert_eq!(
        drive
            .get_range("digits.txt", ByteRange::new(8, Some(100)))
            .unwrap(),
        b"89"
    );
    assert!(matches!(
        drive.get_range("digits.txt", ByteRange::new(10, None)),
        Err(DriveError::InvalidRange { .. })
    ));
}

#[test]
fn head_reports_the_size_and_modified_time() {
    let tmp = TempDir::new("local-head");
    let drive = LocalDrive::new(tmp.path());
    drive.put("a/b.bin", &[7u8; 1234]).unwrap();
    let info = drive.head("a/b.bin").unwrap();
    assert_eq!(info.key, "a/b.bin");
    assert_eq!(info.size, 1234);
    assert!(info.modified.is_some());
    assert_eq!(info.name(), "b.bin");
}

#[test]
fn delete_removes_the_file_and_a_missing_key_is_not_an_error() {
    let tmp = TempDir::new("local-delete");
    let drive = LocalDrive::new(tmp.path());
    drive.put("a.txt", b"x").unwrap();
    drive.delete("a.txt").unwrap();
    assert!(!tmp.path().join("a.txt").exists());
    drive.delete("a.txt").unwrap();
}

#[test]
fn a_missing_key_is_not_found() {
    let tmp = TempDir::new("local-notfound");
    let drive = LocalDrive::new(tmp.path());
    assert!(matches!(
        drive.get("nope.txt"),
        Err(DriveError::NotFound { .. })
    ));
    assert!(matches!(
        drive.head("nope.txt"),
        Err(DriveError::NotFound { .. })
    ));
    std::fs::create_dir_all(tmp.path().join("folder")).unwrap();
    assert!(matches!(
        drive.get("folder"),
        Err(DriveError::NotFound { .. })
    ));
}

#[test]
fn path_traversal_keys_never_touch_the_disk_outside_the_root() {
    let tmp = TempDir::new("local-traversal");
    let root = tmp.path().join("root");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(tmp.path().join("outside.txt"), b"secret").unwrap();
    let drive = LocalDrive::new(&root);

    let invalid = |r: Result<(), DriveError>| matches!(r, Err(DriveError::InvalidKey { .. }));
    assert!(invalid(drive.get("../outside.txt").map(|_| ())));
    assert!(invalid(drive.head("../outside.txt").map(|_| ())));
    assert!(invalid(
        drive
            .get_range("../outside.txt", ByteRange::new(0, None))
            .map(|_| ())
    ));
    assert!(invalid(drive.put("../planted.txt", b"x")));
    assert!(invalid(drive.put("a/../../planted.txt", b"x")));
    assert!(invalid(drive.delete("../outside.txt")));
    assert!(invalid(drive.get("/etc/hosts").map(|_| ())));
    assert!(invalid(drive.list(&ListRequest::folder("../")).map(|_| ())));

    assert!(!tmp.path().join("planted.txt").exists());
    assert_eq!(
        std::fs::read(tmp.path().join("outside.txt")).unwrap(),
        b"secret"
    );
}

/// Yields `left` bytes of `x`, then fails: a source that breaks in the middle.
struct BreaksAfter {
    left: usize,
}

impl std::io::Read for BreaksAfter {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.left == 0 {
            return Err(std::io::Error::other("the source broke"));
        }
        let n = buf.len().min(self.left);
        buf[..n].fill(b'x');
        self.left -= n;
        Ok(n)
    }
}

/// The names in `dir`, sorted.
fn names_in(dir: &std::path::Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn put_from_streams_a_reader_into_the_file_and_records_it_in_the_manifest() {
    let tmp = TempDir::new("local-put-from");
    let drive = LocalDrive::new(tmp.path());
    let body: Vec<u8> = (0..3_000_000u32).map(|i| (i % 251) as u8).collect();
    let written = drive.put_from("big/file.bin", &mut &body[..]).unwrap();
    assert_eq!(written, body.len() as u64);
    assert_eq!(drive.get("big/file.bin").unwrap(), body);
    assert_eq!(
        drive.manifest().unwrap().get("big/file.bin").map(|e| e.size),
        Some(body.len() as u64)
    );
    assert_eq!(names_in(&tmp.path().join("big")), vec!["file.bin"]);
}

#[test]
fn a_put_from_whose_reader_breaks_leaves_the_old_file_and_no_temporary_one() {
    let tmp = TempDir::new("local-put-from-broken");
    let drive = LocalDrive::new(tmp.path());
    drive.put("docs/a.txt", b"old").unwrap();
    let error = drive
        .put_from("docs/a.txt", &mut BreaksAfter { left: 100_000 })
        .unwrap_err();
    assert!(matches!(error, DriveError::Io(_)), "{error:?}");
    assert_eq!(drive.get("docs/a.txt").unwrap(), b"old");
    assert_eq!(names_in(&tmp.path().join("docs")), vec!["a.txt"]);
}

#[test]
fn a_folder_on_disk_cannot_write_conditionally() {
    let tmp = TempDir::new("local-put-if");
    let drive = LocalDrive::new(tmp.path());
    assert!(matches!(
        drive.put_if("a.txt", b"x", &Precondition::Absent),
        Err(DriveError::Unsupported(_))
    ));
    assert!(!tmp.path().join("a.txt").exists());
}
